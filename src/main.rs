use color_eyre::{eyre::Error, Result};
use crossterm::ExecutableCommand;
use ratatui::style::Color;
use std::{
    io::{self},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    app::{App, NavState},
    songs::Songs,
};

mod app;
mod input;
mod mpv;
mod songs;
mod utilities;
mod widget;

const MUSIC_DIR: &'static str = "/home/nate/Music/";
const CACHE_FILE: &'static str = "/home/nate/.cache/music-player-cache.json";
const MPV_SOCKET: &'static str = "/tmp/mpv-socket";
const COLOR_BORDER: Color = Color::Yellow;
const COLOR_HEADERS: Color = Color::Green;

fn main() -> Result<()> {
    color_eyre::install()?;

    let music_dir_path = PathBuf::from(MUSIC_DIR);
    let cache_path = Path::new(CACHE_FILE);
    let songs = Songs::new(cache_path, music_dir_path);
    let app = Arc::new(Mutex::new(App::new(songs)));
    let mut terminal = ratatui::init();

    io::stdout().execute(crossterm::event::EnableMouseCapture)?;
    terminal.clear().unwrap();

    let app_ref = app.clone();
    let handle_draw: JoinHandle<Result<()>> = thread::spawn(move || loop {
        let Ok(mut app) = app_ref.try_lock() else {
            continue;
        };

        if app.nav_state == NavState::Exit {
            return Ok(());
        }

        if let Err(err) = terminal
            .draw(|frame| app.draw(frame))
            .map_err(|err| Error::new(err))
        {
            app.exit();
            return Err(err);
        }

        thread::sleep(Duration::from_millis(20));
    });

    let app_ref = app.clone();
    let handle: JoinHandle<Result<()>> = thread::spawn(move || loop {
        let Ok(mut app) = app_ref.try_lock() else {
            continue;
        };

        if app.nav_state == NavState::Exit {
            return Ok(());
        }

        if let Err(err) = app.handle_events() {
            app.exit();
            return Err(err);
        }

        if let Err(err) = app.handle_song_state() {
            app.exit();
            return Err(err);
        }

        thread::sleep(Duration::from_millis(5));
    });

    loop {
        if handle.is_finished() {
            ratatui::restore();
            return handle.join().unwrap();
        }

        if handle_draw.is_finished() {
            ratatui::restore();
            return handle_draw.join().unwrap();
        }

        thread::sleep(Duration::from_millis(200));
    }
}
