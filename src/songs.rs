use std::{
    fs::File,
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use color_eyre::eyre::Error;
use ffprobe::FfProbeError;
use random_number::rand::{self, seq::SliceRandom};
use ratatui::crossterm::style::Stylize;
use serde::{Deserialize, Serialize};

use crate::{app::SongLoadingState, files::Config};
use crate::{files, MPV_SOCKET};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Song {
    pub title: String,
    pub genre: String,
    pub artist: String,
    pub path: PathBuf,
}

pub struct Songs {
    pub showing_songs_library: SongList,
    songs_data_library: Vec<Song>,
    songs_next: Vec<usize>,
    songs_history: Vec<usize>,
    active: Option<Child>,
}

pub enum SongList {
    All,
    Filtered(Vec<usize>),
}

impl SongList {
    pub fn real_index(&self, index: usize) -> usize {
        match self {
            SongList::All => index,
            SongList::Filtered(indicies) => *indicies.get(index).unwrap(),
        }
    }

    fn clear(&mut self) {
        if let SongList::Filtered(vec) = self {
            vec.clear();
        }
    }
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

    fn matches_query(&self, query: &String) -> bool {
        for query in query.split(",") {
            if query.starts_with("genre(") && query.ends_with(")") && query.len() > 8 {
                let sub_query = query[6..query.len() - 1].to_owned();
                if !self.genre.to_lowercase().contains(&sub_query) {
                    return false;
                }
            } else if query.starts_with("!genre(") && query.ends_with(")") && query.len() > 9 {
                let sub_query = query[7..query.len() - 1].to_owned();
                if self.genre.to_lowercase().contains(&sub_query) {
                    return false;
                }
            } else if query.starts_with("!") {
                if self.title.to_lowercase().contains(query)
                    || self.artist.to_lowercase().contains(query)
                {
                    return false;
                }
            } else {
                if !self.title.to_lowercase().contains(query)
                    && !self.artist.to_lowercase().contains(query)
                {
                    return false;
                }
            }
        }

        return true;
    }
}

impl Songs {
    pub fn new(config: &Config, cache_path: &Path) -> Result<Songs, Error> {
        let mut song_map = Vec::new();

        if !config.is_manual_dir() && cache_path.exists() {
            if let Ok(file) = File::open(cache_path) {
                let songs_cached: Result<Vec<Song>, _> =
                    serde_json::from_reader(BufReader::new(file));
                if let Ok(song_cached) = songs_cached {
                    song_map = song_cached;
                }
            }
        }

        let mut songs = Songs {
            songs_data_library: song_map,
            showing_songs_library: SongList::All,
            songs_next: Vec::new(),
            songs_history: Vec::new(),
            active: None,
        };

        if songs.songs_data_library.is_empty() {
            println!(
                "\n{}",
                "Loading music from Music directory.\n
                This is only necessary the first run of Auditorium.\n
                This may take a moment..."
                    .stylize()
                    .with(config.color_border.into())
            );

            if let Some(errors) = songs.load_songs(config.music_directory()) {
                return Err(Error::msg(
                    errors
                        .iter()
                        .map(|e| e.to_string())
                        .reduce(|acc, e| format!("{}\n{}", acc, e))
                        .unwrap_or("Music FFProbe error".to_owned()),
                ));
            }

            if !config.is_manual_dir() {
                if let Ok(json) = serde_json::to_string(&songs.songs_data_library) {
                    if let Ok(mut cache_file) = File::create(cache_path) {
                        cache_file.write_all(json.as_bytes()).unwrap();
                    }
                }
            }
        }

        songs.songs_data_library.sort_by(|first, second| {
            first
                .artist
                .cmp(&second.artist)
                .then(first.title.cmp(&second.title))
        });

        Ok(songs)
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
                        self.songs_data_library.push(song);
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

    pub fn showing_songs_library(&self) -> Vec<&Song> {
        match &self.showing_songs_library {
            SongList::All => self.songs_data_library.iter().collect(),
            SongList::Filtered(indicies) => indicies
                .iter()
                .map(|index| self.songs_data_library.get(*index).unwrap())
                .collect::<Vec<&Song>>(),
        }
    }

    pub fn current_song_index(&self) -> Option<usize> {
        self.songs_next.get(0).copied()
    }

    pub fn current_song(&self) -> Option<&Song> {
        self.current_song_index()
            .map(|index| self.songs_data_library.get(index))
            .flatten()
    }

    pub fn next_playing(&self) -> Vec<&Song> {
        if self.songs_next.len() <= 1 {
            Vec::new()
        } else {
            self.songs_next[1..]
                .iter()
                .filter_map(|index| self.songs_data_library.get(*index))
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
            .map(|index| self.songs_data_library.get(index))
            .flatten()
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
        self.songs_data_library.len()
    }

    pub fn songs_in_next_up(&self) -> usize {
        self.songs_next.len()
    }

    pub fn clear_up_next(&mut self) {
        self.songs_next.truncate(1);
    }

    pub fn remove_next_up(&mut self, selected: usize) {
        self.songs_next.remove(selected);
    }

    pub fn next_by_index(&self, selected: usize) -> Option<usize> {
        self.songs_next.get(selected).copied()
    }

    pub fn push_back_all(&mut self) {
        let mut adding: Vec<usize> = match &self.showing_songs_library {
            SongList::All => (0..self.songs_in_library() - 1).collect(),
            SongList::Filtered(songs) => songs.clone(),
        };

        adding.shuffle(&mut rand::thread_rng());
        adding.iter().for_each(|song| {
            self.songs_next.push(*song);
        });
    }

    pub fn reload(&mut self, config: &Config) -> Result<(), Error> {
        self.showing_songs_library.clear();
        self.songs_data_library.clear();
        self.songs_next.clear();
        self.kill_current();

        if let Some(errors) = self.load_songs(config.music_directory()) {
            Err(Error::msg(
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .reduce(|acc, e| format!("{}\n{}", acc, e))
                    .unwrap_or("Music FFProbe error".to_owned()),
            ))
        } else {
            self.songs_data_library.sort_by(|first, second| {
                first
                    .artist
                    .cmp(&second.artist)
                    .then(first.title.cmp(&second.title))
            });

            if !config.is_manual_dir() {
                if let Ok(json) = serde_json::to_string(&self.songs_data_library) {
                    if let Ok(mut cache_file) = File::create(files::cache_path()?) {
                        cache_file.write_all(json.as_bytes()).unwrap();
                    }
                }
            }

            Ok(())
        }
    }

    pub fn filtered(&mut self, query: Option<&String>) {
        let query = query.map(|query| query.to_lowercase());

        let filtered: Vec<usize> = if query
            .as_ref()
            .map(|query| query.is_empty())
            .unwrap_or(false)
        {
            if self.songs_data_library.len() > 0 {
                (0..self.songs_data_library.len() - 1).collect()
            } else {
                Vec::new()
            }
        } else {
            self.songs_data_library
                .iter()
                .enumerate()
                .filter(|(_, song)| match &query {
                    Some(query) => song.matches_query(query),
                    None => true,
                })
                .map(|(i, _)| i)
                .collect()
        };

        self.showing_songs_library = SongList::Filtered(filtered)
    }

    pub fn unfiltered(&mut self) {
        self.showing_songs_library = SongList::All;
    }
}
