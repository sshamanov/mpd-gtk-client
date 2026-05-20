//! MPD Adapter — protocol transport and state machine. Thread: dedicated background thread.

#[cfg_attr(not(test), allow(dead_code))]
pub mod mock;
pub mod state_machine;
pub mod cover;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use serde::{Serialize, Deserialize};

/// Unified stream type supporting both TCP and Unix sockets.
pub enum MpdStream {
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
            MpdStream::Unix(s) => s.set_read_timeout(timeout),
        }
    }

    /// Create a new stream handle sharing the same underlying connection.
    /// Write to one clone, read from the other — safe because MPD protocol is half-duplex.
    fn try_clone(&self) -> std::io::Result<Self> {
        match self {
            MpdStream::Tcp(s) => s.try_clone().map(MpdStream::Tcp),
            #[cfg(unix)]
            MpdStream::Unix(s) => s.try_clone().map(MpdStream::Unix),
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
                            closed: AtomicBool::new(false),
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
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub position: usize,
    pub track_id: String,
    pub album_id: String,
}

/// Cached album metadata — fetched once from MPD, reused for all group views.
/// Cover art is cached separately via the coverart module.
#[derive(Debug, Clone)]
pub struct AlbumMeta {
    pub album: String,
    pub album_artist: String,
    pub track_artists: Vec<String>,
    pub year: Option<String>,
    pub genre: Option<String>,
}

/// A group of albums: (group_name, [AlbumMeta, ...]).
pub type AlbumGroup = Vec<(String, Vec<AlbumMeta>)>;

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
    pub file_size: Option<u64>,
    pub mtime: Option<u64>,
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
        self.major >= 1 || (self.major == 0 && self.minor >= 22)
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
    closed: AtomicBool,
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
                    let protocol_version = greeting.trim().to_string();
                    let version = MpdVersion::parse(&protocol_version);
                    Ok(MpdAdapter {
                        stream: MpdStream::Unix(stream),
                        reader,
                        protocol_version: Some(version.to_string()),
                        capabilities: MpdCapabilities::from_version(&version),
                        closed: AtomicBool::new(false),
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
            closed: AtomicBool::new(false),
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

    /// Mark the adapter as closed so Drop doesn't send a redundant `close\n`.
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Return a cloned stream handle for writing `noidle` from another thread.
    /// The clone shares the same underlying TCP connection — writing to it
    /// while the worker thread blocks on `idle` is safe because MPD protocol
    /// is half-duplex and the reader/writer never contend.
    pub fn stream_clone(&self) -> std::io::Result<MpdStream> {
        self.stream.try_clone()
    }

    /// Send the MPD `idle` command and block until MPD responds with
    /// subsystem changes. Returns the list of changed subsystems.
    /// Returns `Err` on transient errors (connection reset, timeout).
    /// Returns `Ok(vec![])` if `noidle` was sent externally (empty response).
    pub fn idle(&mut self) -> Result<Vec<String>, Error> {
        self.stream.write_all(b"idle\n")?;
        self.stream.flush()?;

        // MPD idle blocks indefinitely until a subsystem changes. The 10s TCP
        // read timeout would kill the connection on a quiet server, causing a
        // flood of broken-pipe errors. Remove the timeout during idle.
        let t0 = std::time::Instant::now();
        let mut subsystems = Vec::new();
        let mut line = String::new();
        self.reader.get_mut().set_read_timeout(None)?;
        loop {
            line.clear();
            let n = match self.reader.read_line(&mut line) {
                Ok(n) => n,
                Err(e) => {
                    let _ = self.reader.get_mut().set_read_timeout(Some(Duration::from_secs(10)));
                    return Err(Error::Connection(e));
                }
            };
            if n == 0 {
                let _ = self.reader.get_mut().set_read_timeout(Some(Duration::from_secs(10)));
                return Err(Error::Protocol("Connection closed during idle".into()));
            }
            let trimmed = line.trim_end();
            if trimmed.starts_with("OK") {
                break;
            }
            if trimmed.starts_with("ACK") {
                let _ = self.reader.get_mut().set_read_timeout(Some(Duration::from_secs(10)));
                if trimmed.contains("unknown") {
                    log::info!("[adapter] MPD does not support idle command, falling back to polling");
                    return Err(Error::MpdError(trimmed.to_string()));
                }
                return Err(Error::MpdError(trimmed.to_string()));
            }
            if let Some(subsystem) = trimmed.strip_prefix("changed: ") {
                subsystems.push(subsystem.to_string());
            }
        }
        self.reader.get_mut().set_read_timeout(Some(Duration::from_secs(10)))?;
        let elapsed = t0.elapsed();
        log::debug!("[adapter] idle returned {:?} subsystems in {:?}", subsystems.len(), elapsed);
        Ok(subsystems)
    }

    /// Write `noidle\n` to break MPD out of an idle session.
    /// Safe to call from any thread holding a stream clone.
    /// Ignores the response (the original stream's reader will see it).
    pub fn noidle(&mut self) -> Result<(), Error> {
        self.stream.write_all(b"noidle\n")?;
        self.stream.flush()?;
        // Read the "OK" response so the BufReader stays in sync
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        Ok(())
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

    pub fn update_library(&mut self) -> Result<(), Error> {
        self.send_command("update")?;
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
        match uris.first() {
            Some(uri) => self.albumart_by_uri(uri, album),
            None => Ok(None),
        }
    }

    /// Fetch album art for a known URI — skips the `find_album_uris` round-trip.
    pub fn albumart_by_uri(&mut self, uri: &str, log_label: &str) -> Result<Option<Vec<u8>>, Error> {
        let escaped = uri.replace('\\', "\\\\").replace('"', "\\\"");
        let cmd = format!("albumart \"{}\" 0\n", escaped);

        // First request: get total size and first chunk
        let (total_size, first_chunk) = {
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.flush()?;
            let raw = self.read_albumart_response()?;
            if raw.is_empty() { return Ok(None); }
            let size = Self::parse_albumart_size(&raw);
            let chunk = Self::parse_albumart_chunk(&raw);
            (size, chunk)
        };
        let Some(total_size) = total_size else { return Ok(None); };

        // Fetch remaining chunks with increasing offsets
        let mut data = first_chunk;
        while data.len() < total_size {
            let cmd = format!("albumart \"{}\" {}\n", escaped, data.len());
            self.stream.write_all(cmd.as_bytes())?;
            self.stream.flush()?;
            let raw = self.read_albumart_response()?;
            if raw.is_empty() { break; }
            let chunk = Self::parse_albumart_chunk(&raw);
            if chunk.is_empty() { break; }
            data.extend_from_slice(&chunk);
        }
        if data.is_empty() { return Ok(None); }
        log::info!("[adapter] albumart: got {}/{} bytes for '{log_label}'", data.len(), total_size);
        Ok(Some(data))
    }

    /// Fetch embedded album art via MPD's `readpicture` command. Returns raw JPEG/PNG bytes
    /// plus the mtime timestamp. Requires MPD >= 0.22.
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
            file_size: None, mtime: None,
        };
        for line in lines {
            if let Some(val) = line.strip_prefix("file: ") {
                if !current.file.is_empty() {
                    entries.push(std::mem::replace(&mut current, QueueEntry {
                        position: 0, id: 0, title: None, artist: None,
                        album: None, duration: None, file: String::new(),
                        file_size: None, mtime: None,
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
            } else if let Some(val) = line.strip_prefix("file_size: ") {
                current.file_size = val.parse().ok();
            } else if let Some(val) = line.strip_prefix("mtime: ") {
                current.mtime = val.parse().ok();
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
        // Single pass over response lines: build artist map + preserve insertion order
        let mut current_artist = String::new();
        let mut album_artist: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut album_order: Vec<String> = Vec::new();
        for line in &lines {
            if line.starts_with("file: ") {
                current_artist.clear();
            } else if let Some(artist) = line.strip_prefix("Artist: ") {
                current_artist = artist.to_string();
            } else if let Some(artist) = line.strip_prefix("AlbumArtist: ") {
                current_artist = artist.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                let album = album.to_string();
                let is_new = !album_artist.contains_key(&album);
                if !current_artist.is_empty() {
                    album_artist.entry(album.clone())
                        .and_modify(|e| { if e.is_empty() { *e = current_artist.clone(); } })
                        .or_insert_with(|| current_artist.clone());
                } else {
                    album_artist.entry(album.clone()).or_insert_with(String::new);
                }
                if is_new {
                    album_order.push(album);
                }
            }
        }
        // Emit results in insertion order, replacing empty artists with "Unknown Artist"
        Ok(album_order.into_iter().map(|album| {
            let artist = album_artist.get(&album)
                .map(|a| if a.is_empty() { "Unknown Artist" } else { a.as_str() })
                .unwrap_or("Unknown Artist");
            (artist.to_string(), album)
        }).collect())
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

    /// Extract just the year (first 4 digits) from a Date tag that may be
/// a full date like "2024-03-15" or just "2024".
fn normalize_year(date: &str) -> String {
    date.split('-').next().unwrap_or(date).to_string()
}

/// Fetch all albums with full metadata (AlbumArtist, Date, Genre).
    /// Uses 3 separate MPD `list` commands (MPD only supports single `group`).
    /// Merges locally by album name — each query is one round-trip.
    pub fn list_albums_full(&mut self) -> Result<Vec<AlbumMeta>, Error> {
        // 1. Album→AlbumArtist mapping
        let lines_aa = self.send_command("list album group AlbumArtist")?;
        let mut album_to_artist: HashMap<String, String> = HashMap::new();
        let mut current_aa = String::new();
        for line in &lines_aa {
            if let Some(aa) = line.strip_prefix("AlbumArtist: ") {
                current_aa = aa.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                album_to_artist.insert(album.to_string(), current_aa.clone());
            }
        }

        // 2. Album→Date mapping
        let lines_date = self.send_command("list album group Date")?;
        let mut album_to_date: HashMap<String, String> = HashMap::new();
        let mut current_date = String::new();
        for line in &lines_date {
            if let Some(date) = line.strip_prefix("Date: ") {
                current_date = Self::normalize_year(date);
            } else if let Some(album) = line.strip_prefix("Album: ") {
                album_to_date.insert(album.to_string(), current_date.clone());
            }
        }

        // 3. Album→Genre mapping
        let lines_genre = self.send_command("list album group Genre")?;
        let mut album_to_genre: HashMap<String, String> = HashMap::new();
        let mut current_genre = String::new();
        for line in &lines_genre {
            if let Some(genre) = line.strip_prefix("Genre: ") {
                current_genre = genre.to_string();
            } else if let Some(album) = line.strip_prefix("Album: ") {
                album_to_genre.insert(album.to_string(), current_genre.clone());
            }
        }

        // 4. Album→TrackArtists mapping (artists that appear on each album's tracks)
        let lines_ta = self.send_command("list artist group album")?;
        let mut album_to_artists: HashMap<String, Vec<String>> = HashMap::new();
        let mut current_album_for_artist = String::new();
        for line in &lines_ta {
            if let Some(album) = line.strip_prefix("Album: ") {
                current_album_for_artist = album.to_string();
            } else if let Some(artist) = line.strip_prefix("Artist: ") {
                album_to_artists
                    .entry(current_album_for_artist.clone())
                    .or_default()
                    .push(artist.to_string());
            }
        }

        // Merge: collect unique album names from all sources
        let mut all_albums: Vec<AlbumMeta> = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new(); // album name → index

        let build_meta = |album: &str, aa: &str| -> AlbumMeta {
            let track_artists = album_to_artists.get(album)
                .map(|v| v.clone())
                .unwrap_or_default();
            AlbumMeta {
                album: album.to_string(),
                album_artist: aa.to_string(),
                track_artists,
                year: album_to_date.get(album).cloned(),
                genre: album_to_genre.get(album).cloned(),
            }
        };

        for album in album_to_artist.keys() {
            let aa = album_to_artist.get(album).cloned().unwrap_or_default();
            seen.insert(album.clone(), all_albums.len());
            all_albums.push(build_meta(album, &aa));
        }
        // Albums that appear in date/genre/artists but not album_artist
        for album in album_to_date.keys() {
            if !seen.contains_key(album) {
                seen.insert(album.clone(), all_albums.len());
                all_albums.push(build_meta(album, ""));
            }
        }
        for album in album_to_genre.keys() {
            if !seen.contains_key(album) {
                seen.insert(album.clone(), all_albums.len());
                all_albums.push(build_meta(album, ""));
            }
        }
        for album in album_to_artists.keys() {
            if !seen.contains_key(album) {
                seen.insert(album.clone(), all_albums.len());
                all_albums.push(build_meta(album, ""));
            }
        }

        all_albums.sort_by(|a, b| a.album.to_lowercase().cmp(&b.album.to_lowercase()));
        Ok(all_albums)
    }

    pub fn list_albums(&mut self) -> Result<Vec<AlbumMeta>, Error> {
        // Use the full metadata query for all album listing.
        // Kept as a thin wrapper for backward compat.
        self.list_albums_full()
    }

    /// Fetch file paths for all albums using a single `listallinfo` MPD command.
    /// Parses `file:`, `Album:` and `Artist:` lines to build album→file_paths mapping.
    /// Returns `Vec<(album_artist, album_name, file_paths)>`.
    pub fn fetch_album_file_paths(&mut self) -> Result<Vec<(String, String, Vec<String>)>, Error> {
        let lines = self.send_command("listallinfo")?;
        let mut current_file: Option<String> = None;
        let mut current_album: Option<String> = None;
        let mut current_artist: Option<String> = None;
        // Intermediate: (artist, album) → file paths
        let mut map: std::collections::HashMap<(String, String), Vec<String>> = std::collections::HashMap::new();

        for line in &lines {
            if let Some(file) = line.strip_prefix("file: ") {
                // Flush previous file before starting a new one
                if let (Some(artist), Some(album), Some(file_path)) =
                    (current_artist.take(), current_album.take(), current_file.take())
                {
                    map.entry((artist, album))
                        .or_default()
                        .push(file_path);
                }
                current_file = Some(file.to_string());
                current_album = None;
                current_artist = None;
            } else if let Some(album) = line.strip_prefix("Album: ") {
                current_album = Some(album.to_string());
            } else if let Some(artist) = line.strip_prefix("Artist: ") {
                current_artist = Some(artist.to_string());
            }
        }
        // Flush the last file
        if let (Some(artist), Some(album), Some(file_path)) =
            (current_artist, current_album, current_file)
        {
            map.entry((artist, album))
                .or_default()
                .push(file_path);
        }

        Ok(map
            .into_iter()
            .map(|((artist, album), paths)| (artist, album, paths))
            .collect())
    }

    /// Group cached album metadata locally by the given tag ("Artist", "AlbumArtist",
    /// "Date", "Genre", or "Albums" for flat). No MPD round-trip — uses the provided slice.
    pub fn list_albums_grouped(&self, group: &str, all_albums: &[AlbumMeta]) -> AlbumGroup {
        if group == "Albums" || group.is_empty() {
            return vec![("All Albums".into(), all_albums.to_vec())];
        }

        let group_key = |a: &AlbumMeta| -> String {
            match group {
                "Artist" | "AlbumArtist" => {
                    if a.album_artist.is_empty() {
                        "Unknown Artist".into()
                    } else {
                        a.album_artist.clone()
                    }
                }
                "Date" => a.year.clone().unwrap_or_else(|| "Unknown Year".into()),
                "Genre" => a.genre.clone().unwrap_or_else(|| "Unknown Genre".into()),
                _ => "Unknown".into(),
            }
        };

        let mut groups: Vec<(String, Vec<AlbumMeta>)> = Vec::new();
        let mut current_header = String::new();
        let mut current_items: Vec<AlbumMeta> = Vec::new();

        // Sort by group key first, then by album name
        let mut sorted = all_albums.to_vec();
        sorted.sort_by(|a, b| {
            let ka = group_key(a);
            let kb = group_key(b);
            ka.to_lowercase()
                .cmp(&kb.to_lowercase())
                .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase()))
        });

        for album in sorted {
            let key = group_key(&album);
            if key != current_header {
                if !current_header.is_empty() {
                    groups.push((std::mem::take(&mut current_header), std::mem::take(&mut current_items)));
                }
                current_header = key;
            }
            current_items.push(album);
        }
        if !current_header.is_empty() {
            groups.push((current_header, current_items));
        }

        groups
    }
}

impl Drop for MpdAdapter {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = writeln!(self.stream, "close");
            let _ = self.stream.flush();
        }
    }
}
