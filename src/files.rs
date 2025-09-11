use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use color_eyre::eyre::Error;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub color_border: Color,
    pub color_headers: Color,
    pub color_row: Color,
    music_directory: PathBuf,
    #[serde(skip)]
    manual_music_directory: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            color_border: Color::Yellow,
            color_headers: Color::Green,
            color_row: Color::Indexed(246),
            music_directory: music_path().unwrap_or(Path::new("Music").to_path_buf()),
            manual_music_directory: None,
        }
    }
}

impl Config {
    pub fn with_dir(dir: Option<PathBuf>) -> Result<Config, Error> {
        Self::new().map(|mut config| {
            config.manual_music_directory = dir.clone();
            config
        })
    }

    pub fn new() -> Result<Config, Error> {
        let path = config_path()?;

        if let Ok(file) = File::open(&path) {
            serde_json::from_reader(BufReader::new(file)).map_err(|err| Error::new(err))
        } else {
            let defaults = Config::default();
            let json = serde_json::to_string_pretty(&defaults).map_err(|err| Error::new(err))?;
            let parent = path
                .parent()
                .ok_or(Error::msg("Could not get cache parent directory!"))?;
            if !parent.exists() {
                fs::create_dir(parent).map_err(|err| Error::new(err))?;
            }
            fs::write(&path, json).map_err(|err| Error::new(err))?;
            Ok(defaults)
        }
    }

    pub fn is_manual_dir(&self) -> bool {
        self.manual_music_directory
            .as_ref()
            .map(|dir| !dir.eq(&self.music_directory))
            .unwrap_or(false)
    }

    pub fn music_directory(&self) -> &PathBuf {
        if let Some(dir) = &self.manual_music_directory {
            &dir
        } else {
            &self.music_directory
        }
    }

    pub fn reload(&mut self) -> Result<(), Error> {
        *self = Self::with_dir(self.manual_music_directory.clone())?;
        Ok(())
    }
}

pub fn cache_path() -> Result<PathBuf, Error> {
    let root_dir = dirs::cache_dir().ok_or(Error::msg("Could not load cache directory!"))?;
    let cache_dir = root_dir.join("auditorium");
    if !cache_dir.exists() {
        fs::create_dir(&cache_dir).map_err(|err| Error::new(err))?;
    }
    Ok(cache_dir.join("cache.json"))
}

fn music_path() -> Result<PathBuf, Error> {
    dirs::audio_dir()
        .ok_or(Error::msg("Could not load music directory"))
        .map(|path| Ok(path))
        .unwrap_or(
            dirs::home_dir()
                .map(|home| home.join("Music"))
                .ok_or(Error::msg("Could not load home directory!")),
        )
}

fn config_path() -> Result<PathBuf, Error> {
    dirs::config_local_dir()
        .map(|dir| dir.join("auditorium").join("config.json"))
        .ok_or(Error::msg("Could not load local data directory!"))
}
