#[cfg(feature = "mpris")]
pub mod mpris {
    use crate::app::App;
    use crate::app::Message;
    use crate::app::NavState;
    use crate::mpv::MpvCommand;
    use crate::mpv::MpvCommandFeedback;
    use color_eyre::Result;

    use mpris_server::LoopStatus;
    use mpris_server::Metadata;
    use mpris_server::PlaybackRate;
    use mpris_server::PlaybackStatus;
    use mpris_server::PlayerInterface;
    use mpris_server::Property;
    use mpris_server::RootInterface;
    use mpris_server::Server;

    use mpris_server::Time;
    use mpris_server::TrackId;
    use mpris_server::Volume;
    use mpris_server::zbus;
    use mpris_server::zbus::fdo;

    use std::time::Duration;
    use std::{
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
    };

    struct AuditoriumPlayer {
        app: Arc<Mutex<App>>,
    }

    impl From<Arc<Mutex<App>>> for AuditoriumPlayer {
        fn from(app: Arc<Mutex<App>>) -> Self {
            Self { app }
        }
    }

    fn metadata_current(app: &App) -> Option<Metadata> {
        app.songs.current_song().map(|song| {
            let mut builder = Metadata::builder()
                .artist(vec![song.artist.clone()])
                .album(song.album.clone())
                .genre(song.genres.clone())
                .title(song.title.clone())
                .track_number(song.track.parse().unwrap_or(0));

            if let Some(cover) = &song.cover {
                builder = builder.art_url(cover);
            }

            builder.build()
        })
    }

    fn paused_current(app: &App, paused: bool) -> PlaybackStatus {
        match (app.songs.current_song().is_some(), paused) {
            (true, true) => PlaybackStatus::Paused,
            (true, false) => PlaybackStatus::Playing,
            (false, _) => PlaybackStatus::Stopped,
        }
    }

    pub fn thread_mpris(app: Arc<Mutex<App>>) -> JoinHandle<Result<()>> {
        thread::spawn(move || {
            smol::block_on(async {
                let app_ref = app.clone();
                let server = Server::new("auditorium", AuditoriumPlayer::from(app_ref)).await?;

                loop {
                    let Ok(app) = app.try_lock() else {
                        continue;
                    };

                    if app.nav_state == NavState::Exit {
                        return Ok(());
                    }

                    let mut messages = Vec::new();
                    while let Ok(msg) = app.mpris_message_out.1.try_recv() {
                        messages.push(msg);
                    }

                    for msg in messages.into_iter() {
                        match msg {
                            Message::Exit => return Ok(()),
                            Message::PauseToggle(paused) => {
                                server
                                    .properties_changed([Property::PlaybackStatus(paused_current(
                                        &app, paused,
                                    ))])
                                    .await?;
                            }
                            Message::Stop => {
                                server
                                    .properties_changed([Property::PlaybackStatus(
                                        PlaybackStatus::Stopped,
                                    )])
                                    .await?;
                            }
                            Message::SongNext
                            | Message::PlayAll
                            | Message::SongPrevious
                            | Message::ReloadMusic
                            | Message::MoveSong => {
                                if let Some(metadata) = metadata_current(&app) {
                                    server
                                        .properties_changed([Property::Metadata(metadata)])
                                        .await?;
                                } else {
                                    server
                                        .properties_changed([Property::Metadata(Metadata::new())])
                                        .await?;
                                }

                                server
                                    .properties_changed([Property::PlaybackStatus(paused_current(
                                        &app, app.paused,
                                    ))])
                                    .await?;
                            }
                            _ => {}
                        }
                    }

                    drop(app);
                    thread::sleep(Duration::from_millis(50));
                }
            })
        })
    }

    impl PlayerInterface for AuditoriumPlayer {
        async fn next(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::SongNext)
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn previous(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::SongPrevious)
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn pause(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::PauseToggle(true))
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn play_pause(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::PauseToggle(!app.paused))
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn stop(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::Stop)
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn play(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                let result = if app.songs.current_song().is_some() {
                    app.handle_message_mpris(Message::PauseToggle(false))
                } else {
                    app.handle_message_mpris(Message::PlayAll)
                };

                result.map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn seek(&self, time: Time) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::SongSeek(time.as_secs() as i32))
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn set_position(&self, _: TrackId, _: Time) -> fdo::Result<()> {
            Err(fdo::Error::NotSupported(
                "SetPosition is not supported".into(),
            ))
        }

        async fn open_uri(&self, _: String) -> fdo::Result<()> {
            Err(fdo::Error::NotSupported("OpenUri is not supported".into()))
        }

        async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
            App::do_once(self.app.clone(), |app| {
                Ok(match (app.paused, app.songs.current_song().is_none()) {
                    (true, _) => PlaybackStatus::Stopped,
                    (_, true) => PlaybackStatus::Paused,
                    (_, false) => PlaybackStatus::Playing,
                })
            })
        }

        async fn loop_status(&self) -> fdo::Result<LoopStatus> {
            Ok(LoopStatus::None)
        }

        async fn set_loop_status(&self, _: LoopStatus) -> zbus::Result<()> {
            Err(zbus::Error::from(fdo::Error::NotSupported(
                "SetLoopStatus is not supported".into(),
            )))
        }

        async fn rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn set_rate(&self, _: PlaybackRate) -> zbus::Result<()> {
            Err(zbus::Error::from(fdo::Error::NotSupported(
                "SetRate is not supported".into(),
            )))
        }

        async fn shuffle(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn set_shuffle(&self, _: bool) -> zbus::Result<()> {
            Err(zbus::Error::from(fdo::Error::NotSupported(
                "SetShuffle is not supported".into(),
            )))
        }

        async fn metadata(&self) -> fdo::Result<Metadata> {
            App::do_once(self.app.clone(), |app| {
                metadata_current(&app).ok_or(fdo::Error::Failed("No song playing".to_owned()))
            })
        }

        async fn volume(&self) -> fdo::Result<Volume> {
            Ok(1.0)
        }

        async fn set_volume(&self, _: Volume) -> zbus::Result<()> {
            Err(zbus::Error::from(fdo::Error::NotSupported(
                "SetVolume is not supported".into(),
            )))
        }

        async fn position(&self) -> fdo::Result<Time> {
            match MpvCommand::GetPosition.run() {
                Ok(MpvCommandFeedback::Int(position)) => Ok(Time::from_secs(position as i64)),
                Err(err) => Err(fdo::Error::Failed(err.to_string())),
                _ => Err(fdo::Error::Failed(
                    "Could not send internal message".to_owned(),
                )),
            }
        }

        async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
            Ok(1.0)
        }

        async fn can_go_next(&self) -> fdo::Result<bool> {
            Ok(true)
        }

        async fn can_go_previous(&self) -> fdo::Result<bool> {
            Ok(true)
        }

        async fn can_play(&self) -> fdo::Result<bool> {
            return Ok(true);
        }

        async fn can_pause(&self) -> fdo::Result<bool> {
            return Ok(true);
        }

        async fn can_seek(&self) -> fdo::Result<bool> {
            return Ok(true);
        }

        async fn can_control(&self) -> fdo::Result<bool> {
            return Ok(true);
        }
    }

    impl RootInterface for AuditoriumPlayer {
        async fn raise(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn quit(&self) -> fdo::Result<()> {
            App::do_once(self.app.clone(), |app| {
                app.handle_message_mpris(Message::Exit)
                    .map_err(|_| fdo::Error::Failed("Could not send internal message".to_owned()))
            })
        }

        async fn can_quit(&self) -> fdo::Result<bool> {
            Ok(true)
        }

        async fn fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn set_fullscreen(&self, _: bool) -> zbus::Result<()> {
            Err(zbus::Error::from(fdo::Error::NotSupported(
                "Fullscreen is not supported".into(),
            )))
        }

        async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn can_raise(&self) -> fdo::Result<bool> {
            Ok(true)
        }

        async fn has_track_list(&self) -> fdo::Result<bool> {
            Ok(false)
        }

        async fn identity(&self) -> fdo::Result<String> {
            Ok("Auditorium".to_string())
        }

        async fn desktop_entry(&self) -> fdo::Result<String> {
            Err(fdo::Error::NotSupported(String::from(
                "Desktop shortcut not available",
            )))
        }

        async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
            Ok(vec![])
        }
    }
}
