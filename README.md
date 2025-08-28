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

- Built-in fuzzy finder with category-specific searching 

- Theming support with a hot-reloadable configuration

- Hot-reloadable music directory so you never need to exit the program

- Never requires leaving the terminal or using the mouse (although mouse support is built-in!)

- Extremely light memory usage

___

## Installation

Auditorium can be installed via [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):
```bash
cargo install --git https://github.com/nate-craft/auditorium
```

Auditorium requires [mpv](https://mpv.io/) and [ffmpeg](https://ffmpeg.org/index.html?) to be installed.

___

## Key Binds

### Global

- `Shift+r`       : Reload configuration
- `r`             : Reload music directory
- `Tab/Shift+Tab` : Navigate to next panel

### Player

- `Space`         : Play/Pause current song
- `</>`           : Next/Previous song
- `Left\Right`    : Seek forward/backward

### Up Next

- `Backspace|d`   : Remove from "Up Next"
- `Enter`         : Play now
- `j/k|Up/Down`   : Navigation current selection
- `c`             : Clear "Up Next"

### Library

- `a`             : Add all to "Up Next"
- `/`             : Fuzzy finding search
- `j/k|Up/Down`   : Navigation current selection
- `Enter`         : Add song to "Up Next"

___

## Fuzzy Finder

The built-in fuzzy finder can be activated by pressing `/` or by pressing `Enter` on the search box. It can
accept multiple search queries separated by commas. Any non categorized query will filter on artist and
song title, accepting if it matches either. Otherwise, a category tag must be used.

Examples:

```sh
# Jazz songs by Laufey
genre(Jazz),Laufey
# Any rock song
genre(Rock)
# Everything but Metallica
!Metallica
# Every song, but no rock, metal, nor rap
!genre(Rock),!genre(Metal),!genre(Rap)
```

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
