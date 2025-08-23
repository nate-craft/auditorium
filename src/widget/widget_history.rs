use ratatui::{
    layout::Alignment,
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::App;

pub fn widget_history<'a>(app: &App) -> Paragraph<'a> {
    Paragraph::new(vec![Line::from(
        app.songs
            .last_played()
            .map(|song| song.title.clone())
            .unwrap_or_else(|| "No Previous Songs".to_owned()),
    )])
    .centered()
    .block(
        Block::bordered()
            .title_top(" Last Played ")
            .title_alignment(Alignment::Center),
    )
}
