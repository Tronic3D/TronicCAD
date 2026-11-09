//! The adapters against real, independent servers: rclone's S3 and WebDAV servers.
//! Skipped without `TRONICCAD_RCLONE`, which names the rclone program. rclone is a
//! development tool here and not shipped.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tronic_storage::*;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn rclone() -> Option<String> {
    std::env::var("TRONICCAD_RCLONE").ok()
}

fn start(arguments: &[&str], port: u16) -> Server {
    let child = Command::new(rclone().unwrap())
        .args(arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server(child);
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "server did not start"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    server
}

fn temp(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("troniccad-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

const PROJECT: &str = "11111111-2222-3333-4444-555555555555";

/// What every provider must do, the same way.
fn exercise(provider: &dyn StorageProvider) {
    assert_eq!(provider.get("nothing/here").unwrap(), None);
    assert!(provider.list("nothing").unwrap().is_empty());
    provider.put("a/b/one.txt", b"first").unwrap();
    provider.put("a/b/two.txt", b"second").unwrap();
    provider
        .put("a/c/deep/three.txt", &[0u8, 255, 10, 13])
        .unwrap();
    assert_eq!(provider.get("a/b/one.txt").unwrap().unwrap(), b"first");
    assert_eq!(
        provider.get("a/c/deep/three.txt").unwrap().unwrap(),
        [0u8, 255, 10, 13],
        "binary stays binary"
    );
    provider.put("a/b/one.txt", b"replaced").unwrap();
    assert_eq!(provider.get("a/b/one.txt").unwrap().unwrap(), b"replaced");
    let mut keys = provider.list("a").unwrap();
    keys.sort();
    assert_eq!(keys, ["a/b/one.txt", "a/b/two.txt", "a/c/deep/three.txt"]);
    assert_eq!(provider.list("a/b").unwrap().len(), 2);
    // A file of several megabytes.
    let large: Vec<u8> = (0..8_000_000u32).map(|i| (i % 251) as u8).collect();
    provider.put("large/file", &large).unwrap();
    assert_eq!(
        sha256_hex(&provider.get("large/file").unwrap().unwrap()),
        sha256_hex(&large)
    );

    // The real flow, with two devices and a conflict.
    let repo = Repo::new(provider, PROJECT);
    let mut a = LinkState::default();
    let mut b = LinkState::default();
    assert!(matches!(
        repo.sync(b"one", "a", &mut a).unwrap(),
        Outcome::Pushed { .. }
    ));
    assert!(matches!(
        repo.sync(b"one", "b", &mut b).unwrap(),
        Outcome::UpToDate { .. }
    ));
    assert!(matches!(
        repo.sync(b"two", "a", &mut a).unwrap(),
        Outcome::Pushed { .. }
    ));
    match repo.sync(b"one", "b", &mut b).unwrap() {
        Outcome::Pulled { bytes, .. } => assert_eq!(bytes, b"two"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        repo.sync(b"three", "a", &mut a).unwrap(),
        Outcome::Pushed { .. }
    ));
    assert!(matches!(
        repo.sync(b"four", "b", &mut b).unwrap(),
        Outcome::Conflict { .. }
    ));
    repo.resolve_keep_local(b"four", "b", &mut b).unwrap();
    match repo.sync(b"three", "a", &mut a).unwrap() {
        Outcome::Pulled { bytes, .. } => assert_eq!(bytes, b"four"),
        other => panic!("{other:?}"),
    }
    assert_eq!(Repo::versions(&a).len(), 4);
}

#[test]
fn folder_provider() {
    let root = temp("folder");
    exercise(&FolderProvider::new(&root));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_drive_counts_as_offline() {
    let provider = FolderProvider::new(Path::new("Z:/does/not/exist/troniccad"));
    assert!(matches!(
        provider.put("a", b"x"),
        Err(StorageError::Unreachable(_))
    ));
    assert!(matches!(
        provider.get("a"),
        Err(StorageError::Unreachable(_))
    ));
    assert!(matches!(
        provider.list("a"),
        Err(StorageError::Unreachable(_))
    ));
}

#[test]
fn webdav_against_a_real_server() {
    if rclone().is_none() {
        eprintln!("skipped: TRONICCAD_RCLONE is not set");
        return;
    }
    let root = temp("webdav");
    let port = 18911;
    let _server = start(
        &[
            "serve",
            "webdav",
            root.to_str().unwrap(),
            "--addr",
            &format!("127.0.0.1:{port}"),
            "--user",
            "tester",
            "--pass",
            "test-pass",
        ],
        port,
    );
    let url = format!("http://127.0.0.1:{port}");
    exercise(&WebDavProvider::new(&url, "tester", "test-pass").unwrap());
    let wrong = WebDavProvider::new(&url, "tester", "wrong").unwrap();
    assert_eq!(wrong.get("a/b/one.txt"), Err(StorageError::Unauthorized));
    assert_eq!(wrong.put("a/x", b"y"), Err(StorageError::Unauthorized));
    assert_eq!(wrong.list("a"), Err(StorageError::Unauthorized));
}

#[test]
fn s3_against_a_real_server() {
    if rclone().is_none() {
        eprintln!("skipped: TRONICCAD_RCLONE is not set");
        return;
    }
    let root = temp("s3");
    std::fs::create_dir_all(root.join("bucket")).unwrap();
    let port = 18912;
    let _server = start(
        &[
            "serve",
            "s3",
            root.to_str().unwrap(),
            "--addr",
            &format!("127.0.0.1:{port}"),
            "--auth-key",
            "testaccess,testsecret",
            "--force-path-style=true",
        ],
        port,
    );
    let url = format!("http://127.0.0.1:{port}");
    exercise(&S3Provider::new(&url, "bucket", "us-east-1", "testaccess", "testsecret").unwrap());
    let wrong = S3Provider::new(&url, "bucket", "us-east-1", "testaccess", "wrong").unwrap();
    assert!(
        wrong.get("a/b/one.txt").is_err(),
        "a wrong signature must not read"
    );
    assert!(
        wrong.put("a/x", b"y").is_err(),
        "a wrong signature must not write"
    );
}

#[test]
fn a_server_that_goes_away_counts_as_offline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let provider = WebDavProvider::new(&url, "a", "b").unwrap();
    assert!(matches!(
        provider.get("a"),
        Err(StorageError::Unreachable(_))
    ));
    let provider = S3Provider::new(&url, "b", "", "a", "s").unwrap();
    assert!(matches!(
        provider.list("a"),
        Err(StorageError::Unreachable(_))
    ));
}
