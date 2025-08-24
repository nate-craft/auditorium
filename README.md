# Auditorium

![](https://gist.githubusercontent.com/nate-craft/648bbda6337b503a5d703f86757e4647/raw/144cf1f5f80e9c5ac6b5efde45869d01feb2ccd9/brainmade.png)

Auditorium is a cross-platform, local, simple, fast, and distraction free CLI application to listen to your music library

![Preview](assets/preview.png)

## Features

- Fetches song metadata for all tracks such as genre(s), artist(s), and title via ffprobe locally

- Plays tracks with the lightweight MPV background audio player

- Quick navigation with vim-style keybinds

- Easily adds tracks to "Play Later" to enable auto play

- Automatically saves song history to allow song repetition

- Simple "Play All" key to shuffle all local music

- Theming support with a hot-reloadable configuration

- Never requires leaving the terminal or using the mouse (although mouse support is built-in!)

- Uses minimal resources (under 1 Mb memory)

___

## Installation

Auditorium can be installed via [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):
```bash
cargo install --git https://github.com/nate-craft/auditorium
```

Auditorium requires [mpv](https://mpv.io/) and [ffmpeg](https://ffmpeg.org/index.html?) to be installed.

___

## Configuration

Auditorium's configuration can be found at `$XDG_CONFIG_HOME/auditorium/config.toml`.
It can be reloaded at any time with `Shift+R`

### Color Formatting

Color configuration values can be in the following formats:
```hocon
# Common Names
"color-example": "White"
# Hex
"color-example": "#FFFFFF"
# Indexed
"color-example": "0"
```
