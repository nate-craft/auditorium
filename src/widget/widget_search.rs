use ratatui::{
    layout::Alignment,
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::App;

pub fn build<'a>(app: &App) -> Paragraph<'a> {
    Paragraph::new(vec![Line::from(
        app.song_query
            .as_ref()
            .map(|query| format!("Searching: {}", query))
            .unwrap_or("".to_owned()),
    )])
    .block(
        Block::bordered()
            .title_top(" Find Song ")
            .title_alignment(Alignment::Center),
    )
}
