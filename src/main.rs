use clap::{arg, command, Parser};
use color_eyre::{eyre::Error, Result};
use crossterm::ExecutableCommand;
use std::{
    io::{self},
    path::PathBuf,
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
#[cfg(feature = "mpris")]
mod mpris;
mod mpv;
mod songs;
mod utilities;
mod widget;

const MPV_SOCKET: &'static str = "/tmp/mpv-socket";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Flags {
    #[arg(short, long)]
    dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let flags = Flags::parse();

    let config = Config::with_dir(flags.dir)?;
    let cache_path = files::cache_path()?;
    let songs = Songs::new(&config, &cache_path)?;
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

        thread::sleep(Duration::from_millis(35));
    });

    #[cfg(feature = "mpris")]
    let handle_mpris: JoinHandle<Result<()>> = mpris::mpris::thread_mpris(app.clone());

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

        #[cfg(feature = "mpris")]
        if handle_mpris.is_finished() {
            ratatui::restore();
            return handle_mpris.join().unwrap();
        }

        thread::sleep(Duration::from_millis(200));
    }
}
