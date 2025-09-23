use ratatui::{
    layout::Alignment,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
};

use crate::mpv::MpvCommand;
use crate::mpv::MpvCommandFeedback;
use crate::{App, app::NavState};

pub fn build<'a>(app: &mut App) -> (Paragraph<'a>, Block<'a>) {
    let widget_playing = {
        if let Some(playing) = app.songs.current_song() {
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Track: ", Style::default().fg(app.config.color_headers)),
                    Span::raw(playing.title.to_owned()),
                ]),
                Line::from(vec![
                    Span::styled("Artist: ", Style::default().fg(app.config.color_headers)),
                    Span::raw(playing.artist.to_owned()),
                ]),
                Line::from(vec![
                    Span::styled("Genre: ", Style::default().fg(app.config.color_headers)),
                    Span::raw(
                        playing
                            .genres
                            .clone()
                            .into_iter()
                            .take(4)
                            .collect::<Vec<String>>()
                            .join(", "),
                    ),
                ]),
            ])
        } else {
            Paragraph::new(Line::from("No Track Loaded")).centered()
        }
    };

    let title_player = {
        let prefix = if app.paused { " Paused " } else { " Playing " };
        if app.songs.song_is_running() {
            if let Ok(feedback) = MpvCommand::GetProgress.run() {
                if let MpvCommandFeedback::String(progress) = feedback {
                    format!("{}{} ", prefix, progress)
                } else {
                    prefix.to_string()
                }
            } else {
                prefix.to_string()
            }
        } else {
            prefix.to_string()
        }
    };

    let title_nav = if app.paused {
        " | [Space] Play | [</>] Prev/Next | [Left/Right] Seek | "
    } else {
        " | [Space] Pause | [</>] Prev/Next | [Left/Right] Seek | "
    }
    .to_owned();

    let border_player = if app.nav_state == NavState::Player {
        Block::bordered()
            .border_style(Style::new().fg(app.config.color_border))
            .border_type(BorderType::Thick)
            .title(title_player)
            .title_bottom(title_nav)
            .padding(Padding::uniform(0))
            .title_alignment(Alignment::Center)
    } else {
        Block::bordered()
            .title(title_player)
            .padding(Padding::uniform(0))
            .title_alignment(Alignment::Center)
    };

    (widget_playing, border_player)
}
