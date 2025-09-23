use color_eyre::{Result, eyre::Error};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Position, Rect},
    style::{Style, Stylize},
    widgets::{Block, BorderType, Borders, Clear, TableState},
};
use std::{
    cmp::{max, min},
    sync::mpsc::{self, Receiver, Sender},
};

use crate::{
    files::Config,
    input,
    mpv::MpvCommand,
    songs::Songs,
    widget::{
        widget_history, widget_library, widget_playing, widget_popup, widget_search, widget_up_next,
    },
};

#[derive(PartialEq, Eq)]
pub enum NavState {
    Player,
    UpNext(TableState),
    Library(TableState),
    Search,
    Exit,
}

#[derive(PartialEq, Eq)]
pub enum SongLoadingState {
    Backward,
    Forward,
}

pub struct App {
    pub songs: Songs,
    pub paused: bool,
    pub config: Config,
    pub nav_state: NavState,
    pub song_state: SongLoadingState,
    pub click_position: Option<Position>,
    pub alert: Option<String>,
    pub song_query: Option<String>,
    pub mpris_message_out: (Sender<Message>, Receiver<Message>),
}

#[derive(Clone, Copy)]
pub enum Message {
    None,
    Exit,
    PauseToggle(bool),
    Stop,
    SongNext,
    SongPrevious,
    NavStateNext,
    NavStatePrev,
    NavStateInnerNext(bool, usize),
    NavStateInnerPrev(bool, usize),
    MoveSong,
    DeleteNextUp(usize),
    PlayAll,
    ReloadConfig,
    ReloadMusic,
    Escape,
    Find,
    ModifyFind(Option<char>),
    ClearUpNext,
    SongSeek(i32),
}

impl NavState {
    pub fn rows_per_skip(is_single_row: bool) -> usize {
        if is_single_row { 1 } else { 10 }
    }

    pub fn event_list_up(&mut self, is_single: bool, num_entries: usize) {
        if let NavState::UpNext(state) | NavState::Library(state) = self {
            let skips = Self::rows_per_skip(is_single);

            if num_entries > 0 {
                state.select(Some(
                    max(0i32, state.selected().unwrap_or(0) as i32 - skips as i32) as usize,
                ));
            }
        }
    }

    pub fn event_list_down(&mut self, is_single: bool, num_entries: usize) {
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

    pub fn as_stateful_default(&self, app: &App) -> NavState {
        match self {
            NavState::Player => NavState::Player,
            NavState::UpNext(_) => {
                if app.songs.songs_in_next_up() != 0 {
                    NavState::UpNext(TableState::default().with_selected(Some(0)))
                } else {
                    NavState::UpNext(TableState::default())
                }
            }
            NavState::Library(_) => {
                if app.songs.songs_in_library() != 0 {
                    NavState::Library(TableState::default().with_selected(Some(0)))
                } else {
                    NavState::Library(TableState::default())
                }
            }
            NavState::Search => NavState::Search,
            NavState::Exit => NavState::Exit,
        }
    }
}

impl App {
    pub fn new(songs: Songs, config: Config) -> App {
        App {
            songs,
            nav_state: NavState::Player,
            song_state: SongLoadingState::Forward,
            config,
            paused: false,
            click_position: None,
            alert: None,
            song_query: None,
            mpris_message_out: mpsc::channel(),
        }
    }

    pub fn handle_events(&mut self) -> Result<()> {
        let message = input::handle_input(self);

        let result_handle = self
            .handle_message(message)
            .map_err(|err| Error::msg(err.to_string()));
        let result_state = self.handle_song_state();

        // Need to wait to send mpris update till song state is setup
        match message {
            Message::None | Message::Escape => {}
            _ => {
                self.mpris_message_out.0.send(message)?;
            }
        }

        Self::result_concat([result_handle, result_state])
    }

    pub fn handle_message_mpris(&mut self, message: Message) -> Result<()> {
        let result_mpris = self
            .mpris_message_out
            .0
            .send(message)
            .map_err(|err| Error::new(err));
        let result_handle = self
            .handle_message(message)
            .map_err(|err| Error::msg(err.to_string()));
        let result_state = self.handle_song_state();

        Self::result_concat([result_mpris, result_handle, result_state])
    }

    fn result_concat<const N: usize>(results: [Result<()>; N]) -> Result<()> {
        results
            .into_iter()
            .filter_map(|result| result.err())
            .fold(Ok(()), |acc, added| match acc {
                Ok(_) => Err(added),
                Err(err) => Err(err.wrap_err(added)),
            })
    }

    fn handle_message(&mut self, message: Message) -> Result<()> {
        match message {
            Message::None => {}
            Message::Exit => {
                self.exit();
                return Ok(());
            }
            Message::Escape => {
                self.alert = None;
                self.song_query = None;
                self.songs.unfiltered_apply();
            }
            Message::Stop => {
                self.songs.kill_current();
                self.songs.clear_up_next();
            }
            Message::PauseToggle(paused) => {
                if self.songs.song_is_running() {
                    if let Err(_) = MpvCommand::TogglePause(paused).run() {
                        self.alert = Some("Error querying MPV for pause information".to_owned());
                    } else {
                        self.paused = paused;
                    }
                }
            }
            Message::SongSeek(time) => {
                if self.songs.song_is_running() {
                    if let Err(_) = MpvCommand::Seek(time).run() {
                        self.alert = Some("Error seeking forward with MPV".to_owned());
                    }
                }
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
                self.songs.remove_next_up(selected);
            }
            Message::PlayAll => {
                self.songs.push_back_all();
            }
            Message::Find => {
                self.song_query = Some("".to_owned());
                self.songs.filter_apply(self.song_query.as_ref());
                self.set_nav_state(NavState::Search);
            }
            Message::ModifyFind(addition) => {
                match addition {
                    Some(addition) => {
                        if let Some(query) = self.song_query.as_mut() {
                            query.push(addition);
                        }
                    }
                    None => {
                        if let Some(query) = self.song_query.as_mut() {
                            if query.len() > 0 {
                                query.truncate(query.len() - 1);
                            }
                        }
                    }
                }
                self.songs.filter_apply(self.song_query.as_ref());
            }
            Message::ClearUpNext => {
                self.songs.clear_up_next();
            }
            Message::ReloadConfig => {
                self.config.reload()?;
            }
            Message::ReloadMusic => {
                self.songs.reload(&self.config)?;
                self.set_nav_state(self.nav_state.as_stateful_default(self));
                self.alert = Some(format!(
                    "New music library loaded from {}",
                    self.config.music_directory().to_string_lossy()
                ));
            }
            Message::MoveSong => match &self.nav_state {
                NavState::UpNext(table_state) => {
                    table_state.selected().map(|selected| {
                        let selected = selected + 1;
                        self.songs.next_by_index(selected).map(|library_index| {
                            self.songs.remove_next_up(selected);
                            self.songs.push_song_front(library_index);
                            self.songs.kill_current();
                        });
                    });
                }
                NavState::Library(table_state) => {
                    if let Some(selected) = table_state.selected() {
                        let real_index = self.songs.showing_songs_library.real_index(selected);
                        self.songs.push_song_back(real_index);
                    };
                }
                _ => {}
            },
        }

        return Ok(());
    }

    pub fn handle_song_state(&mut self) -> Result<()> {
        let exists = self.songs.active_exists();
        let running = self.songs.song_is_running();
        let active = self.songs.active_command_mut();

        if self.nav_state == NavState::Exit {
            self.songs.kill_current();
            return Ok(());
        }

        if active.marked_dead || (exists && !running) {
            // Manually killed
            self.songs.next(&self.song_state);
            self.song_state = SongLoadingState::Forward;
            self.paused = false;
            self.songs.try_play_current_song()?;
        } else if !running {
            // Nothing playing yet
            match self.song_state {
                SongLoadingState::Backward => {
                    self.songs.previous();
                }
                SongLoadingState::Forward => {
                    self.songs.try_play_current_song()?;
                }
            }

            self.song_state = SongLoadingState::Forward;
        }

        return Ok(());
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let outer_border = Block::bordered()
            .title(" Music ")
            .title_alignment(Alignment::Center)
            .title_style(Style::default().bold())
            .title_bottom("| [Tab] Nav | [j/k] Up/Down |")
            .borders(Borders::all());
        let outer_area = outer_border.inner(frame.area());

        let [left, right] = Layout::horizontal([Constraint::Fill(1); 2]).areas(outer_area);
        let [right_top, right_bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(right);
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
        } else if self.click_position_matches_rect(right_bottom) {
            self.set_nav_state(NavState::Library(TableState::default()).as_stateful_default(&self));
        } else if self.click_position_matches_rect(right_top) {
            self.set_nav_state(NavState::Search);
            if self.song_query.is_none() {
                self.song_query = Some("".to_string());
            }
        }

        let (widget_playing, border_player) = widget_playing::build(self);
        let widget_history = widget_history::build(self);
        let mut widget_search = widget_search::build(self);
        let mut widget_next = widget_up_next::build(self, left_middle);
        let mut widget_library = widget_library::build(self, right_bottom);

        if let NavState::Search = self.nav_state {
            let mut border = Block::bordered()
                .border_style(Style::new().fg(self.config.color_border))
                .border_type(BorderType::Thick)
                .title_alignment(Alignment::Center)
                .title_top(" Find Song ");

            if self.song_query.is_some() {
                border = border.title_bottom(" | [Esc] Clear | [Tab] Nav | ");
            } else {
                border = border.title_bottom(" | [/] Search | ");
            }

            widget_search = widget_search.block(border);
        }

        if let NavState::UpNext(state) = &mut self.nav_state {
            widget_next = widget_next.block(
                Block::bordered()
                    .border_style(Style::new().fg(self.config.color_border))
                    .border_type(BorderType::Thick)
                    .title_top(" Up Next ")
                    .title_bottom(" | [Enter] Play Now | [Backspace] Remove | [c] Clear | ")
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
            widget_library = widget_library.block(
                Block::bordered()
                    .border_style(Style::new().fg(self.config.color_border))
                    .title(" Library ")
                    .border_type(BorderType::Thick)
                    .title_bottom(" | [/] Search | [Enter] Play Later | [a] Play All | ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_stateful_widget(widget_library, right_bottom, state);
        } else {
            widget_library = widget_library.block(
                Block::bordered()
                    .title(" Library ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(widget_library, right_bottom);
        }

        frame.render_widget(&border_player, left_top);

        #[cfg(feature = "image")]
        if let Some(cover) = &mut self.songs.active_command_mut().cover {
            use ratatui::layout::Direction;

            let left_top_area = border_player.inner(left_top);
            let [player_left, player_right] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(left_top_area.width - (left_top_area.height * 2)),
                    Constraint::Length(7),
                ])
                .areas(left_top_area);

            frame.render_widget(&widget_playing, player_left);

            frame.render_stateful_widget(
                ratatui_image::StatefulImage::default(),
                player_right,
                cover,
            );
        } else {
            frame.render_widget(&widget_playing, border_player.inner(left_top));
        }

        #[cfg(not(feature = "image"))]
        frame.render_widget(&widget_playing, border_player.inner(left_top));

        if let Some(alert) = &self.alert {
            let vertical = Layout::vertical([Constraint::Length(3)]).flex(Flex::Center);
            let horizontal =
                Layout::horizontal([Constraint::Length(alert.len() as u16 + 2)]).flex(Flex::Center);
            let [area] = vertical.areas(frame.area());
            let [area] = horizontal.areas(area);

            frame.render_widget(Clear, area);
            frame.render_widget(widget_popup::build(self, &alert), area);
        }

        frame.render_widget(outer_border, frame.area());
        frame.render_widget(widget_history, left_bottom);
        frame.render_widget(widget_search, right_top);
    }

    pub fn exit(&mut self) {
        self.set_nav_state(NavState::Exit);
        self.songs.kill_current();
    }

    pub fn set_click_position(&mut self, position: Position) {
        self.click_position = Some(position);
    }

    pub fn click_position_matches_rect(&self, rect: Rect) -> bool {
        if let Some(click) = self.click_position {
            rect.contains(click)
        } else {
            false
        }
    }

    pub fn reset_click_position(&mut self) {
        self.click_position = None;
    }

    pub fn set_nav_state(&mut self, state: NavState) {
        self.nav_state = state;
    }

    pub fn previous_nav_state(&mut self) {
        match self.nav_state {
            NavState::Player => {
                self.nav_state = NavState::UpNext(TableState::default()).as_stateful_default(self);
            }
            NavState::UpNext(_) => {
                self.nav_state = NavState::Library(TableState::default()).as_stateful_default(self);
            }
            NavState::Library(_) => self.nav_state = NavState::Search,
            NavState::Search => {
                self.nav_state = NavState::Player;
            }
            NavState::Exit => {}
        }
    }

    pub fn next_nav_state(&mut self) {
        match self.nav_state {
            NavState::Player => {
                self.nav_state = NavState::Search;
            }
            NavState::UpNext(_) => {
                self.nav_state = NavState::Player;
            }
            NavState::Library(_) => {
                self.nav_state = NavState::UpNext(TableState::default()).as_stateful_default(self);
            }
            NavState::Search => {
                self.nav_state = NavState::Library(TableState::default()).as_stateful_default(self);
            }
            NavState::Exit => {}
        }
    }
}
