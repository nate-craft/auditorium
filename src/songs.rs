use std::{
    fs::File,
    io::{self, BufReader, Write},
    option::Option,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use color_eyre::eyre::Error;
use ffprobe::FfProbeError;
use id3::{Tag, TagLike, partial_tag_ok};
use random_number::rand::{self, seq::SliceRandom};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

use crate::{MPV_SOCKET, files};
use crate::{app::SongLoadingState, files::Config};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Song {
    pub id: u64,
    pub title: String,
    pub genres: Vec<String>,
    pub artist: String,
    pub album: String,
    pub track: String,
    pub path: PathBuf,
    pub cover: Option<String>,
}

pub struct ActiveSong {
    pub child: Option<Child>,
    #[cfg(feature = "image")]
    pub cover: Option<ratatui_image::protocol::StatefulProtocol>,
    #[cfg(not(feature = "image"))]
    pub cover: Option<()>,
    pub marked_dead: bool,
}

pub struct Songs {
    pub showing_songs_library: SongList,
    songs_data_library: Vec<Song>,
    songs_next: Vec<usize>,
    songs_history: Vec<usize>,
    active: ActiveSong,
}

pub enum SongList {
    All,
    Filtered(Vec<usize>),
}

impl ActiveSong {
    fn new() -> ActiveSong {
        ActiveSong {
            child: None,
            marked_dead: false,
            cover: None,
        }
    }

    fn with_song(song: Option<&Song>) -> Result<ActiveSong, io::Error> {
        let child = match song {
            Some(song) => Some(song.play_single()?),
            None => None,
        };

        #[cfg(feature = "image")]
        return Ok(ActiveSong {
            child,
            marked_dead: false,
            cover: song
                .map(|song| {
                    use image::ImageReader;
                    use ratatui_image::picker::Picker;

                    let Some(cover) = &song.cover else {
                        return None;
                    };

                    let Ok(picker) = Picker::from_query_stdio() else {
                        return None;
                    };

                    let Ok(Ok(cover)) =
                        ImageReader::open(PathBuf::from(cover)).map(|image| image.decode())
                    else {
                        return None;
                    };

                    let dim = cover.height().min(cover.width());
                    Some(picker.new_resize_protocol(cover.crop_imm(0, 0, dim, dim)))
                })
                .flatten(),
        });

        #[cfg(not(feature = "image"))]
        return Ok(ActiveSong {
            child,
            cover: None,
            marked_dead: false,
        });
    }

    fn try_kill(&mut self) -> Result<(), io::Error> {
        self.marked_dead = true;
        let result = self
            .child
            .as_mut()
            .map(|child| {
                let result = if let Err(err) = child.kill().map(|_| ()) {
                    Err(err)
                } else {
                    child.wait().map(|_| ())
                };

                result
            })
            .unwrap_or(Ok(()));

        if result.is_ok() {
            self.child = None;
        }

        result
    }

    fn is_running(&mut self) -> bool {
        if let Some(active) = &mut self.child.as_mut() {
            if let Ok(status) = active.try_wait() {
                return status.is_none();
            }
        }
        return false;
    }
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
    fn new(file_name: &Path) -> Result<Song, Error> {
        let path = file_name.to_owned();
        let tags = Tag::read_from_path(file_name);
        let Ok(tags) = partial_tag_ok(tags) else {
            return Self::from_ffprobe(file_name).map_err(|err| Error::new(err));
        };

        let title = tags.title().unwrap_or("Unknown").to_owned();
        let genres = tags
            .genres()
            .map(|vec| {
                if vec.first().map(|first| first.is_empty()).unwrap_or(true) {
                    vec!["Unknown".to_owned()]
                } else {
                    vec.into_iter().map(|str| str.to_owned()).collect()
                }
            })
            .unwrap_or(vec!["Unknown".to_owned()]);
        let artist = tags.artist().unwrap_or("Unknown").to_owned();
        let album = tags.album().unwrap_or("Single").to_owned();
        let track = tags
            .track()
            .map(|track| track.to_string())
            .unwrap_or("1".to_owned());
        let id = rand::random::<u64>();

        let cover =
            if let Some(picture) = tags.pictures().next().map(|picture| picture.data.clone()) {
                let mut cover_path = files::art_path()?;
                cover_path.push(format!("{}.jpg", &id));
                let mut file = File::create(&cover_path)?;
                file.write_all(&picture)?;
                Some(cover_path.to_string_lossy().to_string())
            } else {
                None
            };

        let song = Ok(Song {
            id,
            title,
            genres,
            artist,
            album,
            track,
            path,
            cover,
        });

        return song;
    }

    fn from_ffprobe(file_name: &Path) -> Result<Song, FfProbeError> {
        let probe = ffprobe::ffprobe(&file_name)?;
        let mut title: String = "Unknown".to_owned();
        let mut genres: Vec<String> = vec!["Unknown".to_owned()];
        let mut artist: String = "Unknown".to_owned();
        let mut album: String = "Single".to_owned();
        let mut track: String = "1".to_owned();
        let path = file_name.to_owned();

        probe.format.tags.map(|tags| {
            tags.extra.get("title").map(|title_inner| {
                title_inner.as_str().map(|title_inner| {
                    title = title_inner.to_owned();
                });
            });
            tags.extra.get("genre").map(|genre_inner| {
                genre_inner.as_str().map(|genre_inner| {
                    genres = vec![genre_inner.to_owned()];
                });
            });
            tags.extra.get("artist").map(|artist_inner| {
                artist_inner.as_str().map(|author_inner| {
                    artist = author_inner.to_owned();
                });
            });
            tags.extra.get("album").map(|album_inner| {
                album_inner.as_str().map(|album_inner| {
                    album = album_inner.to_owned();
                });
            });
            tags.extra.get("track").map(|track_inner| {
                track_inner.as_str().map(|track_inner| {
                    let track_raw = track_inner.to_owned();
                    let split = track_raw.split_once("/");
                    if let Some(split) = split {
                        if split.0.starts_with("0") && split.0.len() > 1 {
                            track = split.0[1..].to_owned();
                        } else if split.0.starts_with("0") {
                            track = String::from("0");
                        } else {
                            track = split.0.to_owned();
                        }
                    } else {
                        if track_raw.starts_with("0") && track_raw.len() > 1 {
                            track = track_raw[1..].to_owned();
                        } else if track_raw.starts_with("0") {
                            track = String::from("0");
                        } else {
                            track = track_raw;
                        }
                    }
                });
            });
        });

        return Ok(Song {
            id: rand::random::<u64>(),
            title,
            genres,
            artist,
            album,
            track,
            path,
            cover: None,
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
                for genre in &self.genres {
                    if !genre.to_lowercase().contains(&sub_query) {
                        return false;
                    }
                }
            } else if query.starts_with("!genre(") && query.ends_with(")") && query.len() > 9 {
                let sub_query = query[7..query.len() - 1].to_owned();

                for genre in &self.genres {
                    if genre.to_lowercase().contains(&sub_query) {
                        return false;
                    }
                }
            } else if query.starts_with("album(") && query.ends_with(")") && query.len() > 8 {
                let sub_query = query[6..query.len() - 1].to_owned();
                if !self.album.to_lowercase().contains(&sub_query) {
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
            active: ActiveSong::new(),
        };

        if !songs.songs_data_library.is_empty() {
            return Ok(songs);
        }

        match songs.load_songs(config) {
            Err(errors) => Err(errors),
            Ok(loaded) => {
                songs.songs_data_library = loaded;
                Self::sort(&mut songs.songs_data_library);

                if !config.is_manual_dir() {
                    if let Ok(json) = serde_json::to_string(&songs.songs_data_library) {
                        if let Ok(mut cache_file) = File::create(cache_path) {
                            cache_file.write_all(json.as_bytes()).unwrap();
                        }
                    }
                }

                Ok(songs)
            }
        }
    }

    pub fn load_songs(&mut self, config: &Config) -> Result<Vec<Song>, Error> {
        let mut song_paths = Vec::new();
        files::art_path_delete()?;
        Self::load_dir(&config.music_directory(), &mut song_paths);

        rlimit::increase_nofile_limit(u64::MAX).unwrap();

        let results: Vec<Result<Song, Error>> =
            song_paths.par_iter().map(|path| Song::new(path)).collect();

        let valid: Vec<&Song> = results
            .iter()
            .filter_map(|song| song.as_ref().ok())
            .collect();

        if valid.len() == results.len() {
            Ok(results.into_iter().filter_map(|song| song.ok()).collect())
        } else {
            Err(Error::msg(
                results
                    .into_iter()
                    .filter_map(|result| result.err().map(|err| err.to_string()))
                    .reduce(|acc, e| format!("{}\n{}", acc, e))
                    .unwrap_or("Music Error".to_owned()),
            ))
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
        let _ = self.active.try_kill();
    }

    pub fn song_is_running(&mut self) -> bool {
        return self.active.is_running();
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

    pub fn try_play_current_song(&mut self) -> Result<(), io::Error> {
        self.active = ActiveSong::with_song(self.current_song())?;
        Ok(())
    }

    pub fn previous(&mut self) {
        if let Some(previous) = self.last_played_index() {
            self.songs_history.remove(self.songs_history.len() - 1);
            self.songs_next.insert(0, previous);
        }
    }

    pub fn active_exists(&self) -> bool {
        self.active.child.is_some()
    }

    pub fn active_command_mut(&mut self) -> &mut ActiveSong {
        &mut self.active
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

        match self.load_songs(config) {
            Ok(loaded) => {
                self.songs_data_library = loaded;
                Self::sort(&mut self.songs_data_library);

                if !config.is_manual_dir() {
                    if let Ok(json) = serde_json::to_string(&self.songs_data_library) {
                        if let Ok(mut cache_file) = File::create(files::cache_path()?) {
                            cache_file.write_all(json.as_bytes()).unwrap();
                        }
                    }
                }

                Ok(())
            }
            Err(errors) => Err(errors),
        }
    }

    pub fn filter_apply(&mut self, query: Option<&String>) {
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

    pub fn unfiltered_apply(&mut self) {
        self.showing_songs_library = SongList::All;
    }

    fn load_dir(dir: &Path, song_paths: &mut Vec<PathBuf>) {
        if let Ok(child) = dir.read_dir() {
            child.for_each(|child_result| {
                let Ok(child) = child_result else {
                    return;
                };

                let child_path = child.path();
                if child_path.is_dir() {
                    Self::load_dir(&child_path, song_paths);
                } else {
                    song_paths.push(child_path);
                }
            });
        }
    }

    fn sort(songs: &mut Vec<Song>) {
        songs.sort_by(|first, second| {
            first
                .artist
                .cmp(&second.artist)
                .then(first.album.cmp(&second.album))
                .then(first.track.cmp(&second.track))
                .then(first.title.cmp(&second.title))
        });
    }
}
