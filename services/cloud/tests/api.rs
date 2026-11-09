//! The service against a real PostgreSQL. Skipped without
//! `TRONICCAD_TEST_DATABASE_URL`, as the database is a development tool here.
#![allow(clippy::unwrap_used, clippy::panic)]

use futures_util::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tronic_cloud::{connect, router, Config, Store};
use tronic_storage::*;

fn database() -> Option<String> {
    std::env::var("TRONICCAD_TEST_DATABASE_URL").ok()
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    )
}

/// A fresh instance on its own port. Instances share the database and the store.
async fn instance(store_dir: &std::path::Path, quota: i64) -> String {
    let state = connect(
        &database().unwrap(),
        Store::folder(store_dir),
        Config {
            default_quota: quota,
            allow_registration: true,
        },
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });
    url
}

fn store_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(unique("troniccad-cloud"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

const PASSWORD: &str = "a good long test password";

fn email() -> String {
    format!("{}@example.test", unique("user"))
}

/// Blocking client calls off the async test runtime.
async fn off<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap()
}

async fn new_user(url: &str) -> (String, String) {
    let (url, mail) = (url.to_owned(), email());
    let address = mail.clone();
    let token = off(move || CloudProvider::register(&url, &address, PASSWORD).unwrap()).await;
    (mail, token)
}

fn project() -> String {
    format!(
        "{:08x}-1111-4111-8111-{:012x}",
        COUNTER.fetch_add(1, Ordering::Relaxed),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos() as u64 & 0xffff_ffff_ffff
    )
}

fn blob_key(project: &str, content: &[u8]) -> String {
    format!("projects/{project}/blobs/{}", sha256_hex(content))
}

/// A raw request, for what the regular client would never send.
fn raw(method: &str, url: &str, token: Option<&str>, body: &str) -> u16 {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into();
    let mut builder = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    agent
        .run(builder.body(body.to_owned()).unwrap())
        .unwrap()
        .status()
        .as_u16()
}

async fn direct_database() -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(&database().unwrap(), tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    client
}

macro_rules! needs_database {
    () => {
        if database().is_none() {
            eprintln!("skipped: TRONICCAD_TEST_DATABASE_URL is not set");
            return;
        }
    };
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_sync_through_the_service() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, token) = new_user(&url).await;
    let id = project();
    off(move || {
        let provider = CloudProvider::new(&url, &token).unwrap();
        let repo = Repo::new(&provider, &id);
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
        assert!(provider.account().unwrap()["used_bytes"].as_i64().unwrap() > 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn instances_serve_the_same_data() {
    needs_database!();
    let dir = store_dir();
    let (first, second) = (instance(&dir, 1 << 30).await, instance(&dir, 1 << 30).await);
    let (_, token) = new_user(&first).await;
    let id = project();
    off(move || {
        let one = CloudProvider::new(&first, &token).unwrap();
        let two = CloudProvider::new(&second, &token).unwrap();
        let key = blob_key(&id, b"content");
        one.put(&key, b"content").unwrap();
        assert_eq!(two.get(&key).unwrap().unwrap(), b"content");
        assert_eq!(two.list(&format!("projects/{id}/blobs")).unwrap(), [key]);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn accounts_need_a_valid_email_and_a_long_password() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let register = format!("{url}/v1/auth/register");
    let body =
        |email: &str, password: &str| format!(r#"{{"email":"{email}","password":"{password}"}}"#);
    assert_eq!(raw("POST", &register, None, &body(&email(), "short")), 400);
    assert_eq!(
        raw("POST", &register, None, &body("not-an-email", PASSWORD)),
        400
    );
    let (mail, _) = new_user(&url).await;
    assert_eq!(
        raw("POST", &register, None, &body(&mail, PASSWORD)),
        409,
        "taken"
    );
    assert_eq!(
        raw(
            "POST",
            &register,
            None,
            &body(&mail.to_uppercase(), PASSWORD)
        ),
        409,
        "case does not matter"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sql_in_any_field_is_only_text() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, token) = new_user(&url).await;
    let attack = "x'; DROP TABLE accounts; --@example.test";
    assert_eq!(
        raw(
            "POST",
            &format!("{url}/v1/auth/login"),
            None,
            &format!(r#"{{"email":"{attack}","password":"{PASSWORD}"}}"#)
        ),
        401
    );
    assert_eq!(
        raw("GET", &format!("{url}/v1/account"), Some(&token), ""),
        200,
        "the table is still there"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_works_and_wrong_passwords_are_limited() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (mail, _) = new_user(&url).await;
    let login = format!("{url}/v1/auth/login");
    let body = |password: &str| format!(r#"{{"email":"{mail}","password":"{password}"}}"#);
    assert_eq!(raw("POST", &login, None, &body(PASSWORD)), 200);
    assert_eq!(
        raw(
            "POST",
            &login,
            None,
            &format!(
                r#"{{"email":"nobody-{}@example.test","password":"x"}}"#,
                unique("n")
            )
        ),
        401,
        "the same answer as a wrong password"
    );
    for _ in 0..10 {
        assert_eq!(raw("POST", &login, None, &body("wrong password here")), 401);
    }
    assert_eq!(
        raw("POST", &login, None, &body(PASSWORD)),
        429,
        "even the right one has to wait now"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_can_be_revoked_and_expire() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, token) = new_user(&url).await;
    let account = format!("{url}/v1/account");
    assert_eq!(raw("GET", &account, None, ""), 401, "no token");
    assert_eq!(raw("GET", &account, Some("not a token"), ""), 401);
    assert_eq!(raw("GET", &account, Some(&token), ""), 200);
    assert_eq!(
        raw("POST", &format!("{url}/v1/auth/logout"), Some(&token), ""),
        204
    );
    assert_eq!(
        raw("GET", &account, Some(&token), ""),
        401,
        "gone after logging out"
    );

    let (_, old) = new_user(&url).await;
    let db = direct_database().await;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(old.as_bytes()).to_vec();
    db.execute(
        "UPDATE sessions SET expires = now() - interval '1 second' WHERE token_hash = $1",
        &[&hash],
    )
    .await
    .unwrap();
    assert_eq!(raw("GET", &account, Some(&old), ""), 401, "expired");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_database_holds_no_token_and_no_password() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (mail, token) = new_user(&url).await;
    let db = direct_database().await;
    let stored: String = db
        .query_one(
            "SELECT password_hash FROM accounts WHERE email = $1",
            &[&mail],
        )
        .await
        .unwrap()
        .get(0);
    assert!(stored.starts_with("$argon2id$"), "{stored}");
    assert!(!stored.contains(PASSWORD));
    let tokens: Vec<Vec<u8>> = db
        .query("SELECT token_hash FROM sessions", &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert!(tokens.iter().all(|hash| hash != token.as_bytes()));
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_reaches_someone_elses_project() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, alice) = new_user(&url).await;
    let (_, mallory) = new_user(&url).await;
    let id = project();
    let check_url = url.clone();
    off(move || {
        let url = check_url;
        let owner = CloudProvider::new(&url, &alice).unwrap();
        let key = blob_key(&id, b"secret design");
        owner.put(&key, b"secret design").unwrap();
        let attacker = CloudProvider::new(&url, &mallory).unwrap();
        assert_eq!(attacker.get(&key), Err(StorageError::Forbidden));
        assert_eq!(
            attacker.list(&format!("projects/{id}/blobs")),
            Err(StorageError::Forbidden)
        );
        assert_eq!(
            attacker.put(&blob_key(&id, b"x"), b"x"),
            Err(StorageError::Forbidden)
        );
        assert_eq!(
            raw(
                "POST",
                &format!("{url}/v1/repos/{id}/shares"),
                Some(&mallory),
                r#"{"email":"a@example.test","can_write":true}"#
            ),
            403
        );
        let anonymous = CloudProvider::new(&url, "").unwrap();
        assert_eq!(anonymous.get(&key), Err(StorageError::Unauthorized));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sharing_gives_exactly_the_granted_right() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, alice) = new_user(&url).await;
    let (bob_mail, bob) = new_user(&url).await;
    let id = project();
    off(move || {
        let owner = CloudProvider::new(&url, &alice).unwrap();
        let reader = CloudProvider::new(&url, &bob).unwrap();
        let key = blob_key(&id, b"v1");
        owner.put(&key, b"v1").unwrap();
        let shares = format!("{url}/v1/repos/{id}/shares");
        let grant = |write: bool| format!(r#"{{"email":"{bob_mail}","can_write":{write}}}"#);
        assert_eq!(raw("POST", &shares, Some(&alice), &grant(false)), 204);
        assert_eq!(reader.get(&key).unwrap().unwrap(), b"v1");
        assert_eq!(
            reader.put(&blob_key(&id, b"v2"), b"v2"),
            Err(StorageError::Forbidden)
        );
        assert_eq!(raw("POST", &shares, Some(&alice), &grant(true)), 204);
        reader.put(&blob_key(&id, b"v2"), b"v2").unwrap();
        assert_eq!(
            raw("POST", &shares, Some(&bob), &grant(true)),
            403,
            "only the owner shares"
        );
        assert_eq!(
            raw(
                "DELETE",
                &shares,
                Some(&alice),
                &format!(r#"{{"email":"{bob_mail}"}}"#)
            ),
            204
        );
        assert_eq!(reader.get(&key), Err(StorageError::Forbidden));
        assert_eq!(
            raw(
                "POST",
                &shares,
                Some(&alice),
                r#"{"email":"nobody@example.test","can_write":true}"#
            ),
            404
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_content_cannot_be_replaced_or_faked() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let (_, token) = new_user(&url).await;
    let id = project();
    off(move || {
        let client = CloudProvider::new(&url, &token).unwrap();
        let key = blob_key(&id, b"real");
        client.put(&key, b"real").unwrap();
        // Under a name that does not match its checksum.
        assert!(matches!(
            client.put(&key, b"fake"),
            Err(StorageError::Server { status: 400, .. })
        ));
        assert_eq!(client.get(&key).unwrap().unwrap(), b"real");
        // Sent twice, which an interrupted transfer does: fine, and counted once.
        let used = client.account().unwrap()["used_bytes"].as_i64().unwrap();
        client.put(&key, b"real").unwrap();
        assert_eq!(
            client.account().unwrap()["used_bytes"].as_i64().unwrap(),
            used
        );
        // Outside what the sync stores.
        assert!(matches!(
            client.put(&format!("projects/{id}/other/file"), b"x"),
            Err(StorageError::Server { status: 400, .. })
        ));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn large_bodies_need_a_login_and_json_stays_small() {
    needs_database!();
    let url = instance(&store_dir(), 1 << 30).await;
    let huge = "x".repeat(200_000);
    assert_eq!(
        raw("POST", &format!("{url}/v1/auth/login"), None, &huge),
        413,
        "a login is small"
    );
    let id = project();
    let key = blob_key(&id, huge.as_bytes());
    assert_eq!(
        raw("PUT", &format!("{url}/v1/objects/{key}"), None, &huge),
        401,
        "no login, no upload"
    );
    let (_, token) = new_user(&url).await;
    assert_eq!(
        raw(
            "PUT",
            &format!("{url}/v1/objects/{key}"),
            Some(&token),
            &huge
        ),
        201,
        "objects may be larger than JSON"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn odd_paths_go_nowhere() {
    needs_database!();
    let dir = store_dir();
    let url = instance(&dir, 1 << 30).await;
    let (_, token) = new_user(&url).await;
    for path in [
        "projects/../../etc/passwd",
        "projects/not-a-uuid/blobs/x",
        "etc/passwd",
        "projects/11111111-1111-4111-8111-111111111111/..%2f..%2fx",
        "projects/11111111-1111-4111-8111-111111111111/%2e%2e/%2e%2e/x",
    ] {
        let status = raw("GET", &format!("{url}/v1/objects/{path}"), Some(&token), "");
        assert!(matches!(status, 400 | 404), "{path}: {status}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_quota_holds_even_for_simultaneous_uploads() {
    needs_database!();
    let url = instance(&store_dir(), 1000).await;
    let (_, token) = new_user(&url).await;
    let id = project();
    let mut tasks = Vec::new();
    for i in 0..8u8 {
        let (url, token, id) = (url.clone(), token.clone(), id.clone());
        tasks.push(tokio::task::spawn_blocking(move || {
            let client = CloudProvider::new(&url, &token).unwrap();
            let content = vec![i; 300];
            client.put(&blob_key(&id, &content), &content)
        }));
    }
    let mut stored = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(()) => stored += 1,
            Err(StorageError::QuotaExceeded) => {}
            Err(other) => panic!("{other:?}"),
        }
    }
    assert_eq!(stored, 3, "1000 bytes hold three of 300");
    let client = CloudProvider::new(&url, &token).unwrap();
    let used = off(move || client.account().unwrap()).await["used_bytes"]
        .as_i64()
        .unwrap();
    assert_eq!(used, 900);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_an_account_removes_its_data() {
    needs_database!();
    let dir = store_dir();
    let url = instance(&dir, 1 << 30).await;
    let (_, token) = new_user(&url).await;
    let id = project();
    let client_url = url.clone();
    let client_token = token.clone();
    let key_id = id.clone();
    off(move || {
        CloudProvider::new(&client_url, &client_token)
            .unwrap()
            .put(&blob_key(&key_id, b"to go"), b"to go")
            .unwrap()
    })
    .await;
    let file = dir.join(&id).join("blobs").join(sha256_hex(b"to go"));
    assert!(file.exists());
    let account = format!("{url}/v1/account");
    assert_eq!(
        raw("DELETE", &account, Some(&token), r#"{"password":"wrong"}"#),
        401,
        "a stolen token is not enough"
    );
    assert!(file.exists());
    assert_eq!(
        raw(
            "DELETE",
            &account,
            Some(&token),
            &format!(r#"{{"password":"{PASSWORD}"}}"#)
        ),
        204
    );
    assert!(!file.exists(), "the content is gone");
    assert_eq!(
        raw("GET", &account, Some(&token), ""),
        401,
        "the sessions too"
    );
}

/// A change made through one instance is announced by the other, and only to those
/// with access.
#[tokio::test(flavor = "multi_thread")]
async fn events_reach_readers_across_instances() {
    needs_database!();
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let dir = store_dir();
    let (first, second) = (instance(&dir, 1 << 30).await, instance(&dir, 1 << 30).await);
    let (_, alice) = new_user(&first).await;
    let (_, mallory) = new_user(&first).await;
    let connect_events = |token: String, base: String| async move {
        let mut request = format!("{}/v1/events", base.replacen("http", "ws", 1))
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    };
    assert!(
        connect_events("wrong".to_owned(), second.clone())
            .await
            .is_err(),
        "needs a login"
    );
    let mut mine = connect_events(alice.clone(), second.clone()).await.unwrap();
    let mut theirs = connect_events(mallory, second.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let id = project();
    let writer = id.clone();
    off(move || {
        let provider = CloudProvider::new(&first, &alice).unwrap();
        Repo::new(&provider, &writer)
            .sync(b"content", "a", &mut LinkState::default())
            .unwrap();
    })
    .await;
    let message = tokio::time::timeout(Duration::from_secs(10), mine.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(message.to_text().unwrap().contains(&id), "{message:?}");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), theirs.next())
            .await
            .is_err(),
        "no event for others"
    );
}

/// An open connection must not outlive the login it was opened with.
#[tokio::test(flavor = "multi_thread")]
async fn logging_out_ends_events_on_open_connections() {
    needs_database!();
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let url = instance(&store_dir(), 1 << 30).await;
    let (mail, token) = new_user(&url).await;
    let mut request = format!("{}/v1/events", url.replacen("http", "ws", 1))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let write = |content: &'static [u8], token: String, url: String| {
        let id = project();
        off(move || {
            let provider = CloudProvider::new(&url, &token).unwrap();
            Repo::new(&provider, &id)
                .sync(content, "a", &mut LinkState::default())
                .unwrap();
        })
    };
    write(b"first", token.clone(), url.clone()).await;
    let first = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    assert!(
        matches!(first, Ok(Some(Ok(_)))),
        "events arrive while logged in"
    );

    // Logged out on this session; a second session keeps writing.
    let second = {
        let (url, mail) = (url.clone(), mail.clone());
        off(move || CloudProvider::login(&url, &mail, PASSWORD).unwrap()).await
    };
    let (logout_url, logout_token) = (url.clone(), token.clone());
    let status = off(move || {
        raw(
            "POST",
            &format!("{logout_url}/v1/auth/logout"),
            Some(&logout_token),
            "",
        )
    })
    .await;
    assert_eq!(status, 204);
    write(b"second", second, url.clone()).await;
    let after = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    assert!(
        !matches!(
            after,
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(_))))
        ),
        "no event after logging out: {after:?}"
    );
    assert!(after.is_ok(), "the connection ends instead of waiting");
}
