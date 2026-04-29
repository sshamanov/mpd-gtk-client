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
                        handle_client(stream, &received_clone);
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
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
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

fn handle_client(stream: TcpStream, received: &Arc<Mutex<Vec<String>>>) {
    let reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut writer = stream;

    let _ = writeln!(writer, "OK MPD 0.24.0");

    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed == "close" {
                    if trimmed == "close" {
                        received.lock().unwrap().push("close".to_string());
                    }
                    break;
                }
                let cmd = trimmed.split_whitespace().next().unwrap_or(trimmed);
                received.lock().unwrap().push(cmd.to_string());
                let response = get_response(trimmed);
                for resp_line in response {
                    let _ = writeln!(writer, "{resp_line}");
                }
                let _ = writeln!(writer, "OK");
            }
            Err(_) => break,
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
        c if c.starts_with("search ") => make_search_response(),
        c if c == "list album" || c.starts_with("list album group") => make_list_albums_response(),
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

fn make_search_response() -> Vec<String> {
    vec![
        "file: test/01-test.flac".into(),
        "Artist: Test Artist".into(),
        "Album: Test Album".into(),
        "file: test/02-second.flac".into(),
        "Artist: Second Artist".into(),
        "Album: Second Album".into(),
    ]
}

fn make_list_albums_response() -> Vec<String> {
    vec![
        "Artist: Test Artist".into(),
        "Album: Test Album".into(),
        "Artist: Second Artist".into(),
        "Album: Second Album".into(),
        "Artist: Third Artist".into(),
        "Album: Third Album".into(),
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
