# Music Player

![](https://gist.githubusercontent.com/nate-craft/648bbda6337b503a5d703f86757e4647/raw/144cf1f5f80e9c5ac6b5efde45869d01feb2ccd9/brainmade.png)

Music Player is a cross-platform, local, simple, fast, and distraction free CLI application to listen to your music library

![Preview](assets/preview.png)

## Features

- Fetches song metadata for all tracks such as genre(s), artist(s), and title via ffprobe locally

- Plays tracks with the lightweight MPV background audio player

- Quick navigation with vim-style keybinds

- Easily adds tracks to "Play Later" to enable auto play

- Automatically saves song history to allow song repetition

- Simple "Play All" key to shuffle all local music

- Never requires leaving the terminal or using the mouse (although mouse support is built-in!)

- Uses minimal resources (under 1 Mb memory)

___

Auditorium can be installed via [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):
```bash
cargo install --git https://github.com/nate-craft/auditorium
```

Auditorium requires [mpv](https://github.com/mpv-player/mpv) to be installed.
