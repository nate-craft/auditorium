use color_eyre::{eyre::Error, Result};
use crossterm::ExecutableCommand;
use ffprobe::FfProbeError;
use random_number::rand::{seq::SliceRandom, thread_rng};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, BorderType, Borders, TableState},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, min},
    fs::File,
    io::{self, BufReader, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    mpv::MpvCommand,
    widget::{widget_all, widget_history, widget_playing, widget_up_next},
};

mod input;
mod mpv;
mod utilities;
mod widget;

const MUSIC_DIR: &'static str = "/home/nate/Music/";
const CACHE_FILE: &'static str = "/home/nate/.cache/music-player-cache.json";
const MPV_SOCKET: &'static str = "/tmp/mpv-socket";
const SECONDARY_COLOR: Color = Color::Rgb(238, 149, 158);

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Song {
    title: String,
    genre: String,
    artist: String,
    path: PathBuf,
}

struct Songs {
    songs_library: Vec<Song>,
    songs_next: Vec<usize>,
    songs_history: Vec<usize>,
    active: Option<Child>,
}

#[derive(PartialEq, Eq)]
enum NavState {
    Player,
    UpNext(TableState),
    Library(TableState),
    Exit,
}

#[derive(PartialEq, Eq)]
enum SongLoadingState {
    Backward,
    Forward,
}

struct App {
    songs: Songs,
    nav_state: NavState,
    song_state: SongLoadingState,
    paused: bool,
    click_position: Option<Position>,
}

enum Message {
    None,
    Exit,
    Pause(bool),
    SongNext,
    SongPrevious,
    NavStateNext,
    NavStatePrev,
    NavStateInnerNext(bool, usize),
    NavStateInnerPrev(bool, usize),
    MoveSong,
    DeleteNextUp(usize),
    PlayAll,
}

impl NavState {
    fn rows_per_skip(is_single_row: bool) -> usize {
        if is_single_row {
            1
        } else {
            10
        }
    }

    fn event_list_up(&mut self, is_single: bool, num_entries: usize) {
        if let NavState::UpNext(state) | NavState::Library(state) = self {
            let skips = Self::rows_per_skip(is_single);

            if num_entries > 0 {
                state.select(Some(
                    max(0i32, state.selected().unwrap_or(0) as i32 - skips as i32) as usize,
                ));
            }
        }
    }

    fn event_list_down(&mut self, is_single: bool, num_entries: usize) {
        if let NavState::UpNext(state) | NavState::Library(state) = self {
            let skips = Self::rows_per_skip(is_single);

            if num_entries > 0 {
                state.select(Some(min(
                    max(num_entries - 1, 0),
                    state.selected().unwrap_or(0) + skips,
                )));
            }
        }
    }

    fn as_stateful_default(&self, app: &App) -> NavState {
        match self {
            NavState::Player => NavState::Player,
            NavState::UpNext(_) => {
                if !app.songs.songs_next.is_empty() {
                    NavState::UpNext(TableState::default().with_selected(Some(0)))
                } else {
                    NavState::UpNext(TableState::default())
                }
            }
            NavState::Library(_) => {
                if !app.songs.songs_library.is_empty() {
                    NavState::Library(TableState::default().with_selected(Some(0)))
                } else {
                    NavState::Library(TableState::default())
                }
            }
            NavState::Exit => NavState::Exit,
        }
    }
}

impl Song {
    fn new(file_name: &Path) -> Result<Song, FfProbeError> {
        let probe = ffprobe::ffprobe(&file_name)?;
        let mut title: String = "Unknown".to_owned();
        let mut genre: String = "Unknown".to_owned();
        let mut artist: String = "Unknown".to_owned();
        let path = file_name.to_owned();

        probe.format.tags.map(|tags| {
            tags.extra.get("title").map(|title_inner| {
                title_inner.as_str().map(|title_inner| {
                    title = title_inner.to_owned();
                });
            });
            tags.extra.get("genre").map(|genre_inner| {
                genre_inner.as_str().map(|genre_inner| {
                    genre = genre_inner.to_owned();
                });
            });
            tags.extra.get("artist").map(|artist_inner| {
                artist_inner.as_str().map(|author_inner| {
                    artist = author_inner.to_owned();
                });
            })
        });

        return Ok(Song {
            title,
            genre,
            artist,
            path,
        });
    }

    fn play_single(&self) -> Result<Child, io::Error> {
        Command::new("mpv")
            .arg("--no-video")
            .arg("--no-resume-playback")
            .arg("--msg-level=all=no")
            .arg("--no-terminal")
            .arg("--quiet")
            .arg(format!("{}{}", "--input-ipc-server=", MPV_SOCKET))
            .arg(self.path.to_string_lossy().into_owned())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

impl Songs {
    pub fn new(cache_path: &Path, music_path: PathBuf) -> Songs {
        let mut song_map = Vec::new();

        if cache_path.exists() {
            if let Ok(file) = File::open(cache_path) {
                let songs_cached: Result<Vec<Song>, _> =
                    serde_json::from_reader(BufReader::new(file));
                if let Ok(song_cached) = songs_cached {
                    song_map = song_cached;
                }
            }
        }

        let mut songs = Songs {
            songs_library: song_map,
            songs_next: Vec::new(),
            songs_history: Vec::new(),
            active: None,
        };

        if songs.songs_library.is_empty() {
            println!(
                "{}",
                "Loading music from Music directory. This may take a moment...".green()
            );

            if let Some(errors) = songs.load_songs(&music_path) {
                errors.iter().for_each(|e| eprintln!("{}", e));
            }

            if let Ok(json) = serde_json::to_string(&songs.songs_library) {
                if let Ok(mut cache_file) = File::create_new(cache_path) {
                    cache_file.write_all(json.as_bytes()).unwrap();
                }
            }
        }

        songs
            .songs_library
            .sort_by(|first, second| first.artist.cmp(&second.artist));

        songs
    }

    fn load_songs(&mut self, root_dir: &Path) -> Option<Vec<FfProbeError>> {
        let (send, rec) = mpsc::channel::<Result<Song, FfProbeError>>();
        let (send_threads, rec_threads) = mpsc::channel::<JoinHandle<()>>();
        let mut errors = Vec::new();
        let mut num_started = 0;
        let mut num_ended = 0;
        Self::add_from_dir(send.clone(), send_threads.clone(), root_dir);

        loop {
            while let Ok(result) = rec.try_recv() {
                match result {
                    Ok(song) => {
                        self.songs_library.push(song);
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
                num_ended += 1;
            }

            while let Ok(_) = rec_threads.try_recv() {
                num_started += 1;
            }

            if num_ended >= num_started {
                if errors.is_empty() {
                    return None;
                } else {
                    return Some(errors);
                }
            }
        }
    }

    fn add_from_dir(
        send: Sender<Result<Song, FfProbeError>>,
        send_threads: Sender<JoinHandle<()>>,
        dir: &Path,
    ) {
        if let Ok(child) = dir.read_dir() {
            child.for_each(|child_result| {
                let Ok(child) = child_result else {
                    return;
                };

                let child_path = child.path();
                if child_path.is_dir() {
                    Self::add_from_dir(send.clone(), send_threads.clone(), &child_path);
                } else {
                    let send_clone = send.clone();
                    send_threads
                        .send(thread::spawn(move || match Song::new(&child_path) {
                            Ok(song) => send_clone.clone().send(Ok(song)).unwrap(),
                            Err(e) => send_clone.clone().send(Err(e)).unwrap(),
                        }))
                        .unwrap();
                }
            });
        }
    }

    fn current_song_index(&self) -> Option<usize> {
        self.songs_next.get(0).copied()
    }

    fn current_song(&self) -> Option<&Song> {
        self.current_song_index()
            .map(|index| self.songs_library.get(index))
            .flatten()
    }

    fn next_playing(&self) -> Vec<&Song> {
        if self.songs_next.len() <= 1 {
            Vec::new()
        } else {
            self.songs_next[1..]
                .iter()
                .filter_map(|index| self.songs_library.get(*index))
                .collect()
        }
    }

    fn next(&mut self, song_state: &SongLoadingState) {
        match song_state {
            SongLoadingState::Backward => {
                if let Some(previous) = self.last_played_index() {
                    self.songs_history.remove(self.songs_history.len() - 1);
                    self.songs_next.insert(0, previous);
                }
            }
            SongLoadingState::Forward => {
                if !self.songs_next.is_empty() {
                    self.current_song_index()
                        .map(|current| self.songs_history.push(current));
                    self.songs_next.remove(0);
                }
            }
        }
    }

    fn kill_current(&mut self) {
        if let Some(active) = &mut self.active {
            let _ = active.kill();
        }
    }

    fn song_is_active(&mut self) -> bool {
        if let Some(active) = &mut self.active {
            if let Ok(status) = active.try_wait() {
                return status.is_none();
            }
        }
        return false;
    }

    fn last_played_index(&self) -> Option<usize> {
        if !self.songs_history.is_empty() {
            self.songs_history
                .get(self.songs_history.len() - 1)
                .copied()
        } else {
            None
        }
    }

    fn last_played(&self) -> Option<&Song> {
        self.last_played_index()
            .map(|index| self.songs_library.get(index))
            .flatten()
    }

    fn push_songs_back(&mut self, song_indicies: Range<usize>) {
        self.songs_next.extend(song_indicies.into_iter());
    }

    fn shuffle(&mut self) {
        self.songs_next.shuffle(&mut thread_rng());
    }

    fn push_song_back(&mut self, selected: usize) {
        self.songs_next.push(selected);
    }

    fn push_song_front(&mut self, selected: usize) {
        self.songs_next.insert(1, selected);
    }
}

impl App {
    fn new(songs: Songs) -> App {
        App {
            songs,
            nav_state: NavState::Player,
            song_state: SongLoadingState::Forward,
            paused: false,
            click_position: None,
        }
    }

    fn handle_events(&mut self) -> Result<()> {
        match input::handle_events(self) {
            Message::None => {}
            Message::Exit => {
                self.exit();
                return Ok(());
            }
            Message::Pause(paused) => {
                MpvCommand::TogglePause.run()?;
                self.paused = paused;
            }
            Message::SongNext => {
                self.songs.kill_current();
                self.song_state = SongLoadingState::Forward;
            }
            Message::SongPrevious => {
                self.songs.kill_current();
                self.song_state = SongLoadingState::Backward;
            }
            Message::NavStateNext => {
                self.next_nav_state();
            }
            Message::NavStatePrev => {
                self.previous_nav_state();
            }
            Message::NavStateInnerNext(is_single, elements) => {
                self.nav_state.event_list_down(is_single, elements);
            }
            Message::NavStateInnerPrev(is_single, elements) => {
                self.nav_state.event_list_up(is_single, elements);
            }
            Message::DeleteNextUp(selected) => {
                self.songs.songs_next.remove(selected);
            }
            Message::PlayAll => {
                self.songs
                    .push_songs_back(0..self.songs.songs_library.len() - 1);
                self.songs.shuffle();
            }
            Message::MoveSong => match &self.nav_state {
                NavState::UpNext(table_state) => {
                    table_state.selected().map(|selected| {
                        let selected = selected + 1;
                        self.songs
                            .songs_next
                            .get(selected)
                            .map(|library_index| *library_index)
                            .map(|library_index| {
                                self.songs.songs_next.remove(selected);
                                self.songs.push_song_front(library_index);
                                self.songs.kill_current();
                            });
                    });
                }
                NavState::Library(table_state) => {
                    if let Some(selected) = table_state.selected() {
                        self.songs.push_song_back(selected);
                    };
                }
                _ => {}
            },
        }

        return Ok(());
    }

    fn handle_song_state(&mut self) -> Result<()> {
        match self.songs.active.as_mut() {
            None => {
                match self.song_state {
                    SongLoadingState::Backward => {
                        if let Some(previous) = self.songs.last_played_index() {
                            self.songs
                                .songs_history
                                .remove(self.songs.songs_history.len() - 1);
                            self.songs.songs_next.insert(0, previous);
                        }
                    }
                    SongLoadingState::Forward => {
                        self.songs.active = self
                            .songs
                            .current_song()
                            .map(|process| process.play_single().unwrap());
                    }
                }

                self.song_state = SongLoadingState::Forward;
            }
            Some(active) => {
                let status = active.try_wait()?;
                if status.is_some() {
                    self.songs.next(&self.song_state);
                    self.song_state = SongLoadingState::Forward;
                    self.paused = false;
                    self.songs.active = self
                        .songs
                        .current_song()
                        .map(|process| process.play_single().unwrap());
                }
            }
        }

        return Ok(());
    }

    fn draw(&mut self, frame: &mut Frame) {
        let outer_border = Block::bordered()
            .title(" Music ")
            .title_alignment(Alignment::Center)
            .title_style(Style::default().bold())
            .title_bottom("| [Tab] Nav |")
            .borders(Borders::all());
        let outer_area = outer_border.inner(frame.area());

        let [left, right] = Layout::horizontal([Constraint::Fill(1); 2]).areas(outer_area);
        let [left_top, left_middle, left_bottom] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .areas(left);

        if self.click_position_matches_rect(left_top) {
            self.set_nav_state(NavState::Player);
        } else if self.click_position_matches_rect(left_middle) {
            self.set_nav_state(NavState::UpNext(TableState::default()).as_stateful_default(&self));
        } else if self.click_position_matches_rect(right) {
            self.set_nav_state(NavState::Library(TableState::default()).as_stateful_default(&self));
        }

        let widget_playing = widget_playing::widget_playing(self);
        let widget_history = widget_history::widget_history(self);
        let mut widget_next = widget_up_next::build(self, left_middle);
        let mut widget_all = widget_all::build(self, right);

        frame.render_widget(outer_border, frame.area());
        frame.render_widget(widget_history, left_bottom);
        frame.render_widget(widget_playing, left_top);

        if let NavState::UpNext(state) = &mut self.nav_state {
            widget_next = widget_next.block(
                Block::bordered()
                    .border_style(Style::new().fg(SECONDARY_COLOR))
                    .border_type(BorderType::Thick)
                    .title_top(" Up Next ")
                    .title_bottom(" | [j/k] Up/Down | [Enter] Play Now | [Backspace] Remove | ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_stateful_widget(widget_next, left_middle, state);
        } else {
            widget_next = widget_next.block(
                Block::bordered()
                    .title_top(" Up Next ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(widget_next, left_middle);
        }

        if let NavState::Library(state) = &mut self.nav_state {
            widget_all = widget_all.block(
                Block::bordered()
                    .border_style(Style::new().fg(SECONDARY_COLOR))
                    .title(" Library ")
                    .border_type(BorderType::Thick)
                    .title_bottom(" | [j/k] Up/Down | [Enter] Play Later | [a] Play All | ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_stateful_widget(widget_all, right, state);
        } else {
            widget_all = widget_all.block(
                Block::bordered()
                    .title(" Library ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(widget_all, right);
        }
    }

    fn exit(&mut self) {
        self.set_nav_state(NavState::Exit);
        self.songs.kill_current();
    }

    fn set_click_position(&mut self, position: Position) {
        self.click_position = Some(position);
    }

    fn click_position_matches_rect(&self, rect: Rect) -> bool {
        if let Some(click) = self.click_position {
            rect.contains(click)
        } else {
            false
        }
    }

    fn reset_click_position(&mut self) {
        self.click_position = None;
    }

    fn set_nav_state(&mut self, state: NavState) {
        self.nav_state = state;
    }

    fn previous_nav_state(&mut self) {
        match self.nav_state {
            NavState::Player => {
                self.nav_state = NavState::Library(TableState::default()).as_stateful_default(self);
            }
            NavState::UpNext(_) => {
                self.nav_state = NavState::Player;
            }
            NavState::Library(_) => {
                self.nav_state = NavState::UpNext(TableState::default()).as_stateful_default(self);
            }
            NavState::Exit => {}
        }
    }

    fn next_nav_state(&mut self) {
        match self.nav_state {
            NavState::Player => {
                self.nav_state = NavState::UpNext(TableState::default()).as_stateful_default(self);
            }
            NavState::UpNext(_) => {
                self.nav_state = NavState::Library(TableState::default()).as_stateful_default(self);
            }
            NavState::Library(_) => {
                self.nav_state = NavState::Player;
            }
            NavState::Exit => {}
        }
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let music_dir_path = PathBuf::from(MUSIC_DIR);
    let cache_path = Path::new(CACHE_FILE);
    let songs = Songs::new(cache_path, music_dir_path);
    let app = Arc::new(Mutex::new(App::new(songs)));
    let mut terminal = ratatui::init();

    io::stdout().execute(crossterm::event::EnableMouseCapture)?;
    terminal.clear().unwrap();

    let app_ref = app.clone();
    let handle_draw: JoinHandle<Result<()>> = thread::spawn(move || loop {
        let Ok(mut app) = app_ref.try_lock() else {
            continue;
        };

        if app.nav_state == NavState::Exit {
            return Ok(());
        }

        if let Err(err) = terminal
            .draw(|frame| app.draw(frame))
            .map_err(|err| Error::new(err))
        {
            app.exit();
            return Err(err);
        }

        thread::sleep(Duration::from_millis(20));
    });

    let app_ref = app.clone();
    let handle: JoinHandle<Result<()>> = thread::spawn(move || loop {
        let Ok(mut app) = app_ref.try_lock() else {
            continue;
        };

        if app.nav_state == NavState::Exit {
            return Ok(());
        }

        if let Err(err) = app.handle_events() {
            app.exit();
            return Err(err);
        }

        if let Err(err) = app.handle_song_state() {
            app.exit();
            return Err(err);
        }

        thread::sleep(Duration::from_millis(5));
    });

    loop {
        if handle.is_finished() {
            ratatui::restore();
            return handle.join().unwrap();
        }

        if handle_draw.is_finished() {
            ratatui::restore();
            return handle_draw.join().unwrap();
        }

        thread::sleep(Duration::from_millis(200));
    }
}
