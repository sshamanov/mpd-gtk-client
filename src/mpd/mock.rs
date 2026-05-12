//! Mock MPD server — test-only mock that speaks the MPD text protocol. For use in integration tests.
//! Uses a real TCP listener on a random port.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// A mock MPD server that binds to a random port and responds to MPD commands with canned data.
/// Designed for integration testing of MpdAdapter without requiring a real MPD instance.
#[doc(hidden)]
pub struct MockMpdServer {
    addr: SocketAddr,
    received: Arc<Mutex<Vec<String>>>,
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    idle_subsystems: Arc<Mutex<Vec<String>>>,
    idle_unknown_cmd: Arc<AtomicBool>,
}

#[doc(hidden)]
impl MockMpdServer {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind mock server");
        let addr = listener.local_addr().expect("Failed to get address");
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let idle_subsystems_clone = Arc::new(Mutex::new(vec!["player".to_string()]));
        let idle_for_client = idle_subsystems_clone.clone();
        let idle_unknown_cmd = Arc::new(AtomicBool::new(false));
        let idle_unknown_for_client = idle_unknown_cmd.clone();

        let handle = thread::spawn(move || {
            listener
                .set_nonblocking(true)
                .expect("Failed to set non-blocking");
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }
                match stream {
                    Ok(stream) => {
                        if let Err(e) = stream.set_nonblocking(false) {
                            eprintln!("MockMpdServer: set_nonblocking(false) failed: {e}");
                            continue;
                        }
                        handle_client(stream, &received_clone, &idle_for_client, &idle_unknown_for_client);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => {
                        eprintln!("MockMpdServer: accept error: {e}");
                        break;
                    }
                }
            }
        });

        Self {
            addr,
            received,
            handle: Some(handle),
            stop,
            idle_subsystems: idle_subsystems_clone,
            idle_unknown_cmd,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Set the subsystems that the mock returns when `noidle` is sent.
    /// Defaults to `["player"]`.
    /// When enabled, the mock returns "unknown command" for the `idle` command
    /// to simulate MPD < 0.19.
    pub fn set_idle_unknown_command(&self, enabled: bool) {
        self.idle_unknown_cmd.store(enabled, Ordering::Release);
    }

    pub fn set_idle_subsystems(&self, subsystems: Vec<String>) {
        *self.idle_subsystems.lock().unwrap() = subsystems;
    }

    pub fn assert_received(&self, cmd: &str) {
        let received = self.received.lock().unwrap();
        assert!(
            received.iter().any(|r| r == cmd),
            "Expected command '{cmd}' was not received. Received: {received:?}"
        );
    }
}

impl Default for MockMpdServer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MockMpdServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_client(
    stream: TcpStream,
    received: &Arc<Mutex<Vec<String>>>,
    idle_subsystems: &Arc<Mutex<Vec<String>>>,
    idle_unknown_cmd: &AtomicBool,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut writer = stream;

    let _ = writeln!(writer, "OK MPD 0.24.0");

    let mut batch_mode = false;
    let mut batch_responses: Vec<String> = Vec::new();
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        match reader.read_line(&mut line_buf) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = line_buf.trim().to_string();
        if trimmed.is_empty() || trimmed == "close" {
            if trimmed == "close" {
                received.lock().unwrap().push("close".to_string());
            }
            break;
        }
        if trimmed == "command_list_begin" {
            batch_mode = true;
            batch_responses.clear();
            continue;
        }
        if trimmed == "command_list_end" {
            for resp_line in &batch_responses {
                let _ = writeln!(writer, "{resp_line}");
            }
            let _ = writeln!(writer, "OK");
            batch_mode = false;
            batch_responses.clear();
            continue;
        }
        let cmd = trimmed.split_whitespace().next().unwrap_or(&trimmed).to_string();

        // -- Idle handling --
        if cmd == "idle" {
            if idle_unknown_cmd.load(Ordering::Acquire) {
                received.lock().unwrap().push("idle".to_string());
                let _ = writeln!(writer, "ACK [5@0] {} unknown command", "idle");
                continue;
            }
            received.lock().unwrap().push("idle".to_string());
            let mut idle_buf = String::new();
            loop {
                idle_buf.clear();
                match reader.read_line(&mut idle_buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        let t = idle_buf.trim().to_string();
                        if t == "noidle" {
                            received.lock().unwrap().push("noidle".to_string());
                            let subsystems = idle_subsystems.lock().unwrap().clone();
                            for sub in &subsystems {
                                let _ = writeln!(writer, "changed: {sub}");
                            }
                            let _ = writeln!(writer, "OK");
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            continue;
        }
        if cmd == "noidle" {
            received.lock().unwrap().push("noidle".to_string());
            let _ = writeln!(writer, "OK");
            continue;
        }

        received.lock().unwrap().push(cmd);
        let response = get_response(&trimmed);
        if batch_mode {
            for resp_line in response {
                batch_responses.push(resp_line);
            }
        } else {
            for resp_line in response {
                let _ = writeln!(writer, "{resp_line}");
            }
            let _ = writeln!(writer, "OK");
        }
    }
}

fn get_response(command: &str) -> Vec<String> {
    match command {
        "status" => vec![
            "volume: 80".into(),
            "repeat: 0".into(),
            "random: 0".into(),
            "single: 0".into(),
            "consume: 0".into(),
            "playlist: 42".into(),
            "playlistlength: 10".into(),
            "state: play".into(),
            "song: 0".into(),
            "elapsed: 12.345".into(),
            "duration: 240.000".into(),
            "nextsong: 1".into(),
            "nextsongid: 11".into(),
        ],
        "currentsong" => vec![
            "file: test/01-test.flac".into(),
            "Artist: Test Artist".into(),
            "Title: Test Track".into(),
            "Album: Test Album".into(),
            "date: 2024".into(),
            "duration: 240.000".into(),
        ],
        "playlistinfo" => make_playlistinfo_response(),
        "play" | "pause" | "next" | "previous" | "stop" => vec![],
        "clear" => vec![],
        c if c.starts_with("play ") => vec![],
        c if c.starts_with("pause 0") => vec![],
        c if c.starts_with("seekcur ") => vec![],
        c if c.starts_with("deleteid ") => vec![],
        c if c.starts_with("moveid ") => vec![],
        c if c.starts_with("addid ") => vec!["Id: 99".into()],
        c if c.starts_with("search ") => make_search_response(c),
        c if c == "list album" => make_list_albums_response(),
        c if c.starts_with("list album group AlbumArtist") => make_list_album_artist_response(),
        c if c.starts_with("list album group Date") => make_list_date_response(),
        c if c.starts_with("list album group Genre") => make_list_genre_response(),
        c if c.starts_with("list artist group album") => make_list_artist_group_album_response(),
        c if c.starts_with("find album ") => make_find_album_response(),
        c if c.starts_with("lsinfo") => make_lsinfo_response(command),
        _ => vec![],
    }
}

fn make_playlistinfo_response() -> Vec<String> {
    vec![
        "file: test/01-test.flac".into(),
        "Last-Modified: 2024-01-01T00:00:00Z".into(),
        "Artist: Test Artist".into(),
        "Title: Test Track".into(),
        "Album: Test Album".into(),
        "duration: 240.000".into(),
        "Pos: 0".into(),
        "Id: 10".into(),
        "file: test/02-second.flac".into(),
        "Last-Modified: 2024-01-01T00:00:00Z".into(),
        "Artist: Second Artist".into(),
        "Title: Second Track".into(),
        "Album: Second Album".into(),
        "duration: 180.000".into(),
        "Pos: 1".into(),
        "Id: 11".into(),
        "file: test/03-third.flac".into(),
        "Artist: Third Artist".into(),
        "Title: Third Track".into(),
        "Album: Third Album".into(),
        "duration: 300.000".into(),
        "Pos: 2".into(),
        "Id: 12".into(),
    ]
}

fn make_search_response(command: &str) -> Vec<String> {
    // Dispatch on query text so tests can request specific edge-case responses
    if command.contains("missing-artist") {
        return make_search_missing_artist_response();
    }
    if command.contains("no-artist") {
        return make_search_no_artist_response();
    }
    if command.contains("albumartist") {
        return make_search_albumartist_response();
    }
    // Default response: all tracks have Artist tags
    vec![
        "file: test/01-test.flac".into(),
        "Artist: Test Artist".into(),
        "Album: Test Album".into(),
        "file: test/02-second.flac".into(),
        "Artist: Second Artist".into(),
        "Album: Second Album".into(),
    ]
}

/// Search response where some tracks in the same album are missing Artist tags.
fn make_search_missing_artist_response() -> Vec<String> {
    vec![
        "file: test/01-first.flac".into(),
        "Album: Split Album".into(),                // No Artist before this Album
        "file: test/02-second.flac".into(),
        "Artist: Real Artist".into(),
        "Album: Split Album".into(),                // Later track has the artist
        "file: test/03-third.flac".into(),
        "Artist: Other Artist".into(),
        "Album: Other Album".into(),
    ]
}

/// Search response where NO tracks have Artist tags.
fn make_search_no_artist_response() -> Vec<String> {
    vec![
        "file: test/01-track.flac".into(),
        "Album: Artistless Album".into(),           // No Artist: at all
        "file: test/02-track.flac".into(),
        "Album: Another Artistless".into(),         // No Artist: at all
    ]
}

/// Search response where AlbumArtist is used instead of Artist.
fn make_search_albumartist_response() -> Vec<String> {
    vec![
        "file: test/01-comp.flac".into(),
        "AlbumArtist: Various Artists".into(),
        "Title: Track One".into(),
        "Album: Compilation Album".into(),
        "file: test/02-comp.flac".into(),
        "AlbumArtist: Various Artists".into(),
        "Title: Track Two".into(),
        "Album: Compilation Album".into(),
    ]
}

fn make_list_albums_response() -> Vec<String> {
    vec![
        "Album: Second Album".into(),
        "Album: Test Album".into(),
        "Album: Third Album".into(),
    ]
}

fn make_list_album_artist_response() -> Vec<String> {
    vec![
        "AlbumArtist: Second Artist".into(),
        "Album: Second Album".into(),
        "AlbumArtist: Test Artist".into(),
        "Album: Test Album".into(),
        "AlbumArtist: Third Artist".into(),
        "Album: Third Album".into(),
    ]
}

fn make_list_date_response() -> Vec<String> {
    vec![
        "Date: 2020".into(),
        "Album: Second Album".into(),
        "Date: 2022".into(),
        "Album: Test Album".into(),
        "Date: 2024".into(),
        "Album: Third Album".into(),
    ]
}

fn make_list_genre_response() -> Vec<String> {
    vec![
        "Genre: Rock".into(),
        "Album: Second Album".into(),
        "Genre: Jazz".into(),
        "Album: Test Album".into(),
        "Genre: Electronic".into(),
        "Album: Third Album".into(),
    ]
}

fn make_list_artist_group_album_response() -> Vec<String> {
    vec![
        "Album: Second Album".into(),
        "Artist: Second Artist".into(),
        "Album: Test Album".into(),
        "Artist: Test Artist".into(),
        "Artist: Featured Artist".into(),
        "Album: Third Album".into(),
        "Artist: Third Artist".into(),
    ]
}

fn make_find_album_response() -> Vec<String> {
    vec![
        "file: test/01-test.flac".into(),
        "Artist: Test Artist".into(),
        "Album: Test Album".into(),
        "file: test/01-test-2.flac".into(),
        "Artist: Test Artist".into(),
        "Album: Test Album".into(),
    ]
}

fn make_lsinfo_response(command: &str) -> Vec<String> {
    if command == "lsinfo" || command == "lsinfo \"\"" {
        // Root directory listing
        vec![
            "directory: Artist One".into(),
            "directory: Artist Two".into(),
            "playlist: my-playlist.m3u".into(),
            "file: loose-track.flac".into(),
            "duration: 200.000".into(),
            "Format: 44100:16:2".into(),
        ]
    } else {
        // Subdirectory listing
        vec![
            "file: subdir/track1.flac".into(),
            "duration: 240.000".into(),
            "Artist: Sub Artist".into(),
            "Title: Sub Track".into(),
            "Album: Sub Album".into(),
            "file: subdir/track2.flac".into(),
            "duration: 180.000".into(),
        ]
    }
}
