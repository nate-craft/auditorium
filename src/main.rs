use color_eyre::{eyre::Error, Result};
use crossterm::ExecutableCommand;
use std::{
    io::{self},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    app::{App, NavState},
    files::Config,
    songs::Songs,
};

mod app;
mod files;
mod input;
mod mpv;
mod songs;
mod utilities;
mod widget;

const MPV_SOCKET: &'static str = "/tmp/mpv-socket";

fn main() -> Result<()> {
    color_eyre::install()?;

    let config = Config::new()?;
    let cache_path = files::cache_path()?;
    let songs = Songs::new(&config, &cache_path);
    let app = Arc::new(Mutex::new(App::new(songs, config)));
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
