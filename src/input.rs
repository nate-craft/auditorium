use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Position;

use crate::app::App;
use crate::app::Message;
use crate::app::NavState;

pub fn handle_input(app: &mut App) -> Message {
    let event;
    if event::poll(Duration::new(0, 0)).unwrap() {
        event = event::read().unwrap();
    } else {
        return Message::None;
    }

    app.reset_click_position();

    if let Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        ..
    }) = event
    {
        app.set_click_position(Position::new(column, row));
    } else if let Event::Key(KeyEvent {
        code: KeyCode::Char('c') | KeyCode::Char('d'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        ..
    }) = event
    {
        return Message::Exit;
    } else if let Event::Key(KeyEvent {
        code,
        kind: KeyEventKind::Press,
        ..
    }) = event
    {
        if app.song_query.is_some() && app.nav_state == NavState::Search {
            if code == KeyCode::Esc {
                return Message::Escape;
            } else if code == KeyCode::Backspace {
                return Message::ModifyFind(None);
            } else if let KeyCode::Char(c) = code {
                return Message::ModifyFind(Some(c));
            } else if let KeyCode::Tab = code {
                return Message::NavStateNext;
            } else if let KeyCode::BackTab = code {
                return Message::NavStatePrev;
            } else {
                return Message::None;
            }
        }

        match code {
            KeyCode::Char('q') => return Message::Exit,
            KeyCode::Char('R') => return Message::ReloadConfig,
            KeyCode::Char('r') => return Message::ReloadMusic,
            KeyCode::Char('c') => return Message::ClearUpNext,
            KeyCode::Char('/') => return Message::Find,
            KeyCode::Char(' ') => return Message::PauseToggle(!app.paused),
            KeyCode::Char('>') | KeyCode::Char('n') => return Message::SongNext,
            KeyCode::Char('<') | KeyCode::Char('p') => return Message::SongPrevious,
            KeyCode::Right => return Message::SongSeek(5),
            KeyCode::Left => return Message::SongSeek(-5),
            KeyCode::Char('a') => return Message::PlayAll,
            KeyCode::BackTab => return Message::NavStatePrev,
            KeyCode::Tab => return Message::NavStateNext,
            KeyCode::Esc => return Message::Escape,
            KeyCode::Enter => {
                if app.nav_state == NavState::Search {
                    return Message::Find;
                } else {
                    return Message::MoveSong;
                }
            }
            KeyCode::Backspace | KeyCode::Char('d') => {
                if let NavState::UpNext(table_state) = &app.nav_state {
                    if let Some(selected) = table_state.selected() {
                        return Message::DeleteNextUp(selected + 1);
                    }
                }
            }
            KeyCode::Char('j') | KeyCode::PageDown | KeyCode::Down => {
                let elements = match app.nav_state {
                    NavState::UpNext(_) => app.songs.songs_in_next_up(),
                    NavState::Library(_) => app.songs.songs_in_library(),
                    _ => 0,
                };
                return Message::NavStateInnerNext(code != KeyCode::PageDown, elements);
            }
            KeyCode::Char('k') | KeyCode::PageUp | KeyCode::Up => {
                let elements = match app.nav_state {
                    NavState::UpNext(_) => app.songs.songs_in_next_up(),
                    NavState::Library(_) => app.songs.songs_in_library(),
                    _ => 0,
                };
                return Message::NavStateInnerPrev(code != KeyCode::PageUp, elements);
            }
            _ => return Message::None,
        }
    }

    return Message::None;
}
