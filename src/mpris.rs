#[cfg(feature = "mpris")]
pub mod mpris {
    use crate::app::App;
    use crate::app::Message;
    use color_eyre::eyre::Error;
    use color_eyre::Result;
    use mpris_server::Player;
    use std::{
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
    };

    pub fn thread_mpris(app: Arc<Mutex<App>>) -> JoinHandle<Result<()>> {
        thread::spawn(move || {
            async_std::task::block_on(async {
                 let player = Player::builder("com.github.nate-craft.Auditorium")
                    .can_play(true)
                    .can_pause(true)
                    .can_seek(true)
                    .can_go_next(true)
                    .can_go_previous(true)
                    .can_set_fullscreen(false)
                    .build()
                    .await.map_err(|err| Error::new(err))?;

                let app_clone = app.clone();
                player.connect_play_pause(move |_| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        app.mpris_message = Some(Message::PauseToggle(!app.paused));
                        break; 
                    }
                });

                let app_clone = app.clone();
                player.connect_play(move |_| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        app.mpris_message = Some(Message::PauseToggle(true));
                        break; 
                    }
                });

                let app_clone = app.clone();
                player.connect_pause(move |_| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        app.mpris_message = Some(Message::PauseToggle(false));
                        break; 
                    }
                });

                let app_clone = app.clone();
                player.connect_seek(move |_, time| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        //TODO: allow time specific seeking from MPRIS
                        if time.is_negative() {
                            app.mpris_message = Some(Message::SongSeekBackward);
                        } else {                           
                            app.mpris_message = Some(Message::SongSeekForward);
                        }
                        break; 
                    }
                });

                let app_clone = app.clone();
                player.connect_next(move |_| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        app.mpris_message = Some(Message::SongNext);
                        break; 
                    }
                });

                let app_clone = app.clone();
                player.connect_previous(move |_| {
                    loop {
                        let Ok(mut app) = app_clone.try_lock() else {
                            continue;
                        };
                        app.mpris_message = Some(Message::SongPrevious);
                        break; 
                    }
                });                

                async_std::task::block_on(async {
                    player.run().await; 
                });


                Ok(())
            })
        })
    }
}
