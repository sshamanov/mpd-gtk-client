use mpd_client::mpd::MpdAdapter;
use std::path::PathBuf;

pub use mpd_client::mpd::mock::MockMpdServer;

/// Starts a MockMpdServer and creates an MpdAdapter connected to it.
pub fn with_mpd_server() -> (MockMpdServer, MpdAdapter) {
    let server = MockMpdServer::new();
    let client = MpdAdapter::connect("127.0.0.1", server.addr().port()).unwrap();
    (server, client)
}

#[allow(dead_code)]
pub fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(name);
    path
}
