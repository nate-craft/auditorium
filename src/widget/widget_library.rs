use std::cmp::max;

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    widgets::{Cell, Row, Table},
};
use textwrap::Options;

use crate::app::App;

pub fn build<'a>(app: &App, area: Rect) -> Table<'a> {
    let left_percent = 0.66;
    let right_percent = 0.33;
    let row_constraints = [
        Constraint::Percentage((left_percent * 100.0) as u16),
        Constraint::Percentage((right_percent * 100.0) as u16),
    ];

    let next: Vec<Row> = app
        .songs
        .showing_songs_library()
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let title_width = (left_percent * area.width as f32) as u16 - 1;
            let options = Options::new(title_width as usize).break_words(true);
            let title_lines = textwrap::wrap(&song.title, options);
            let title_lines_str: String = title_lines
                .iter()
                .map(|line| format!("{}\n", line))
                .collect();

            let artist_width = (right_percent * area.width as f32) as u16 - 1;
            let artist_lines = textwrap::wrap(&song.artist, artist_width as usize);
            let artist_lines_str: String = artist_lines
                .iter()
                .map(|line| format!("{}\n", line))
                .collect();

            if i % 2 == 0 {
                Row::new(vec![
                    Cell::new(title_lines_str),
                    Cell::new(artist_lines_str),
                ])
                .height(max(title_lines.len(), artist_lines.len()) as u16)
                .fg(app.config.color_row)
            } else {
                Row::new(vec![
                    Cell::new(title_lines_str),
                    Cell::new(artist_lines_str),
                ])
                .height(max(title_lines.len(), artist_lines.len()) as u16)
            }
        })
        .collect();

    Table::new(next, row_constraints)
        .header(
            Row::new(vec![Cell::new("Title"), Cell::new("Artist")])
                .bold()
                .style(Style::default().fg(app.config.color_headers))
                .bottom_margin(1),
        )
        .row_highlight_style(Style::new().bg(app.config.color_border).fg(Color::Black))
}
