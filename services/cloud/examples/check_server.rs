//! Checks a running server the way the application uses it: over TLS with the
//! certificates of the operating system, two devices, a real project, a lock, a
//! merge and the audit log. It leaves a test team behind on the server.
//! Usage: `TRONIC_CHECK_PASSWORD=… check_server https://server admin@example.com [--save|--compare file]`
//!
//! `--save` notes what is now stored. `--compare` first asks whether exactly that is
//! still there, which is what a restored backup has to prove: a new project alone
//! would also succeed with an empty object store.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use serde_json::{json, Value};
use std::path::Path;
use tronic_project::areas::{merge, Merge};
use tronic_project::format::{decode, encode, Document};
use tronic_storage::{CloudProvider, LinkState, Outcome, Repo, StorageError};

fn fail<T>(what: &str, detail: impl std::fmt::Debug) -> Result<T, String> {
    Err(format!("{what}: {detail:?}"))
}

fn file(document: &Document) -> Result<Vec<u8>, String> {
    encode(
        &document.project,
        &document.schematic,
        &document.board,
        Path::new("x"),
    )
    .map_err(|error| error.to_string())
}

/// What a later `--compare` looks for.
fn remember(client: &CloudProvider, team: &str, id: &str) -> Result<Value, String> {
    let mut state = LinkState::default();
    let repo = Repo::new(client, id);
    repo.refresh(&mut state).map_err(|e| e.to_string())?;
    let audit = client
        .call("GET", &format!("/v1/teams/{team}/audit"), &Value::Null)
        .map_err(|e| format!("audit: {e}"))?;
    let mut versions = Vec::new();
    for version in Repo::versions(&state) {
        // Reading checks the content against its hash.
        let bytes = repo
            .read(&state, &version.id)
            .map_err(|e| format!("version {}: {e}", version.id))?;
        decode(&bytes, Path::new("x")).map_err(|e| format!("version {}: {e}", version.id))?;
        versions.push(version.id);
    }
    versions.sort();
    if !audit["first_damaged"].is_null() {
        return fail("audit chain", &audit["first_damaged"]);
    }
    Ok(json!({
        "team": team,
        "project": id,
        "versions": versions,
        "audit": audit["entries"].as_array().map_or(0, Vec::len),
    }))
}

fn compare(client: &CloudProvider, file: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let saved: Value = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
    let team = saved["team"].as_str().ok_or("no team noted")?;
    let id = saved["project"].as_str().ok_or("no project noted")?;
    let found = remember(client, team, id)?;
    if saved["versions"].as_array().is_none_or(Vec::is_empty) {
        return fail("nothing noted", &saved);
    }
    if found != saved {
        return Err(format!("stored data differs: noted {saved}, found {found}"));
    }
    println!(
        "still stored: {} versions, {} audit entries",
        saved["versions"].as_array().map_or(0, Vec::len),
        saved["audit"]
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let url = arguments.next().ok_or("server address missing")?;
    let email = arguments.next().ok_or("e-mail missing")?;
    let mode = arguments.next();
    let note = arguments.next();
    if mode.is_some() && note.is_none() {
        return Err("file missing".into());
    }
    let password = std::env::var("TRONIC_CHECK_PASSWORD").map_err(|_| "password missing")?;

    let token = CloudProvider::login(&url, &email, &password).map_err(|e| format!("login: {e}"))?;
    let client = CloudProvider::new(&url, &token).map_err(|e| e.to_string())?;
    println!("logged in over TLS");
    if let (Some("--compare"), Some(note)) = (mode.as_deref(), note.as_deref()) {
        compare(&client, note)?;
    }

    let team = client
        .call("POST", "/v1/teams", &json!({"name": "Installation check"}))
        .map_err(|e| format!("create team: {e}"))?;
    let team = team["id"].as_str().ok_or("no team id")?.to_owned();

    let project = tronic_core::Project::new("Check", "", time::OffsetDateTime::now_utc())
        .map_err(|e| e.to_string())?;
    let id = project.id.to_string();
    client
        .call(
            "POST",
            &format!("/v1/teams/{team}/projects"),
            &json!({"repo": id, "name": "Check"}),
        )
        .map_err(|e| format!("add project: {e}"))?;
    let base = Document {
        project,
        schematic: tronic_schematic::Schematic::default(),
        board: tronic_pcb::Board::default(),
    };
    let base_bytes = file(&base)?;

    let repo = Repo::new(&client, &id);
    let (mut first, mut second) = (LinkState::default(), LinkState::default());
    match repo.sync(&base_bytes, "first", &mut first) {
        Ok(Outcome::Pushed { .. }) => {}
        other => return fail("first version", other),
    }
    match repo.sync(&base_bytes, "second", &mut second) {
        Ok(Outcome::UpToDate { .. }) => {}
        other => return fail("second device", other),
    }
    println!("two devices share a project");

    let mut with_mechanics = base.clone();
    with_mechanics.board.mechanics.thickness = 800_000;
    let mut with_board = base.clone();
    with_board.board.outline = vec![
        tronic_schematic::Point::new(0, 0),
        tronic_schematic::Point::new(50_000_000, 0),
        tronic_schematic::Point::new(50_000_000, 50_000_000),
    ];
    let (mechanics_bytes, board_bytes) = (file(&with_mechanics)?, file(&with_board)?);

    // A lock stops nobody who holds it, and the area can be stored.
    let locks = format!("/v1/projects/{id}/locks");
    client
        .call("POST", &locks, &json!({"area": "mechanics"}))
        .map_err(|e| format!("lock: {e}"))?;
    match repo.sync(&mechanics_bytes, "first", &mut first) {
        Ok(Outcome::Pushed { .. }) => {}
        other => return fail("version under a lock", other),
    }
    client
        .call("DELETE", &format!("{locks}/mechanics"), &Value::Null)
        .map_err(|e| format!("release: {e}"))?;
    println!("lock taken, version stored, lock released");

    // The other device changed another area: both come together.
    match repo.sync(&board_bytes, "second", &mut second) {
        Ok(Outcome::Conflict { .. }) => {}
        other => return fail("expected two states", other),
    }
    let merged = match merge(&base_bytes, &board_bytes, &mechanics_bytes) {
        Ok(Merge::Merged { bytes, .. }) => bytes,
        other => return fail("merge", other),
    };
    repo.resolve_keep_local(&merged, "second", &mut second)
        .map_err(|e: StorageError| format!("merged version: {e}"))?;
    let bytes = match repo.sync(&mechanics_bytes, "first", &mut first) {
        Ok(Outcome::Pulled { bytes, .. }) => bytes,
        other => return fail("fetch the merge", other),
    };
    let result = decode(&bytes, Path::new("x")).map_err(|e| e.to_string())?;
    if result.board.mechanics.thickness != 800_000 || result.board.outline.len() != 3 {
        return fail("merge result", result.board.outline.len());
    }
    println!("changes in two areas came together");

    let audit = client
        .call("GET", &format!("/v1/teams/{team}/audit"), &Value::Null)
        .map_err(|e| format!("audit: {e}"))?;
    let actions: Vec<&str> = audit["entries"]
        .as_array()
        .ok_or("no audit entries")?
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();
    for needed in [
        "team.create",
        "project.add",
        "lock.take",
        "lock.release",
        "version.add",
    ] {
        if !actions.contains(&needed) {
            return fail("missing in the audit log", needed);
        }
    }
    if !audit["first_damaged"].is_null() {
        return fail("audit chain", &audit["first_damaged"]);
    }
    println!("audit log complete and intact");
    if let (Some("--save"), Some(note)) = (mode.as_deref(), note.as_deref()) {
        let noted = remember(&client, &team, &id)?;
        std::fs::write(note, noted.to_string()).map_err(|e| format!("{note}: {e}"))?;
        println!("noted what is stored");
    }
    println!("server is working");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
