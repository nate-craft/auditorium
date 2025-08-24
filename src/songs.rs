use std::{
    fs::File,
    io::{self, BufReader, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use ffprobe::FfProbeError;
use random_number::rand::{self, seq::SliceRandom};
use ratatui::crossterm::style::Stylize;
use serde::{Deserialize, Serialize};

use crate::{SongLoadingState, MPV_SOCKET};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Song {
    pub title: String,
    pub genre: String,
    pub artist: String,
    pub path: PathBuf,
}

pub struct Songs {
    pub songs_library: Vec<Song>,
    songs_next: Vec<usize>,
    songs_history: Vec<usize>,
    active: Option<Child>,
}

impl Song {
    fn new(file_name: &Path) -> Result<Song, FfProbeError> {
        let probe = ffprobe::ffprobe(&file_name)?;
        let mut title: String = "Unknown".to_owned();
        let mut genre: String = "Unknown".to_owned();
        let mut artist: String = "Unknown".to_owned();
        let path = file_name.to_owned();

        probe.format.tags.map(|tags| {
            tags.extra.get("title").map(|title_inner| {
                title_inner.as_str().map(|title_inner| {
                    title = title_inner.to_owned();
                });
            });
            tags.extra.get("genre").map(|genre_inner| {
                genre_inner.as_str().map(|genre_inner| {
                    genre = genre_inner.to_owned();
                });
            });
            tags.extra.get("artist").map(|artist_inner| {
                artist_inner.as_str().map(|author_inner| {
                    artist = author_inner.to_owned();
                });
            })
        });

        return Ok(Song {
            title,
            genre,
            artist,
            path,
        });
    }

    pub fn play_single(&self) -> Result<Child, io::Error> {
        Command::new("mpv")
            .arg("--no-video")
            .arg("--no-resume-playback")
            .arg("--msg-level=all=no")
            .arg("--no-terminal")
            .arg("--quiet")
            .arg(format!("{}{}", "--input-ipc-server=", MPV_SOCKET))
            .arg(self.path.to_string_lossy().into_owned())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

impl Songs {
    pub fn new(cache_path: &Path, music_path: PathBuf) -> Songs {
        let mut song_map = Vec::new();

        if cache_path.exists() {
            if let Ok(file) = File::open(cache_path) {
                let songs_cached: Result<Vec<Song>, _> =
                    serde_json::from_reader(BufReader::new(file));
                if let Ok(song_cached) = songs_cached {
                    song_map = song_cached;
                }
            }
        }

        let mut songs = Songs {
            songs_library: song_map,
            songs_next: Vec::new(),
            songs_history: Vec::new(),
            active: None,
        };

        if songs.songs_library.is_empty() {
            println!(
                "{}",
                "Loading music from Music directory. This may take a moment...".green()
            );

            if let Some(errors) = songs.load_songs(&music_path) {
                errors.iter().for_each(|e| eprintln!("{}", e));
            }

            if let Ok(json) = serde_json::to_string(&songs.songs_library) {
                if let Ok(mut cache_file) = File::create_new(cache_path) {
                    cache_file.write_all(json.as_bytes()).unwrap();
                }
            }
        }

        songs
            .songs_library
            .sort_by(|first, second| first.artist.cmp(&second.artist));

        songs
    }

    pub fn load_songs(&mut self, root_dir: &Path) -> Option<Vec<FfProbeError>> {
        let (send, rec) = mpsc::channel::<Result<Song, FfProbeError>>();
        let (send_threads, rec_threads) = mpsc::channel::<JoinHandle<()>>();
        let mut errors = Vec::new();
        let mut num_started = 0;
        let mut num_ended = 0;
        Self::add_from_dir(send.clone(), send_threads.clone(), root_dir);

        loop {
            while let Ok(result) = rec.try_recv() {
                match result {
                    Ok(song) => {
                        self.songs_library.push(song);
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
                num_ended += 1;
            }

            while let Ok(_) = rec_threads.try_recv() {
                num_started += 1;
            }

            if num_ended >= num_started {
                if errors.is_empty() {
                    return None;
                } else {
                    return Some(errors);
                }
            }
        }
    }

    pub fn add_from_dir(
        send: Sender<Result<Song, FfProbeError>>,
        send_threads: Sender<JoinHandle<()>>,
        dir: &Path,
    ) {
        if let Ok(child) = dir.read_dir() {
            child.for_each(|child_result| {
                let Ok(child) = child_result else {
                    return;
                };

                let child_path = child.path();
                if child_path.is_dir() {
                    Self::add_from_dir(send.clone(), send_threads.clone(), &child_path);
                } else {
                    let send_clone = send.clone();
                    send_threads
                        .send(thread::spawn(move || match Song::new(&child_path) {
                            Ok(song) => send_clone.clone().send(Ok(song)).unwrap(),
                            Err(e) => send_clone.clone().send(Err(e)).unwrap(),
                        }))
                        .unwrap();
                }
            });
        }
    }

    pub fn current_song_index(&self) -> Option<usize> {
        self.songs_next.get(0).copied()
    }

    pub fn current_song(&self) -> Option<&Song> {
        self.current_song_index()
            .map(|index| self.songs_library.get(index))
            .flatten()
    }

    pub fn next_playing(&self) -> Vec<&Song> {
        if self.songs_next.len() <= 1 {
            Vec::new()
        } else {
            self.songs_next[1..]
                .iter()
                .filter_map(|index| self.songs_library.get(*index))
                .collect()
        }
    }

    pub fn next(&mut self, song_state: &SongLoadingState) {
        match song_state {
            SongLoadingState::Backward => {
                if let Some(previous) = self.last_played_index() {
                    self.songs_history.remove(self.songs_history.len() - 1);
                    self.songs_next.insert(0, previous);
                }
            }
            SongLoadingState::Forward => {
                if !self.songs_next.is_empty() {
                    self.current_song_index()
                        .map(|current| self.songs_history.push(current));
                    self.songs_next.remove(0);
                }
            }
        }
    }

    pub fn kill_current(&mut self) {
        if let Some(active) = &mut self.active {
            let _ = active.kill();
        }
    }

    pub fn song_is_active(&mut self) -> bool {
        if let Some(active) = &mut self.active {
            if let Ok(status) = active.try_wait() {
                return status.is_none();
            }
        }
        return false;
    }

    pub fn last_played_index(&self) -> Option<usize> {
        if !self.songs_history.is_empty() {
            self.songs_history
                .get(self.songs_history.len() - 1)
                .copied()
        } else {
            None
        }
    }

    pub fn last_played(&self) -> Option<&Song> {
        self.last_played_index()
            .map(|index| self.songs_library.get(index))
            .flatten()
    }

    pub fn push_songs_back(&mut self, song_indicies: Range<usize>) {
        self.songs_next.extend(song_indicies.into_iter());
    }

    pub fn shuffle(&mut self) {
        self.songs_next.shuffle(&mut rand::thread_rng());
    }

    pub fn push_song_back(&mut self, selected: usize) {
        self.songs_next.push(selected);
    }

    pub fn push_song_front(&mut self, selected: usize) {
        self.songs_next.insert(1, selected);
    }

    pub fn try_play_current_song(&mut self) {
        self.active = self
            .current_song()
            .map(|process| process.play_single().unwrap());
    }

    pub fn skip_backward(&mut self) {
        if let Some(previous) = self.last_played_index() {
            self.songs_history.remove(self.songs_history.len() - 1);
            self.songs_next.insert(0, previous);
        }
    }

    pub fn active_command_mut(&mut self) -> Option<&mut Child> {
        self.active.as_mut()
    }

    pub fn songs_in_library(&self) -> usize {
        self.songs_library.len()
    }

    pub fn songs_in_next_up(&self) -> usize {
        self.songs_next.len()
    }

    pub fn remove_next_up(&mut self, selected: usize) {
        self.songs_next.remove(selected);
    }

    pub fn get_next_by_index(&self, selected: usize) -> Option<usize> {
        self.songs_next.get(selected).copied()
    }
}
