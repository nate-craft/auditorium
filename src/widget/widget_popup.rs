use ratatui::{
    layout::Alignment,
    style::Style,
    text::Text,
    widgets::{Block, Paragraph},
};

use crate::app::App;

pub fn build<'a>(app: &App, content: &'a str) -> Paragraph<'a> {
    Paragraph::new(Text::from(content).centered())
        .centered()
        .block(
            Block::bordered()
                .title_bottom(" | [Esc] Close | ")
                .title_alignment(Alignment::Center)
                .border_style(Style::default().fg(app.config.color_border)),
        )
}
