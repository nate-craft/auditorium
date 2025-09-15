#[cfg(feature = "mpris")]
pub mod mpris {
    use crate::app::App;
    use crate::app::Message;
    use crate::mpv::MpvCommand;
    use crate::mpv::MpvCommandFeedback;
    use color_eyre::Result;
    use mpris_server::LoopStatus;
    use mpris_server::Metadata;
    use mpris_server::PlaybackRate;
    use mpris_server::PlaybackStatus;
    use mpris_server::PlayerInterface;
    use mpris_server::RootInterface;
    use mpris_server::Server;
    use mpris_server::Time;
    use mpris_server::TrackId;
    use mpris_server::Volume;
    use mpris_server::zbus;
    use mpris_server::zbus::fdo;
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

    pub fn thread_mpris(app: Arc<Mutex<App>>) -> JoinHandle<Result<()>> {
        thread::spawn(move || {
            async_std::task::block_on(async {
                let _server = Server::new("auditorium", AuditoriumPlayer::from(app)).await?;
                async_std::future::pending::<()>().await;
                Ok(())
            })
        })
    }

    impl PlayerInterface for AuditoriumPlayer {
        async fn next(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::SongNext);
                break;
            }
            Ok(())
        }

        async fn previous(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::SongPrevious);
                break;
            }
            Ok(())
        }

        async fn pause(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::PauseToggle(true));
                break;
            }
            Ok(())
        }

        async fn play_pause(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::PauseToggle(!app.paused));
                break;
            }
            Ok(())
        }

        async fn stop(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::Stop);
                break;
            }
            Ok(())
        }

        async fn play(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::PauseToggle(false));
                break;
            }
            Ok(())
        }

        async fn seek(&self, time: Time) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                if time.is_negative() {
                    app.mpris_message_in = Some(Message::SongSeekBackward(time.as_secs() as i32));
                } else {
                    app.mpris_message_in = Some(Message::SongSeekForward(time.as_secs() as i32));
                }

                break;
            }
            Ok(())
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
            loop {
                let Ok(app) = self.app.try_lock() else {
                    continue;
                };

                if app.songs.current_song().is_none() {
                    return Ok(PlaybackStatus::Stopped);
                }

                match app.paused {
                    true => return Ok(PlaybackStatus::Paused),
                    false => return Ok(PlaybackStatus::Playing),
                }
            }
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
            loop {
                let Ok(app) = self.app.try_lock() else {
                    continue;
                };

                match app.songs.current_song() {
                    Some(song) => {
                        return Ok(Metadata::builder()
                            .artist(vec![song.artist.clone()])
                            .album(song.album.clone())
                            .genre(vec![song.genre.clone()])
                            .title(song.title.clone())
                            .track_number(song.track.parse().unwrap_or(0))
                            .build());
                    }
                    None => return Err(fdo::Error::Failed("No song playing".to_owned())),
                }
            }
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
                Ok(result) => {
                    if let MpvCommandFeedback::Int(position) = result {
                        Ok(Time::from_secs(position as i64))
                    } else {
                        Err(fdo::Error::Failed(
                            "Critial error: MPV get position returning incorrect type".to_owned(),
                        ))
                    }
                }
                Err(err) => Err(fdo::Error::Failed(err.to_string())),
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
            loop {
                let Ok(app) = self.app.try_lock() else {
                    continue;
                };
                return Ok(app.songs.current_song().is_some());
            }
        }

        async fn can_pause(&self) -> fdo::Result<bool> {
            return self.can_play().await;
        }

        async fn can_seek(&self) -> fdo::Result<bool> {
            return self.can_play().await;
        }

        async fn can_control(&self) -> fdo::Result<bool> {
            return self.can_play().await;
        }
    }

    impl RootInterface for AuditoriumPlayer {
        async fn raise(&self) -> fdo::Result<()> {
            Ok(())
        }

        async fn quit(&self) -> fdo::Result<()> {
            loop {
                let Ok(mut app) = self.app.try_lock() else {
                    continue;
                };
                app.mpris_message_in = Some(Message::Exit);
                return Ok(());
            }
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
