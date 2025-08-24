use ratatui::{
    layout::Alignment,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::{mpv::MpvCommandFeedback, SECONDARY_COLOR};
use crate::{App, MpvCommand, NavState};

pub fn widget_playing<'a>(app: &mut App) -> Paragraph<'a> {
    let mut widget_playing = {
        if let Some(playing) = app.songs.current_song() {
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Track: ", Style::default().green()),
                    Span::raw(playing.title.to_owned()),
                ]),
                Line::from(vec![
                    Span::styled("Artist: ", Style::default().green()),
                    Span::raw(playing.artist.to_owned()),
                ]),
                Line::from(vec![
                    Span::styled("Genre: ", Style::default().green()),
                    Span::raw(playing.genre.to_owned()),
                ]),
            ])
        } else {
            Paragraph::new(Line::from("No Track Loaded")).centered()
        }
    };

    let player_title = {
        let prefix = if app.paused { " Paused " } else { " Playing " };
        if app.songs.song_is_active() {
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

    if app.nav_state == NavState::Player {
        widget_playing = widget_playing.block(
            Block::bordered()
                .border_style(Style::new().fg(SECONDARY_COLOR))
                .border_type(BorderType::Thick)
                .title(player_title)
                .title_bottom("| [Space] Play | [</>] Prev/Next |")
                .title_alignment(Alignment::Center),
        );
    } else {
        widget_playing = widget_playing.block(
            Block::bordered()
                .title(player_title)
                .title_alignment(Alignment::Center),
        );
    }

    widget_playing
}
