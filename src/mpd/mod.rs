//! MPD Adapter — protocol transport and state machine. Thread: dedicated background thread.

#[cfg_attr(not(test), allow(dead_code))]
pub mod mock;
pub mod state_machine;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;
use serde::{Serialize, Deserialize};

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

/// A group of albums: (group_name, [(artist, album_name), ...]).
pub type AlbumGroup = Vec<(String, Vec<(String, String)>)>;

/// An item in the MPD playback queue with display metadata.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub position: i32,
    pub id: i32,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub file: String,
}

/// A directory entry from MPD's lsinfo command.
#[derive(Debug, Clone)]
pub enum DirEntry {
    Directory { path: String, name: String },
    File { path: String, name: String, duration: Option<f64>, format: Option<String>, audio: Option<String> },
    Playlist { path: String, name: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MPD connection failed: {0}")]
    Connection(#[from] std::io::Error),
    #[error("MPD protocol error: {0}")]
    Protocol(String),
    #[error("MPD error: {0}")]
    MpdError(String),
}

pub struct MpdAdapter {
    reader: BufReader<TcpStream>,
    stream: TcpStream,
}

impl MpdAdapter {
    pub fn connect(host: &str, port: u16) -> Result<Self, Error> {
        let stream = TcpStream::connect_timeout(
            &(host, port).to_socket_addrs()?.next().ok_or_else(|| {
                Error::Connection(std::io::Error::new(std::io::ErrorKind::NotFound, "could not resolve host"))
            })?,
            Duration::from_secs(5),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        // Read and validate the MPD protocol greeting line
        let mut greeting = String::new();
        match reader.read_line(&mut greeting) {
            Ok(0) => return Err(Error::Protocol("MPD closed connection during greeting".into())),
            Err(e) => return Err(Error::Connection(e)),
            Ok(_) => {
                log::info!("[adapter] MPD greeting: {}", greeting.trim());
                if !greeting.trim().starts_with("OK ") {
                    return Err(Error::Protocol(format!(
                        "Unexpected MPD greeting: {}",
                        greeting.trim()
                    )));
                }
            }
        }
        Ok(Self { reader, stream })
    }

    pub fn send_command(&mut self, command: &str) -> Result<Vec<String>, Error> {
        let t0 = std::time::Instant::now();
        let cmd = format!("{}\n", command);
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;

        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                log::error!("[adapter] send_command({command:?}) — connection closed after {:?}", t0.elapsed());
                return Err(Error::Protocol("Connection closed".into()));
            }
            let trimmed = line.trim_end();
            if trimmed.starts_with("OK") {
                break;
            }
            if trimmed.starts_with("ACK") {
                log::error!("[adapter] send_command({command:?}) — ACK error after {:?}: {trimmed}", t0.elapsed());
                return Err(Error::MpdError(trimmed.to_string()));
            }
            lines.push(trimmed.to_string());
        }
        let elapsed = t0.elapsed();
        if elapsed > Duration::from_millis(100) {
            log::warn!("[adapter] send_command({command:?}) took {:?}, {} lines", elapsed, lines.len());
        }
        Ok(lines)
    }

    pub fn status(&mut self) -> Result<HashMap<String, String>, Error> {
        let lines = self.send_command("status")?;
        let mut map = HashMap::new();
        for line in &lines {
            if let Some((key, value)) = line.split_once(": ") {
                map.insert(key.to_string(), value.to_string());
            }
        }
        Ok(map)
    }

    pub fn current_song(&mut self) -> Result<Option<HashMap<String, String>>, Error> {
        let lines = self.send_command("currentsong")?;
        if lines.is_empty() {
            return Ok(None);
        }
        let mut map = HashMap::new();
        for line in &lines {
            if let Some((key, value)) = line.split_once(": ") {
                map.insert(key.to_string(), value.to_string());
            }
        }
        Ok(Some(map))
    }

    pub fn play(&mut self) -> Result<(), Error> {
        self.send_command("play")?;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), Error> {
        self.send_command("pause")?;
        Ok(())
    }

    pub fn next_track(&mut self) -> Result<(), Error> {
        self.send_command("next")?;
        Ok(())
    }

    pub fn previous(&mut self) -> Result<(), Error> {
        self.send_command("previous")?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        self.send_command("stop")?;
        Ok(())
    }

    pub fn seek(&mut self, position: i64) -> Result<(), Error> {
        self.send_command(&format!("seekcur {}", position))?;
        Ok(())
    }

    /// Add a URI to the queue and return its playlist ID.
    pub fn addid(&mut self, uri: &str) -> Result<i32, Error> {
        let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!("addid \"{}\"", escaped))?;
        lines
            .first()
            .and_then(|l| l.strip_prefix("Id: "))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::Protocol("No Id in addid response".into()))
    }

    /// Fetch album art via MPD's `albumart` command. Returns raw JPEG/PNG bytes.
    pub fn albumart(&mut self, album: &str) -> Result<Option<Vec<u8>>, Error> {
        let uris = self.find_album_uris(album)?;
        let uri = match uris.first() {
            Some(u) => {
                log::debug!("[adapter] albumart: found URI for '{album}': {u}");
                u.clone()
            }
            None => {
                log::debug!("[adapter] albumart: no URIs found for '{album}'");
                return Ok(None);
            }
        };
        let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
        let cmd = format!("albumart \"{}\" 0\n", escaped);
        log::info!("[adapter] albumart CMD: {cmd:?}");
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;

        // Use a fresh reader for albumart response — the shared BufReader may have
        // stale buffered data from the preceding find_album_uris/send_command call.
        let mut reader = BufReader::new(self.stream.try_clone()?);

        let mut line = String::new();
        let mut size: Option<usize> = None;
        loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                log::error!("[adapter] albumart: connection closed");
                return Err(Error::Protocol("Connection closed during albumart".into()));
            }
            let trimmed = line.trim_end();
            log::debug!("[adapter] albumart line: {trimmed:?}");
            if trimmed.starts_with("OK") {
                break;
            }
            if trimmed.starts_with("ACK") {
                log::warn!("[adapter] albumart ACK: {trimmed}");
                return Err(Error::MpdError(trimmed.to_string()));
            }
            if let Some(s) = trimmed.strip_prefix("size: ") {
                size = s.parse().ok();
            } else if trimmed.starts_with("binary: ") {
                if let Some(sz) = size {
                    let buf_data = reader.buffer().to_vec();
                    let buf_len = buf_data.len();
                    reader.consume(buf_len);

                    let mut data = vec![0u8; sz];
                    let from_buffer = buf_len.min(sz);
                    data[..from_buffer].copy_from_slice(&buf_data[..from_buffer]);
                    if from_buffer < sz {
                        reader.get_mut().read_exact(&mut data[from_buffer..])?;
                    }
                    // Read trailing newline + OK
                    line.clear();
                    reader.read_line(&mut line)?;
                    line.clear();
                    reader.read_line(&mut line)?;
                    log::info!("[adapter] albumart: got {} bytes for '{album}'", data.len());
                    return Ok(Some(data));
                }
            }
        }
        log::info!("[adapter] albumart: no art for '{album}' — MPD sent no binary data");
        Ok(None)
    }

    /// Find all track URIs for an album.
    pub fn find_album_uris(&mut self, album: &str) -> Result<Vec<String>, Error> {
        let escaped = album.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!("find album \"{}\"", escaped))?;
        Ok(lines
            .into_iter()
            .filter_map(|l| l.strip_prefix("file: ").map(|s| s.to_string()))
            .collect())
    }

    /// List all album names in the library.
    pub fn list_album_names(&mut self) -> Result<Vec<String>, Error> {
        let lines = self.send_command("list album")?;
        Ok(lines
            .into_iter()
            .filter_map(|line| line.strip_prefix("Album: ").map(|s| s.to_string()))
            .collect())
    }

    /// Get the artist name for a given album title.
    pub fn album_artist(&mut self, album: &str) -> Result<Option<String>, Error> {
        let escaped = album.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!(r#"find album "{}""#, escaped))?;
        // find returns song metadata for all tracks; extract first Artist: line
        Ok(lines
            .into_iter()
            .find_map(|line| line.strip_prefix("Artist: ").map(|s| s.to_string())))
    }

    /// List all items in the playback queue via `playlistinfo`.
    pub fn list_queue(&mut self) -> Result<Vec<QueueEntry>, Error> {
        let lines = self.send_command("playlistinfo")?;
        let mut entries = Vec::new();
        let mut current = QueueEntry {
            position: 0, id: 0, title: None, artist: None,
            album: None, duration: None, file: String::new(),
        };
        for line in &lines {
            if let Some(val) = line.strip_prefix("file: ") {
                if !current.file.is_empty() {
                    entries.push(std::mem::replace(&mut current, QueueEntry {
                        position: 0, id: 0, title: None, artist: None,
                        album: None, duration: None, file: String::new(),
                    }));
                }
                current.file = val.to_string();
            } else if let Some(val) = line.strip_prefix("Title: ") {
                current.title = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("Artist: ") {
                current.artist = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("Album: ") {
                current.album = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("Duration: ") {
                current.duration = val.parse().ok();
            } else if let Some(val) = line.strip_prefix("Pos: ") {
                current.position = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Id: ") {
                current.id = val.parse().unwrap_or(0);
            }
        }
        if !current.file.is_empty() {
            entries.push(current);
        }
        Ok(entries)
    }

    /// Search albums by query, returns deduplicated (artist, album_name) pairs.
    pub fn search_albums(&mut self, query: &str) -> Result<Vec<(String, String)>, Error> {
        let escaped = query.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!("search any \"{}\"", escaped))?;
        let mut results: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut current_artist = String::new();
        // First pass: map album names to first artist encountered
        let mut album_artist: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for line in &lines {
            if line.starts_with("file: ") {
                current_artist.clear();
            } else if let Some(artist) = line.strip_prefix("Artist: ") {
                current_artist = artist.to_string();
            } else if let Some(artist) = line.strip_prefix("AlbumArtist: ") {
                // AlbumArtist takes priority over per-track Artist
                current_artist = artist.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                album_artist.entry(album.to_string())
                    .or_insert_with(|| current_artist.clone());
            }
        }
        // Second pass: build results in order
        for line in &lines {
            if let Some(album) = line.strip_prefix("Album: ") {
                if seen.insert(album.to_string()) {
                    if let Some(artist) = album_artist.get(album) {
                        results.push((artist.clone(), album.to_string()));
                    }
                }
            }
        }
        Ok(results)
    }

    /// List directory contents via MPD's lsinfo command.
    pub fn lsinfo(&mut self, path: &str) -> Result<Vec<DirEntry>, Error> {
        let escaped = if path.is_empty() { String::new() } else {
            format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
        };
        let lines = self.send_command(&format!("lsinfo {}", escaped))?;
        let mut entries = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_playlist: Option<String> = None;
        let mut duration: Option<f64> = None;
        let mut format_str: Option<String> = None;
        let mut audio: Option<String> = None;

        fn push_entry(
            entries: &mut Vec<DirEntry>, file: &mut Option<String>,
            pl: &mut Option<String>, dur: &mut Option<f64>, fmt: &mut Option<String>, aud: &mut Option<String>,
        ) {
            if let Some(f) = file.take() {
                let name = f.rsplit('/').next().unwrap_or(&f).to_string();
                entries.push(DirEntry::File { path: f, name, duration: dur.take(), format: fmt.take(), audio: aud.take() });
            }
            if let Some(n) = pl.take() {
                let name = n.rsplit('/').next().unwrap_or(&n).to_string();
                entries.push(DirEntry::Playlist { path: n, name });
            }
        }

        for line in &lines {
            if let Some(d) = line.strip_prefix("directory: ") {
                push_entry(&mut entries, &mut current_file, &mut current_playlist, &mut duration, &mut format_str, &mut audio);
                entries.push(DirEntry::Directory { path: d.to_string(), name: d.rsplit('/').next().unwrap_or(d).to_string() });
            } else if let Some(f) = line.strip_prefix("file: ") {
                push_entry(&mut entries, &mut current_file, &mut current_playlist, &mut duration, &mut format_str, &mut audio);
                current_file = Some(f.to_string());
            } else if let Some(p) = line.strip_prefix("playlist: ") {
                push_entry(&mut entries, &mut current_file, &mut current_playlist, &mut duration, &mut format_str, &mut audio);
                current_playlist = Some(p.to_string());
            } else if let Some(dur) = line.strip_prefix("duration: ") {
                duration = dur.parse().ok();
            } else if let Some(f) = line.strip_prefix("Format: ") {
                format_str = Some(f.to_string());
            } else if let Some(a) = line.strip_prefix("Audio: ") {
                audio = Some(a.to_string());
            }
        }
        push_entry(&mut entries, &mut current_file, &mut current_playlist, &mut duration, &mut format_str, &mut audio);
        Ok(entries)
    }

    /// Fetch all albums with their artist names.
    pub fn list_albums(&mut self) -> Result<Vec<(String, String)>, Error> {
        let lines = self.send_command("list album group Artist")?;
        let mut albums: Vec<(String, String)> = Vec::new();
        let mut current_artist = String::new();
        for line in lines {
            if let Some(artist) = line.strip_prefix("Artist: ") {
                current_artist = artist.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                albums.push((current_artist.clone(), album.to_string()));
            }
        }
        albums.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        Ok(albums)
    }

    /// Fetch albums grouped by the given type ("Artist", "Date", "Genre", or "Albums" for flat).
    pub fn list_albums_grouped(&mut self, group: &str) -> Result<AlbumGroup, Error> {
        if group == "Albums" || group.is_empty() {
            let flat = self.list_albums()?;
            return Ok(vec![("All Albums".into(), flat)]);
        }
        let lines = self.send_command(&format!("list album group {group}"))?;
        let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut current_header = String::new();
        let mut current_items: Vec<(String, String)> = Vec::new();
        let header_prefix = format!("{group}: ");
        for line in lines {
            if let Some(name) = line.strip_prefix(&header_prefix) {
                if !current_header.is_empty() {
                    groups.push((current_header.clone(), std::mem::take(&mut current_items)));
                }
                current_header = name.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                // For Artist grouping the header IS the artist; for Date/Genre it's unavailable
                let artist = if group == "Artist" { &current_header } else { "" };
                current_items.push((artist.to_string(), album.to_string()));
            }
        }
        if !current_header.is_empty() {
            groups.push((current_header, current_items));
        }

        // Collect all album names that appear in any group
        let mut grouped_albums: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (_, albums) in &groups {
            for (_, name) in albums {
                grouped_albums.insert(name.clone());
            }
        }

        // Find albums from the flat list that are not in any group
        let unknown_label = match group {
            "Date" => "Unknown Year",
            "Genre" => "Unknown Genre",
            _ => "Unknown",
        };
        let untagged: Vec<(String, String)> = self.list_albums()?
            .into_iter()
            .filter(|(_, name)| !grouped_albums.contains(name))
            .collect();
        if !untagged.is_empty() {
            groups.push((unknown_label.into(), untagged));
        }

        Ok(groups)
    }
}
