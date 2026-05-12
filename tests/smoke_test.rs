#[path = "common.rs"]
mod common;

#[test]
fn smoke_test_connect_and_status() {
    let (server, mut client) = common::with_mpd_server();

    let status = client.status().unwrap();
    assert_eq!(status.get("volume"), Some(&"80".to_string()));
    assert_eq!(status.get("state"), Some(&"play".to_string()));
    assert_eq!(status.get("song"), Some(&"0".to_string()));

    server.assert_received("status");
}

#[test]
fn smoke_test_current_song() {
    let (server, mut client) = common::with_mpd_server();

    let song = client.current_song().unwrap();
    assert!(song.is_some());
    let song = song.unwrap();
    assert_eq!(song.get("Artist"), Some(&"Test Artist".to_string()));
    assert_eq!(song.get("Title"), Some(&"Test Track".to_string()));

    server.assert_received("currentsong");
}

#[test]
fn smoke_test_playback_commands() {
    let (server, mut client) = common::with_mpd_server();

    client.play().unwrap();
    client.pause().unwrap();
    client.next_track().unwrap();
    client.previous().unwrap();
    client.stop().unwrap();

    server.assert_received("play");
    server.assert_received("pause");
    server.assert_received("next");
    server.assert_received("previous");
    server.assert_received("stop");
}

#[test]
fn test_list_queue() {
    let (server, mut client) = common::with_mpd_server();
    let queue = client.list_queue().unwrap();
    assert_eq!(queue.len(), 3);
    assert_eq!(queue[0].title.as_deref(), Some("Test Track"));
    assert_eq!(queue[0].position, 0);
    assert_eq!(queue[1].title.as_deref(), Some("Second Track"));
    assert_eq!(queue[2].title.as_deref(), Some("Third Track"));
    server.assert_received("playlistinfo");
}

#[test]
fn test_search_albums() {
    let (server, mut client) = common::with_mpd_server();
    let results = client.search_albums("test query").unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, "Test Album");
    assert_eq!(results[1].1, "Second Album");
    server.assert_received("search");
}

#[test]
fn test_search_albums_missing_artist() {
    let (server, mut client) = common::with_mpd_server();
    // The mock returns a response where Split Album's first track has no Artist but later track does
    let results = client.search_albums("missing-artist").unwrap();
    // Expect 2 albums, in file-order: "Split Album" then "Other Album"
    assert_eq!(results.len(), 2);
    // "Split Album" comes first (file order) — first track had no artist, second track fills it in
    assert_eq!(results[0].1, "Split Album");
    assert_eq!(results[0].0, "Real Artist");
    assert_eq!(results[1].1, "Other Album");
    assert_eq!(results[1].0, "Other Artist");
    server.assert_received("search");
}

#[test]
fn test_search_albums_all_missing_artist() {
    let (server, mut client) = common::with_mpd_server();
    let results = client.search_albums("no-artist").unwrap();
    // Both albums have no Artist tags — should show "Unknown Artist", in file order
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, "Artistless Album");
    assert_eq!(results[0].0, "Unknown Artist");
    assert_eq!(results[1].1, "Another Artistless");
    assert_eq!(results[1].0, "Unknown Artist");
    server.assert_received("search");
}

#[test]
fn test_search_albums_albumartist() {
    let (server, mut client) = common::with_mpd_server();
    let results = client.search_albums("albumartist").unwrap();
    // Album has only AlbumArtist tag — should use it
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, "Compilation Album");
    assert_eq!(results[0].0, "Various Artists");
    server.assert_received("search");
}

#[test]
fn test_list_albums_full() {
    let (server, mut client) = common::with_mpd_server();
    let albums = client.list_albums_full().unwrap();
    assert_eq!(albums.len(), 3);
    // Sorted alphabetically by album name
    assert_eq!(albums[0].album, "Second Album");
    assert_eq!(albums[0].album_artist, "Second Artist");
    assert_eq!(albums[0].year.as_deref(), Some("2020"));
    assert_eq!(albums[0].genre.as_deref(), Some("Rock"));
    assert_eq!(albums[1].album, "Test Album");
    assert_eq!(albums[2].album, "Third Album");
    server.assert_received("list");
}

#[test]
fn test_list_albums_grouped() {
    let (server, mut client) = common::with_mpd_server();
    // First fetch the full metadata
    let albums = client.list_albums_full().unwrap();
    // Then group locally by Artist
    let groups = client.list_albums_grouped("Artist", &albums);
    assert!(!groups.is_empty());
    // First group should have header and albums
    let (header, albums) = &groups[0];
    assert!(!header.is_empty());
    assert!(!albums.is_empty());
}

#[test]
fn test_lsinfo_root() {
    let (server, mut client) = common::with_mpd_server();
    let entries = client.lsinfo("").unwrap();
    assert!(!entries.is_empty());
    // Should have at least one directory
    let has_directory = entries.iter().any(|e| matches!(e, mpd_client::mpd::DirEntry::Directory { .. }));
    assert!(has_directory);
    server.assert_received("lsinfo");
}

#[test]
fn test_find_album_uris() {
    let (server, mut client) = common::with_mpd_server();
    let uris = client.find_album_uris("Test Album").unwrap();
    assert_eq!(uris.len(), 2);
    assert_eq!(uris[0], "test/01-test.flac");
    assert_eq!(uris[1], "test/01-test-2.flac");
    server.assert_received("find");
}

#[test]
fn test_addid() {
    let (server, mut client) = common::with_mpd_server();
    let id = client.addid("test/some-track.flac").unwrap();
    assert_eq!(id, 99);
    server.assert_received("addid");
}

#[test]
fn test_queue_mutation_commands() {
    let (server, mut client) = common::with_mpd_server();
    client.send_command("deleteid 10").unwrap();
    server.assert_received("deleteid");
    client.send_command("moveid 10 0").unwrap();
    server.assert_received("moveid");
    client.send_command("clear").unwrap();
    server.assert_received("clear");
}

#[test]
fn test_close_command() {
    let (server, mut client) = common::with_mpd_server();

    // Send a known-good command first to verify connection is alive
    client.send_command("status").unwrap();
    server.assert_received("status");

    // Send close — the mock disconnects; send_command returns an error
    // because reading the response fails after the server drops the connection
    let result = client.send_command("close");
    assert!(result.is_err(), "close should fail because server disconnects");

    // Verify the mock received the close command
    server.assert_received("close");

    // Subsequent commands should also fail — connection is dead
    let result = client.send_command("status");
    assert!(result.is_err(), "status after close should fail");
}

#[test]
fn test_send_batch() {
    let (server, mut client) = common::with_mpd_server();

    // Send a batch of independent commands
    let cmds = vec!["clear".to_string(), "play 0".to_string()];
    let result = client.send_batch(&cmds);
    assert!(result.is_ok(), "send_batch should succeed, got: {:?}", result.err());

    server.assert_received("clear");
    server.assert_received("play");

    // Send a larger batch (simulates adding multiple tracks)
    let add_cmds: Vec<String> = (0..5)
        .map(|i| format!("addid \"track-{i}.flac\""))
        .collect();
    let result = client.send_batch(&add_cmds);
    assert!(result.is_ok(), "batch of addids should succeed, got: {:?}", result.err());
    // Each addid should have been received
    server.assert_received("addid");
}

#[test]
fn test_send_batch_empty() {
    let (_server, mut client) = common::with_mpd_server();

    // Empty batch — just command_list_begin/end, should return OK
    let result = client.send_batch(&[]);
    assert!(result.is_ok(), "empty batch should succeed, got: {:?}", result.err());
}
