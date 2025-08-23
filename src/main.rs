use color_eyre::Result;
use crossterm::ExecutableCommand;
use random_number::rand::{seq::SliceRandom, thread_rng};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, BorderType, Borders, TableState},
    DefaultTerminal, Frame,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, min},
    fs::File,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
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
    fn new(file_name: &Path) -> Option<Song> {
        match ffprobe::ffprobe(&file_name) {
            Ok(probe) => {
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
                return Some(Song {
                    title,
                    genre,
                    artist,
                    path,
                });
            }
            Err(e) => {
                eprintln!("{}", e);
                return None;
            }
        }
    }

    fn play_single(&self) -> Result<Child, std::io::Error> {
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
            songs.add_from_dir(&music_path);
        }

        songs
    }

    fn add_from_dir(&mut self, dir: &Path) {
        if let Ok(child) = dir.read_dir() {
            child.for_each(|child_result| {
                if let Ok(child) = child_result {
                    let child_path = child.path();
                    if child_path.is_dir() {
                        self.add_from_dir(&child_path);
                    } else if let Some(song) = Song::new(&child_path) {
                        self.songs_library.push(song);
                    }
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

    fn run(&mut self, terminal: &mut DefaultTerminal, tick: &u64) -> Result<()> {
        //TODO: draw on separate thread and add actual time based ticks
        if tick % 10000 == 0 {
            terminal.draw(|frame| self.draw(frame))?;
        }

        match input::handle_events(self) {
            Message::None => {}
            Message::Exit => self.exit(),
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
                for i in 0..self.songs.songs_library.len() - 1 {
                    self.songs.songs_next.push(i);
                }
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
                                self.songs.songs_next.insert(1, library_index);
                                self.songs.kill_current();
                            });
                    });
                }
                NavState::Library(table_state) => {
                    if let Some(selected) = table_state.selected() {
                        self.songs.songs_next.push(selected);
                    };
                }
                _ => {}
            },
        }

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
                if let Ok(status) = active.try_wait() {
                    if status.is_some() {
                        self.songs.next(&self.song_state);
                        self.song_state = SongLoadingState::Forward;
                        self.paused = false;
                        self.songs.active = self
                            .songs
                            .current_song()
                            .map(|process| process.play_single().unwrap());
                    }
                    //TODO: handle error on running
                }
            }
        }

        Ok(())
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
                    .border_style(Style::new().fg(Color::Yellow))
                    .border_type(BorderType::Thick)
                    .title_top(" Up Next ")
                    .title_bottom(" | [j/k] Up/Down | [Enter] Play Now | [Backspace] Remove | ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_stateful_widget(widget_next, left_middle, state);
        } else {
            widget_next = widget_next.block(
                Block::bordered()
                    .title_top(" Library ")
                    .border_type(BorderType::Thick)
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(widget_next, left_middle);
        }

        if let NavState::Library(state) = &mut self.nav_state {
            widget_all = widget_all.block(
                Block::bordered()
                    .border_style(Style::new().fg(Color::Yellow))
                    .title(" Library ")
                    .border_type(BorderType::Thick)
                    .title_bottom(" | [j/k] Up/Down | [Enter] Play Later | [a] Play All | ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_stateful_widget(widget_all, right, state);
        } else {
            widget_all = widget_all.block(
                Block::bordered()
                    .title(" Up Next ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(widget_all, right);
        }
    }

    fn exit(&mut self) {
        self.set_nav_state(NavState::Exit);
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
    let music_dir_path = PathBuf::from(MUSIC_DIR);
    let cache_path = Path::new(CACHE_FILE);
    let songs = Songs::new(cache_path, music_dir_path);

    if !cache_path.exists() {
        if let Ok(json) = serde_json::to_string(&songs.songs_library) {
            if let Ok(mut cache_file) = File::create_new(cache_path) {
                cache_file.write_all(json.as_bytes()).unwrap();
            }
        }
    }

    let mut app = App::new(songs);
    app.songs.songs_library.shuffle(&mut thread_rng());
    // for i in 0..app.songs.songs_all.len() - 1 {
    // app.songs.songs_playing.push(i);
    // }

    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let mut result = Ok(());

    std::io::stdout()
        .execute(crossterm::event::EnableMouseCapture)
        .unwrap();

    let mut ticks: u64 = 0;
    while app.nav_state != NavState::Exit {
        result = app.run(&mut terminal, &ticks);
        ticks += 1;
        //TODO: handle errors here or send to UI for next draw loop?
    }

    ratatui::restore();

    app.songs.kill_current();

    result
}
