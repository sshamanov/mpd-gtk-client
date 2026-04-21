use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Serialize, Deserialize};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub cover_path: Option<PathBuf>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub album_id: String,
    pub path: PathBuf,
    pub duration: Option<Duration>,
    pub format: Option<AudioFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioFormat {
    Pcm { bit_depth: u16, sample_rate: u32 },
    Dsd { rate: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub position: usize,
    pub track_id: String,
    pub album_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MPD connection failed: {0}")]
    Connection(#[from] std::io::Error),
    #[error("MPD protocol error: {0}")]
    Protocol(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("MPD error: {0}")]
    MpdError(String),
}

pub struct MpdAdapter {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl MpdAdapter {
    pub async fn connect(host: &str, port: u16) -> Result<Self, Error> {
        let stream = TcpStream::connect((host, port)).await?;
        let (reader, writer) = stream.into_split();
        let reader = BufReader::new(reader);
        Ok(Self { reader, writer })
    }

    async fn send_command(&mut self, command: &str) -> Result<Vec<String>, Error> {
        let cmd = format!("{}\n", command);
        self.writer.write_all(cmd.as_bytes()).await?;
        self.writer.flush().await?;

        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line).await?;
            if n == 0 {
                return Err(Error::Protocol("Connection closed".into()));
            }
            let trimmed = line.trim_end();
            if trimmed.starts_with("OK") {
                break;
            }
            if trimmed.starts_with("ACK") {
                return Err(Error::MpdError(trimmed.to_string()));
            }
            lines.push(trimmed.to_string());
        }
        Ok(lines)
    }

    pub async fn status(&mut self) -> Result<HashMap<String, String>, Error> {
        let lines = self.send_command("status").await?;
        let mut map = HashMap::new();
        for line in lines {
            if let Some((key, value)) = line.split_once(": ") {
                map.insert(key.to_string(), value.to_string());
            }
        }
        Ok(map)
    }

    pub async fn current_song(&mut self) -> Result<Option<HashMap<String, String>>, Error> {
        let lines = self.send_command("currentsong").await?;
        if lines.is_empty() {
            return Ok(None);
        }
        let mut map = HashMap::new();
        for line in lines {
            if let Some((key, value)) = line.split_once(": ") {
                map.insert(key.to_string(), value.to_string());
            }
        }
        Ok(Some(map))
    }

    pub async fn play(&mut self) -> Result<(), Error> {
        self.send_command("play").await?;
        Ok(())
    }

    pub async fn pause(&mut self) -> Result<(), Error> {
        self.send_command("pause").await?;
        Ok(())
    }

    pub async fn next(&mut self) -> Result<(), Error> {
        self.send_command("next").await?;
        Ok(())
    }

    pub async fn previous(&mut self) -> Result<(), Error> {
        self.send_command("previous").await?;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), Error> {
        self.send_command("stop").await?;
        Ok(())
    }

    pub async fn seek(&mut self, position: u64) -> Result<(), Error> {
        self.send_command(&format!("seekcur {}", position)).await?;
        Ok(())
    }
}