use clap::Parser;
use color_eyre::{Result, eyre::Error};
use crossterm::ExecutableCommand;
use ratatui::{Terminal, prelude::CrosstermBackend};
use std::{
    io::{self, Stdout},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    app::{App, AppLayout, NavState},
    files::Config,
    songs::Songs,
};

mod app;
mod files;
mod input;
#[cfg(feature = "mpris")]
#[cfg(not(target_os = "windows"))]
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
    let mut handles = threads(app.clone(), ratatui::init());

    io::stdout().execute(crossterm::event::EnableMouseCapture)?;

    loop {
        for handle_option in handles
            .iter_mut()
            .filter(|handle| handle.as_ref().is_some_and(|handle| handle.is_finished()))
        {
            if let Some(handle) = handle_option.take() {
                ratatui::restore();
                return generate_results(handle.join().unwrap(), handles);
            }
        }

        thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(feature = "mpris")]
#[cfg(not(target_os = "windows"))]
fn threads(
    app: Arc<Mutex<App>>,
    terminal: Terminal<CrosstermBackend<Stdout>>,
) -> [Option<JoinHandle<Result<()>>>; 3] {
    [
        Some(thread_draw(app.clone(), terminal)),
        Some(thread_events(app.clone())),
        Some(mpris::mpris::thread_mpris(app.clone())),
    ]
}

#[cfg(not(feature = "mpris"))]
fn threads(
    app: Arc<Mutex<App>>,
    terminal: Terminal<CrosstermBackend<Stdout>>,
) -> [Option<JoinHandle<Result<()>>>; 3] {
    [
        Some(thread_draw(app.clone(), terminal)),
        Some(thread_events(app.clone())),
        None,
    ]
}

fn thread_draw(
    app: Arc<Mutex<App>>,
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let mut ms_elapsed: u64 = 0;
        const DELAY: u64 = 17;

        loop {
            let Ok(mut app) = app.try_lock() else {
                continue;
            };

            if app.nav_state == NavState::Exit {
                return Ok(());
            }

            if app.needs_redraw || ms_elapsed % (DELAY * 10) == 0 {
                let area = terminal.get_frame().area();
                let layout = AppLayout::new(&area);

                if app.click_position.is_some() {
                    app.handle_click(&layout);
                }

                let result = terminal
                    .draw(|frame| app.draw(frame, layout))
                    .map_err(|err| Error::new(err));

                if let Err(err) = result {
                    app.exit();
                    return Err(err);
                }

                app.needs_redraw = false;
            }

            drop(app);
            ms_elapsed += DELAY;
            thread::sleep(Duration::from_millis(DELAY));
        }
    })
}

fn thread_events(app: Arc<Mutex<App>>) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        loop {
            let Ok(mut app) = app.try_lock() else {
                continue;
            };

            if app.nav_state == NavState::Exit {
                return Ok(());
            }

            if let Err(err) = app.handle_events() {
                app.exit();
                return Err(err);
            }

            drop(app);
            thread::sleep(Duration::from_millis(5));
        }
    })
}

fn generate_results<const N: usize>(
    mut result_sum: Result<()>,
    handles: [Option<JoinHandle<Result<()>>>; N],
) -> Result<()> {
    for handle in handles.into_iter().filter_map(|handle| handle) {
        let handle_joined = handle
            .join()
            .unwrap_or(Err(Error::msg("Could not join handle")));

        if let Err(err) = handle_joined {
            result_sum = match result_sum {
                Ok(_) => Err(err),
                Err(existing) => Err(existing.wrap_err(err)),
            };
        }
    }

    result_sum
}
