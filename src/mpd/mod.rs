//! MPD Adapter — protocol transport and state machine. Thread: dedicated background thread.

#[cfg_attr(not(test), allow(dead_code))]
pub mod mock;
pub mod state_machine;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use serde::{Serialize, Deserialize};

/// Unified stream type supporting both TCP and Unix sockets.
enum MpdStream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl Read for MpdStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            MpdStream::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            MpdStream::Unix(s) => s.read(buf),
        }
    }
}

impl Write for MpdStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            MpdStream::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            MpdStream::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            MpdStream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            MpdStream::Unix(s) => s.flush(),
        }
    }
}

impl MpdStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            MpdStream::Tcp(s) => s.set_read_timeout(timeout),
            #[cfg(unix)]
            MpdStream::Unix(_) => Ok(()), // Unix sockets don't need timeouts
        }
    }

}

/// Describes how to connect to an MPD server — used internally and in the state machine.
#[derive(Debug, Clone)]
pub enum ConnectionTarget {
    /// Auto-detect: Unix socket at common paths, then TCP fallback to localhost:6600.
    Auto,
    /// TCP connection to a host:port.
    Tcp(String, u16),
    /// Unix socket at an explicit path.
    Unix(String),
}

impl ConnectionTarget {
    /// Extract the TCP host:port, if this target is TCP.
    pub fn tcp_host_port(&self) -> Option<(&str, u16)> {
        match self {
            ConnectionTarget::Tcp(host, port) => Some((host.as_str(), *port)),
            _ => None,
        }
    }
}

/// Try connecting to MPD via Unix socket at common paths.
/// Returns the first successful connection, or None if all paths fail.
#[cfg(unix)]
fn try_unix_socket_connect() -> Result<MpdAdapter, Error> {
    let paths = [
        std::env::var("XDG_RUNTIME_DIR")
            .map(|d| std::path::PathBuf::from(d).join("mpd/socket"))
            .ok(),
        Some(std::path::PathBuf::from("/run/mpd/socket")),
    ];

    for path in paths.into_iter().flatten() {
        match UnixStream::connect(&path) {
            Ok(stream) => {
                log::info!("[adapter] connected via Unix socket at {}", path.display());
                let clone = MpdStream::Unix(stream.try_clone()?);
                let mut reader = BufReader::new(clone);
                let mut greeting = String::new();
                match reader.read_line(&mut greeting) {
                    Ok(0) | Err(_) => {
                        log::warn!("[adapter] Unix socket {}: no greeting, trying next", path.display());
                        continue;
                    }
                    Ok(_) => {
                        if !greeting.trim().starts_with("OK ") {
                            log::warn!("[adapter] Unix socket {}: bad greeting, trying next", path.display());
                            continue;
                        }
                        let ver_str = greeting.trim().to_string();
                        let version = MpdVersion::parse(&ver_str);
                        log::info!("[adapter] MPD greeting via Unix socket: {} (version {version})", greeting.trim());
                        let adap = MpdAdapter {
                            reader,
                            stream: MpdStream::Unix(stream),
                            protocol_version: Some(version.to_string()),
                            capabilities: MpdCapabilities::from_version(&version),
                        };
                        return Ok(adap);
                    }
                }
            }
            Err(e) => {
                log::debug!("[adapter] Unix socket {}: {e}, trying next", path.display());
            }
        }
    }
    Err(Error::Connection(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no Unix socket found",
    )))
}

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

/// Parsed MPD protocol version from the greeting banner.
#[derive(Debug, Clone, Default)]
pub struct MpdVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for MpdVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl MpdVersion {
    fn parse(greeting: &str) -> Self {
        // Greeting format: "OK MPD {major}.{minor}.{patch}" or "OK {major}.{minor}.{patch}"
        let s = greeting
            .strip_prefix("OK ")
            .and_then(|s| s.strip_prefix("MPD ").or(Some(s)))
            .unwrap_or("");
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 3 {
            Self {
                major: parts[0].parse().unwrap_or(0),
                minor: parts[1].parse().unwrap_or(0),
                patch: parts[2].parse().unwrap_or(0),
            }
        } else if parts.len() == 2 {
            Self {
                major: parts[0].parse().unwrap_or(0),
                minor: parts[1].parse().unwrap_or(0),
                patch: 0,
            }
        } else {
            Self::default()
        }
    }

    fn supports_readpicture(&self) -> bool {
        self.major >= 1 || (self.major == 0 && self.minor >= 24)
    }

    fn supports_albumart(&self) -> bool {
        self.major >= 1 || (self.major == 0 && self.minor >= 21)
    }
}

pub struct MpdAdapter {
    reader: BufReader<MpdStream>,
    stream: MpdStream,
    pub protocol_version: Option<String>,
    pub capabilities: MpdCapabilities,
}

/// Feature capability matrix computed from MPD protocol version.
#[derive(Debug, Clone)]
pub struct MpdCapabilities {
    pub readpicture: bool,
    pub albumart: bool,
}

impl MpdCapabilities {
    fn from_version(version: &MpdVersion) -> Self {
        Self {
            readpicture: version.supports_readpicture(),
            albumart: version.supports_albumart(),
        }
    }
}

impl Default for MpdCapabilities {
    fn default() -> Self {
        Self { readpicture: false, albumart: true }
    }
}

impl MpdAdapter {
    /// Connect to MPD via TCP host:port or Unix socket path.
    /// If `host` is "auto" or empty, attempts Unix socket auto-detection
    /// with fallback chain: $XDG_RUNTIME_DIR/mpd/socket → /run/mpd/socket → localhost:6600.
    pub fn connect(target: &ConnectionTarget) -> Result<Self, Error> {
        match target {
            ConnectionTarget::Auto => {
                #[cfg(unix)]
                if let Ok(adapter) = try_unix_socket_connect() {
                    return Ok(adapter);
                }
                Self::connect_tcp("localhost", 6600)
            }
            ConnectionTarget::Tcp(host, port) => Self::connect_tcp(host, *port),
            ConnectionTarget::Unix(path) => Self::connect_unix(path),
        }
    }

    /// Connect to MPD via a specific Unix socket path.
    fn connect_unix(path: &str) -> Result<Self, Error> {
        #[cfg(unix)]
        {
            let stream = UnixStream::connect(Path::new(path))?;
            log::info!("[adapter] connected via Unix socket at {path}");
            let clone = MpdStream::Unix(stream.try_clone()?);
            let mut reader = BufReader::new(clone);
            let mut greeting = String::new();
            match reader.read_line(&mut greeting) {
                Ok(0) | Err(_) => return Err(Error::Connection(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset, "no greeting from MPD",
                ))),
                Ok(_) => {
                    if !greeting.trim().starts_with("OK ") {
                        return Err(Error::Protocol("invalid MPD greeting".into()));
                    }
                    let version = greeting.trim().trim_start_matches("OK ").to_string();
                    Ok(MpdAdapter {
                        stream: MpdStream::Unix(stream),
                        reader,
                        protocol_version: Some(version),
                        capabilities: Default::default(),
                    })
                }
            }
        }
        #[cfg(not(unix))]
        Err(Error::Connection(std::io::Error::new(
            std::io::ErrorKind::Unsupported, "Unix sockets not supported on this platform",
        )))
    }

    fn connect_tcp(host: &str, port: u16) -> Result<Self, Error> {
        let stream = TcpStream::connect_timeout(
            &(host, port).to_socket_addrs()?.next().ok_or_else(|| {
                Error::Connection(std::io::Error::new(std::io::ErrorKind::NotFound, "could not resolve host"))
            })?,
            Duration::from_secs(5),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let reader_stream = stream.try_clone()?;
        let mut reader = BufReader::new(MpdStream::Tcp(reader_stream));
        // Read and validate the MPD protocol greeting line
        let mut greeting = String::new();
        let protocol_version = match reader.read_line(&mut greeting) {
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
                greeting.trim().to_string()
            }
        };
        let version = MpdVersion::parse(&protocol_version);
        log::info!("[adapter] MPD protocol version: {version}");
        Ok(Self {
            reader,
            stream: MpdStream::Tcp(stream),
            protocol_version: Some(version.to_string()),
            capabilities: MpdCapabilities::from_version(&version),
        })
    }

    /// Send multiple commands as a single MPD command list (command_list_begin/end).
    /// All commands execute atomically — MPD aborts the entire list on any failure.
    /// Returns the accumulated response lines or an error.
    pub fn send_batch(&mut self, commands: &[String]) -> Result<Vec<String>, Error> {
        let t0 = std::time::Instant::now();
        self.stream.write_all(b"command_list_begin\n")?;
        for cmd in commands {
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.write_all(b"\n")?;
        }
        self.stream.write_all(b"command_list_end\n")?;
        self.stream.flush()?;

        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                log::error!("[adapter] send_batch({} cmds) — connection closed after {:?}", commands.len(), t0.elapsed());
                return Err(Error::Protocol("Connection closed".into()));
            }
            let trimmed = line.trim_end();
            if trimmed.starts_with("OK") {
                break;
            }
            if trimmed.starts_with("ACK") {
                log::error!("[adapter] send_batch({} cmds) — ACK error after {:?}: {trimmed}", commands.len(), t0.elapsed());
                return Err(Error::MpdError(trimmed.to_string()));
            }
            lines.push(trimmed.to_string());
        }
        let elapsed = t0.elapsed();
        if elapsed > Duration::from_millis(100) {
            log::warn!("[adapter] send_batch({} cmds) took {:?}, {} lines", commands.len(), elapsed, lines.len());
        }
        Ok(lines)
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

    fn read_albumart_response(&mut self) -> Result<Vec<u8>, Error> {
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        self.stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        loop {
            let n = self.stream.read(&mut buf)?;
            if n == 0 { return Err(Error::Protocol("Connection closed".into())); }
            raw.extend_from_slice(&buf[..n]);
            if raw.ends_with(b"\nOK\n") { break; }
        }
        self.stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        Ok(raw)
    }

/// Parse the total picture size from an albumart response.
fn parse_albumart_size(raw: &[u8]) -> Option<usize> {
    let s = String::from_utf8_lossy(raw);
    s.lines().find_map(|l| l.strip_prefix("size: ").and_then(|v| v.parse().ok()))
}

/// Parse the mtime timestamp from a readpicture response.
fn parse_albumart_mtime(raw: &[u8]) -> Option<u64> {
    let s = String::from_utf8_lossy(raw);
    s.lines().find_map(|l| l.strip_prefix("mtime: ").and_then(|v| v.parse().ok()))
}

/// Parse the binary data chunk from an albumart response.
fn parse_albumart_chunk(raw: &[u8]) -> Vec<u8> {
    let header = b"binary: ";
    let Some(bin_pos) = raw.windows(header.len()).position(|w| w == header) else { return vec![] };
    let header_start = bin_pos + header.len();
    let Some(nl_pos) = raw[header_start..].iter().position(|&b| b == b'\n') else { return vec![] };
    let chunk_size: usize = std::str::from_utf8(&raw[header_start..header_start + nl_pos])
        .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let data_start = header_start + nl_pos + 1;
    let data_end = (data_start + chunk_size).min(raw.len());
    if data_end > data_start { raw[data_start..data_end].to_vec() } else { vec![] }
}

    /// Fetch album art via MPD's `albumart` command. Returns raw JPEG/PNG bytes.
    /// Issues multiple commands with increasing offsets to reassemble large images.
    pub fn albumart(&mut self, album: &str) -> Result<Option<Vec<u8>>, Error> {
        let uris = self.find_album_uris(album)?;
        let uri = match uris.first() {
            Some(u) => u.clone(),
            None => return Ok(None),
        };
        let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");

        // First request: get total size and first chunk
        let total_size = {
            let cmd = format!("albumart \"{}\" 0\n", escaped);
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.flush()?;
            let raw = self.read_albumart_response()?;
            if raw.is_empty() { return Ok(None); }
            Self::parse_albumart_size(&raw)
        };
        let Some(total_size) = total_size else { return Ok(None); };

        // Fetch all chunks with increasing offsets
        let mut data = Vec::with_capacity(total_size);
        let mut offset = 0usize;
        while data.len() < total_size {
            let cmd = format!("albumart \"{}\" {offset}\n", escaped);
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.flush()?;
            let raw = self.read_albumart_response()?;
            if raw.is_empty() { break; }
            let chunk = Self::parse_albumart_chunk(&raw);
            if chunk.is_empty() { break; }
            data.extend_from_slice(&chunk);
            offset += chunk.len();
        }
        if data.is_empty() { return Ok(None); }
        log::info!("[adapter] albumart: got {}/{} bytes for '{album}'", data.len(), total_size);
        Ok(Some(data))
    }

    /// Fetch embedded album art via MPD's `readpicture` command. Returns raw JPEG/PNG bytes
    /// plus the mtime timestamp. Requires MPD >= 0.24.
    /// Issues multiple commands with increasing offsets to reassemble large images.
    pub fn readpicture(&mut self, uri: &str) -> Result<Option<(Vec<u8>, u64)>, Error> {
        let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
        let cmd = format!("readpicture \"{}\" 0\n", escaped);
        self.stream.write_all(cmd.as_bytes())?;
        self.stream.flush()?;
        let raw = self.read_albumart_response()?;
        if raw.is_empty() { return Ok(None); }

        let total_size = Self::parse_albumart_size(&raw);
        let mtime = Self::parse_albumart_mtime(&raw);
        let Some(size) = total_size else { return Ok(None); };
        let Some(mtime_val) = mtime else {
            log::warn!("[adapter] readpicture: no mtime in response for '{uri}'");
            return Ok(None);
        };

        // Collect first chunk
        let mut data = Self::parse_albumart_chunk(&raw);

        // Fetch remaining chunks
        while data.len() < size {
            let cmd = format!("readpicture \"{}\" {}\n", escaped, data.len());
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.flush()?;
            let raw = self.read_albumart_response()?;
            if raw.is_empty() { break; }
            let chunk = Self::parse_albumart_chunk(&raw);
            if chunk.is_empty() { break; }
            data.extend_from_slice(&chunk);
        }

        if data.is_empty() { return Ok(None); }
        log::info!("[adapter] readpicture: got {}/{} bytes for '{uri}'", data.len(), size);
        Ok(Some((data, mtime_val)))
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

    /// Find all tracks for an album with metadata. Returns (title, file, duration).
    pub fn find_album_tracks(&mut self, album: &str) -> Result<Vec<(String, String, f64)>, Error> {
        let escaped = album.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!("find album \"{}\"", escaped))?;
        let mut tracks = Vec::new();
        let mut current_file = String::new();
        let mut current_title = String::new();
        let mut current_duration = 0.0_f64;
        for line in &lines {
            if let Some(file) = line.strip_prefix("file: ") {
                if !current_file.is_empty() {
                    tracks.push((std::mem::take(&mut current_title), std::mem::take(&mut current_file), current_duration));
                    current_duration = 0.0;
                }
                current_file = file.to_string();
            } else if let Some(title) = line.strip_prefix("Title: ") {
                current_title = title.to_string();
            } else if let Some(dur) = line.strip_prefix("Duration: ").and_then(|s| s.parse::<f64>().ok()) {
                current_duration = dur;
            }
        }
        if !current_file.is_empty() {
            tracks.push((current_title, current_file, current_duration));
        }
        Ok(tracks)
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
        Ok(Self::parse_queue_response(&lines))
    }

    /// Fetch queue changes since a given playlist version.
    /// Returns entries added or changed since that version.
    /// Does NOT report deletions — callers must cross-reference with playlist length from status.
    pub fn plchanges(&mut self, version: &str) -> Result<Vec<QueueEntry>, Error> {
        let lines = self.send_command(&format!("plchanges {version}"))?;
        Ok(Self::parse_queue_response(&lines))
    }

    /// Parse an MPD queue response (from `playlistinfo` or `plchanges`) into QueueEntry values.
    fn parse_queue_response(lines: &[String]) -> Vec<QueueEntry> {
        let mut entries = Vec::new();
        let mut current = QueueEntry {
            position: 0, id: 0, title: None, artist: None,
            album: None, duration: None, file: String::new(),
        };
        for line in lines {
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
        entries
    }

    /// Search filenames/paths by query. Returns (file_path, display_name) pairs.
    pub fn search_files(&mut self, query: &str) -> Result<Vec<(String, String)>, Error> {
        let escaped = query.replace('\\', "\\\\").replace('"', "\\\"");
        let lines = self.send_command(&format!("search filename \"{}\"", escaped))?;
        let mut results = Vec::new();
        for line in &lines {
            if let Some(file) = line.strip_prefix("file: ") {
                let name = file.rsplit('/').next().unwrap_or(file).to_string();
                results.push((file.to_string(), name));
            }
        }
        Ok(results)
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
                let album = album.to_string();
                if !current_artist.is_empty() {
                    // Insert or update: replace empty placeholder with real artist
                    album_artist.entry(album)
                        .and_modify(|e| { if e.is_empty() { *e = current_artist.clone(); } })
                        .or_insert_with(|| current_artist.clone());
                } else {
                    // No artist yet — insert empty placeholder (may be updated later or set to Unknown Artist)
                    album_artist.entry(album).or_insert_with(String::new);
                }
            }
        }
        // Replace any remaining empty artists with "Unknown Artist"
        for (_, artist) in album_artist.iter_mut() {
            if artist.is_empty() {
                *artist = "Unknown Artist".to_string();
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

        // For non-Artist groupings, fetch the flat album list to build an artist lookup map.
        // MPD's `list album group {group}` does not include Artist metadata for Date/Genre
        // groupings, so we backfill from the flat list grouped by Artist.
        let flat_albums: Option<Vec<(String, String)>> = if group != "Artist" {
            Some(self.list_albums()?)
        } else {
            None
        };
        let artist_lookup: std::collections::HashMap<&str, &str> = flat_albums
            .as_ref()
            .map(|albums| albums.iter().map(|(a, b)| (b.as_str(), a.as_str())).collect())
            .unwrap_or_default();

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
                // Artist grouping: header IS the artist
                // Date/Genre grouping: look up artist from the flat album list
                let artist = if group == "Artist" {
                    current_header.clone()
                } else {
                    artist_lookup
                        .get(album)
                        .unwrap_or(&"Unknown Artist")
                        .to_string()
                };
                current_items.push((artist, album.to_string()));
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
        let untagged: Vec<(String, String)> = if let Some(albums) = flat_albums {
            albums
                .into_iter()
                .filter(|(_, name)| !grouped_albums.contains(name))
                .collect()
        } else {
            self.list_albums()?
                .into_iter()
                .filter(|(_, name)| !grouped_albums.contains(name))
                .collect()
        };
        if !untagged.is_empty() {
            groups.push((unknown_label.into(), untagged));
        }

        Ok(groups)
    }
}
