//! Measures the service with one and with several instances on this computer. Each
//! worker logs in, writes objects, reads them back and lists them.
//! Usage: `DATABASE_URL=… cargo run --release -p tronic-cloud --example load`
//!
//! On one computer this shows that instances do not get in each other's way and
//! where the limit lies, not what a real cluster reaches.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::panic)]

use std::time::Instant;
use tronic_cloud::{connect, router, Config, Store};
use tronic_storage::{sha256_hex, CloudProvider, StorageProvider};

const WORKERS: usize = 32;
const ROUNDS: usize = 40;
const OBJECT_BYTES: usize = 16 * 1024;

async fn instance(database: &str, store: &std::path::Path) -> String {
    let state = connect(
        database,
        Store::folder(store),
        Config {
            default_quota: 1 << 40,
            allow_registration: true,
        },
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    url
}

fn worker(url: String, number: usize, run: u128) -> (u64, u64) {
    let email = format!("load-{run}-{number}@example.test");
    let token = CloudProvider::register(&url, &email, "a good long test password").unwrap();
    let client = CloudProvider::new(&url, &token).unwrap();
    let project = format!(
        "{number:08x}-1111-4111-8111-{:012x}",
        run as u64 & 0xffff_ffff_ffff
    );
    let (mut requests, mut failures) = (0, 0);
    for round in 0..ROUNDS {
        let mut content = vec![0u8; OBJECT_BYTES];
        content[..16].copy_from_slice(
            &((run as u64) << 32 | (number * 100_000 + round) as u64)
                .to_le_bytes()
                .repeat(2),
        );
        let key = format!("projects/{project}/blobs/{}", sha256_hex(&content));
        let ok = client.put(&key, &content).is_ok()
            && client.get(&key).ok().flatten().as_deref() == Some(&content[..])
            && client
                .list(&format!("projects/{project}/blobs"))
                .map(|keys| keys.len())
                == Ok(round + 1);
        requests += 3;
        if !ok {
            failures += 1;
        }
    }
    (requests, failures)
}

async fn measure(urls: &[String]) -> (f64, u64) {
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let started = Instant::now();
    let mut tasks = Vec::new();
    for number in 0..WORKERS {
        let url = urls[number % urls.len()].clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            worker(url, number, run)
        }));
    }
    let (mut requests, mut failures) = (0, 0);
    for task in tasks {
        let (done, failed) = task.await.unwrap();
        requests += done;
        failures += failed;
    }
    (requests as f64 / started.elapsed().as_secs_f64(), failures)
}

#[tokio::main]
async fn main() {
    let database = std::env::var("DATABASE_URL").unwrap();
    let store = std::env::temp_dir().join(format!("troniccad-load-{}", std::process::id()));
    std::fs::create_dir_all(&store).unwrap();
    let mut urls = Vec::new();
    for count in [1, 2, 4] {
        while urls.len() < count {
            urls.push(instance(&database, &store).await);
        }
        let (rate, failures) = measure(&urls).await;
        println!(
            "{count} instance(s), {WORKERS} workers, {ROUNDS} rounds: {rate:.0} requests/s, {failures} failed rounds"
        );
    }
    let _ = std::fs::remove_dir_all(&store);
}
