use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Position;

use crate::{App, Message, NavState};

pub fn handle_events(app: &mut App) -> Message {
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
        code,
        modifiers,
        kind: KeyEventKind::Press,
        ..
    }) = event
    {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => app.exit(),
            KeyCode::Char('c') | KeyCode::Char('d') => {
                if modifiers.eq(&KeyModifiers::CONTROL) {
                    return Message::Exit;
                }
            }
            KeyCode::Enter => {
                return Message::MoveSong;
            }
            KeyCode::Char(' ') => {
                return Message::Pause(!app.paused);
            }
            KeyCode::Char('>') | KeyCode::Char('n') => {
                return Message::SongNext;
            }
            KeyCode::Char('<') | KeyCode::Char('p') => {
                return Message::SongPrevious;
            }
            KeyCode::Tab => {
                return Message::NavStateNext;
            }
            KeyCode::BackTab => {
                return Message::NavStatePrev;
            }
            KeyCode::Backspace => {
                if let NavState::UpNext(table_state) = &app.nav_state {
                    if let Some(selected) = table_state.selected() {
                        return Message::DeleteNextUp(selected);
                    }
                }
            }
            KeyCode::Char('a') => {
                return Message::PlayAll;
            }
            KeyCode::Char('j') | KeyCode::PageDown | KeyCode::Down => {
                let elements = match app.nav_state {
                    NavState::UpNext(_) => app.songs.songs_next.len(),
                    NavState::Library(_) => app.songs.songs_library.len(),
                    _ => 0,
                };
                return Message::NavStateInnerNext(code != KeyCode::PageDown, elements);
            }
            KeyCode::Char('k') | KeyCode::PageUp | KeyCode::Up => {
                let elements = match app.nav_state {
                    NavState::UpNext(_) => app.songs.songs_next.len(),
                    NavState::Library(_) => app.songs.songs_library.len(),
                    _ => 0,
                };
                return Message::NavStateInnerPrev(code != KeyCode::PageUp, elements);
            }
            _ => return Message::None,
        }
    }

    return Message::None;
}
