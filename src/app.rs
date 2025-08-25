use color_eyre::Result;
use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Position, Rect},
    style::{Style, Stylize},
    widgets::{Block, BorderType, Borders, Clear, TableState},
    Frame,
};
use std::cmp::{max, min};

use crate::{
    files::Config,
    input,
    mpv::MpvCommand,
    songs::Songs,
    widget::{widget_all, widget_history, widget_playing, widget_popup, widget_up_next},
};

#[derive(PartialEq, Eq)]
pub enum NavState {
    Player,
    UpNext(TableState),
    Library(TableState),
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
}

pub enum Message {
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
    ReloadConfig,
    ReloadMusic,
    ClearError,
}

impl NavState {
    pub fn rows_per_skip(is_single_row: bool) -> usize {
        if is_single_row {
            1
        } else {
            10
        }
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
        }
    }

    pub fn handle_events(&mut self) -> Result<()> {
        match input::handle_events(self) {
            Message::None => {}
            Message::Exit => {
                self.exit();
                return Ok(());
            }
            Message::ClearError => self.alert = None,
            Message::Pause(paused) => {
                if let Err(_) = MpvCommand::TogglePause.run() {
                    self.alert = Some("Error querying MPV for pause information".to_owned());
                } else {
                    self.paused = paused;
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
                if self.songs.songs_library.len() > 0 {
                    self.songs
                        .push_songs_back(0..self.songs.songs_in_library() - 1);
                }
                self.songs.shuffle();
            }
            Message::ReloadConfig => {
                self.config.reload()?;
            }
            Message::ReloadMusic => {
                self.songs.reload(&self.config)?;
                self.set_nav_state(self.nav_state.as_stateful_default(self));
                self.alert = Some(format!(
                    "New music library loaded from {}",
                    self.config.music_directory.to_string_lossy()
                ));
            }
            Message::MoveSong => match &self.nav_state {
                NavState::UpNext(table_state) => {
                    table_state.selected().map(|selected| {
                        let selected = selected + 1;
                        self.songs.get_next_by_index(selected).map(|library_index| {
                            self.songs.remove_next_up(selected);
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

    pub fn handle_song_state(&mut self) -> Result<()> {
        match self.songs.active_command_mut() {
            None => {
                match self.song_state {
                    SongLoadingState::Backward => {
                        self.songs.skip_backward();
                    }
                    SongLoadingState::Forward => {
                        self.songs.try_play_current_song();
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
                    self.songs.try_play_current_song();
                }
            }
        }

        return Ok(());
    }

    pub fn draw(&mut self, frame: &mut Frame) {
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
                    .border_style(Style::new().fg(self.config.color_border))
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
                    .border_style(Style::new().fg(self.config.color_border))
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

        if let Some(alert) = &self.alert {
            let vertical = Layout::vertical([Constraint::Length(3)]).flex(Flex::Center);
            let horizontal =
                Layout::horizontal([Constraint::Length(alert.len() as u16 + 2)]).flex(Flex::Center);
            let [area] = vertical.areas(frame.area());
            let [area] = horizontal.areas(area);

            frame.render_widget(Clear, area);
            frame.render_widget(widget_popup::build(self, &alert), area);
        }
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

    pub fn next_nav_state(&mut self) {
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
