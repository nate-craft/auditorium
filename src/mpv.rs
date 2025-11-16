use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
};

use serde_json::{Value, json};

use crate::{MPV_SOCKET, utilities::progress_formatted};

pub enum MpvCommand {
    TogglePause(bool),
    GetProgress,
    GetPosition,
    Seek(i32),
}

pub enum MpvCommandFeedback {
    Void,
    String(String),
    Int(i32),
}

#[cfg(target_os = "windows")]
impl MpvCommand {
    pub fn run(&self) -> Result<MpvCommandFeedback, std::io::Error> {
        match self {
            MpvCommand::TogglePause(_) => Ok(MpvCommandFeedback::Void),
            MpvCommand::GetProgress => Ok(MpvCommandFeedback::String("Not Supported".to_string())),
            MpvCommand::GetPosition => Ok(MpvCommandFeedback::Int(0)),
            MpvCommand::Seek(_) => Ok(MpvCommandFeedback::Void),
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl MpvCommand {
    /// See: https://mpv.io/manual/stable/#list-of-input-commands
    pub fn run(&self) -> Result<MpvCommandFeedback, std::io::Error> {
        match self {
            MpvCommand::Seek(target) => {
                let cmd = json!({"command" : ["seek", target.to_string(), "relative"]}).to_string();
                Self::send_to_ipc(&cmd).map(|_| MpvCommandFeedback::Void)
            }
            MpvCommand::TogglePause(paused) => {
                let cmd_get = json!({"command" : ["set_property", "pause", paused]}).to_string();
                Self::send_to_ipc(&cmd_get).map(|_| MpvCommandFeedback::Void)
            }
            MpvCommand::GetPosition => {
                let cmd = json!({"command" : ["get_property", "playback-time"]}).to_string();
                let json =
                    serde_json::from_str::<HashMap<String, Value>>(&Self::read_from_ipc(&cmd)?)?;

                let progress = json
                    .get("data")
                    .ok_or(std::io::Error::other("Couldn't parse json data from IPC"))?
                    .as_f64()
                    .ok_or(std::io::Error::other("Couldn't parse json float from IPC"))?
                    as i32;

                Ok(MpvCommandFeedback::Int(progress))
            }
            MpvCommand::GetProgress => {
                let cmd_now = json!({"command" : ["get_property", "playback-time"]}).to_string();
                let cmd_total = json!({"command" : ["get_property", "duration"]}).to_string();

                let json_now = serde_json::from_str::<HashMap<String, Value>>(
                    &Self::read_from_ipc(&cmd_now)?,
                )?;
                let json_total = serde_json::from_str::<HashMap<String, Value>>(
                    &Self::read_from_ipc(&cmd_total)?,
                )?;

                let now = json_now
                    .get("data")
                    .ok_or(std::io::Error::other("Couldn't parse json data from IPC"))?
                    .as_f64()
                    .ok_or(std::io::Error::other("Couldn't parse json float from IPC"))?
                    as i32;

                let total = json_total
                    .get("data")
                    .ok_or(std::io::Error::other("Couldn't parse json data from IPC"))?
                    .as_f64()
                    .ok_or(std::io::Error::other("Couldn't parse json float from IPC"))?
                    as i32;

                Ok(MpvCommandFeedback::String(format!(
                    "{} / {}",
                    progress_formatted(now),
                    progress_formatted(total)
                )))
            }
        }
    }

    fn read_from_ipc(command: &str) -> Result<String, std::io::Error> {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect(MPV_SOCKET)?;
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")?;

        let mut input = String::new();

        let mut reader = BufReader::new(&stream);
        reader.read_line(&mut input)?;
        Ok(input)
    }

    fn send_to_ipc(command: &str) -> Result<(), std::io::Error> {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect(MPV_SOCKET)?;
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")
    }
}
