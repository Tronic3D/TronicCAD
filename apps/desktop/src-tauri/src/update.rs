//! Looking for, fetching and installing updates. See `docs/specs/updates.md`.
//!
//! All decisions about a manifest are in `tronic_update`. This part does the fetching
//! and the exchange of the program file. The old program stays running until the new
//! one reports that its window came up, and takes itself back if that does not happen.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tronic_project::store;
use tronic_storage::{checked_base, http_agent};
use tronic_update::{
    check_artifact, decide, this_platform, Channel, Context, Decision, Offer, SignedManifest,
    UpdateError, UpdateKind, Version, MAX_MANIFEST_BYTES,
};

use crate::error::CommandError;
use crate::{locked, AppState};

const CHANNEL_KEY: &str = "update.channel";
const AUTO_KEY: &str = "update.auto";
const FAILED_KEY: &str = "update.failed";
const PREVIOUS_KEY: &str = "update.previous_version";
const CHECKED_KEY: &str = "update.last_check";
const BUILD_URL: Option<&str> = option_env!("TRONICCAD_UPDATE_URL");
/// Comma separated: the current key and the next one.
const BUILD_KEYS: Option<&str> = option_env!("TRONICCAD_UPDATE_PUBLIC_KEYS");
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(60);
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
const KEEP_BACKUPS: usize = 3;
const MAX_BACKUP_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PROGRESS_STEP: u64 = 4 * 1024 * 1024;
const COPY_BUFFER: usize = 64 * 1024;

#[derive(Default)]
pub struct UpdateState {
    available: Mutex<Option<Offer>>,
    staged: Mutex<Option<PathBuf>>,
    last_error: Mutex<Option<CommandError>>,
}

/// In development only: tests name their own version, address and key. None of this
/// is compiled into a release.
#[cfg(debug_assertions)]
fn development(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[cfg(not(debug_assertions))]
fn development(_name: &str) -> Option<String> {
    None
}

fn current_version() -> Result<Version, CommandError> {
    let text =
        development(test_names::VERSION).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    Version::parse(&text).map_err(error_of)
}

#[cfg(debug_assertions)]
mod test_names {
    pub const VERSION: &str = "TRONICCAD_TEST_APP_VERSION";
    pub const URL: &str = "TRONICCAD_TEST_UPDATE_URL";
    pub const KEYS: &str = "TRONICCAD_TEST_UPDATE_KEYS";
}

#[cfg(not(debug_assertions))]
mod test_names {
    pub const VERSION: &str = "";
    pub const URL: &str = "";
    pub const KEYS: &str = "";
}

fn update_url() -> Option<String> {
    development(test_names::URL).or_else(|| BUILD_URL.map(str::to_owned))
}

fn trusted_keys() -> Vec<String> {
    development(test_names::KEYS)
        .or_else(|| BUILD_KEYS.map(str::to_owned))
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .collect()
}

fn error_of(error: UpdateError) -> CommandError {
    match error {
        UpdateError::Malformed | UpdateError::UnknownFormat(_) | UpdateError::BadKey => {
            CommandError::new("update.unreadable")
        }
        UpdateError::BadSignature => CommandError::new("update.badSignature"),
        UpdateError::BadVersion(version) => {
            CommandError::new("update.unreadable").with("detail", version)
        }
        UpdateError::WrongChannel { .. } => CommandError::new("update.wrongChannel"),
        UpdateError::Replayed { .. } => CommandError::new("update.replayed"),
        UpdateError::Expired => CommandError::new("update.expired"),
        UpdateError::WrongPlatform(platform) => {
            CommandError::new("update.wrongPlatform").with("platform", platform)
        }
        UpdateError::BadAddress => CommandError::new("update.badAddress"),
        UpdateError::WrongSize { .. } | UpdateError::WrongContent => {
            CommandError::new("update.wrongContent")
        }
    }
}

fn io_failure(code: &'static str, error: impl std::fmt::Display) -> CommandError {
    CommandError::new(code).with("detail", error)
}

fn updates_dir(state: &AppState) -> Result<PathBuf, CommandError> {
    let directory = state
        .data_directory
        .as_ref()
        .ok_or_else(|| CommandError::new("update.noDataDirectory"))?
        .join("updates");
    std::fs::create_dir_all(&directory).map_err(|error| io_failure("update.io", error))?;
    Ok(directory)
}

fn channel(state: &AppState) -> Result<Channel, CommandError> {
    Ok(locked(&state.data)?
        .setting(CHANNEL_KEY)?
        .and_then(|text| serde_json::from_value(serde_json::Value::String(text)).ok())
        .unwrap_or(Channel::Stable))
}

/// On unless switched off. Looking only asks for a public file and sends nothing about
/// the user, and hearing of a security update late is the larger risk.
fn automatic(state: &AppState) -> Result<bool, CommandError> {
    Ok(locked(&state.data)?.setting(AUTO_KEY)?.as_deref() != Some("false"))
}

fn sequence_key(channel: Channel) -> String {
    format!("update.sequence.{}", channel.name())
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[derive(Serialize)]
pub struct AvailableView {
    version: String,
    kind: UpdateKind,
    notes: BTreeMap<String, String>,
    size: u64,
    backup_first: bool,
    downloaded: bool,
    failed_before: bool,
}

#[derive(Serialize)]
pub struct BackupView {
    path: String,
    from: String,
    to: String,
    created: i64,
    projects: usize,
    skipped: usize,
}

#[derive(Serialize)]
pub struct UpdateStatus {
    version: String,
    channel: Channel,
    automatic: bool,
    configured: bool,
    can_install: bool,
    last_check: Option<i64>,
    available: Option<AvailableView>,
    last_error: Option<CommandError>,
    previous_version: Option<String>,
    backups: Vec<BackupView>,
}

fn backups_dir(state: &AppState) -> Result<PathBuf, CommandError> {
    let directory = updates_dir(state)?.join("backups");
    std::fs::create_dir_all(&directory).map_err(|error| io_failure("update.io", error))?;
    Ok(directory)
}

fn list_backups(state: &AppState) -> Result<Vec<BackupView>, CommandError> {
    let mut found = Vec::new();
    let entries =
        std::fs::read_dir(backups_dir(state)?).map_err(|error| io_failure("update.io", error))?;
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path().join("backup.json")).unwrap_or_default();
        let Ok(index) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        found.push(BackupView {
            path: entry.path().display().to_string(),
            from: index["from"].as_str().unwrap_or_default().to_owned(),
            to: index["to"].as_str().unwrap_or_default().to_owned(),
            created: index["created"].as_i64().unwrap_or(0),
            projects: index["projects"].as_array().map_or(0, Vec::len),
            skipped: index["skipped"].as_array().map_or(0, Vec::len),
        });
    }
    found.sort_by_key(|backup| std::cmp::Reverse(backup.created));
    Ok(found)
}

#[tauri::command(async)]
pub fn update_status(state: State<'_, AppState>) -> Result<UpdateStatus, CommandError> {
    let (last_check, previous_version, failed) = {
        let data = locked(&state.data)?;
        (
            data.setting(CHECKED_KEY)?
                .and_then(|text| text.parse().ok()),
            data.setting(PREVIOUS_KEY)?,
            data.setting(FAILED_KEY)?,
        )
    };
    let staged = locked(&state.update.staged)?.is_some();
    let available = locked(&state.update.available)?
        .as_ref()
        .map(|offer| AvailableView {
            version: offer.version.to_string(),
            kind: offer.kind,
            notes: offer.notes.clone(),
            size: offer.artifact.size,
            backup_first: offer.backup_first,
            downloaded: staged,
            failed_before: failed.as_deref() == Some(offer.version.to_string().as_str()),
        });
    let previous_exists = previous_path().is_ok_and(|path| path.exists());
    Ok(UpdateStatus {
        version: current_version()?.to_string(),
        channel: channel(&state)?,
        automatic: automatic(&state)?,
        configured: update_url().is_some() && !trusted_keys().is_empty(),
        can_install: cfg!(windows),
        last_check,
        available,
        last_error: locked(&state.update.last_error)?.clone(),
        previous_version: previous_version.filter(|_| previous_exists),
        backups: list_backups(&state).unwrap_or_default(),
    })
}

#[tauri::command(async)]
pub fn update_set_channel(
    state: State<'_, AppState>,
    channel: Channel,
) -> Result<(), CommandError> {
    locked(&state.data)?.set_setting(CHANNEL_KEY, channel.name())?;
    // What another channel offered no longer holds. The version never goes back: a
    // channel that is behind simply has nothing to offer until it catches up.
    *locked(&state.update.available)? = None;
    discard_staged(&state)
}

#[tauri::command(async)]
pub fn update_set_automatic(
    state: State<'_, AppState>,
    automatic: bool,
) -> Result<(), CommandError> {
    locked(&state.data)?.set_setting(AUTO_KEY, if automatic { "true" } else { "false" })?;
    Ok(())
}

fn discard_staged(state: &AppState) -> Result<(), CommandError> {
    if let Some(path) = locked(&state.update.staged)?.take() {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

fn fetch_manifest(base: &str, channel: Channel) -> Result<SignedManifest, CommandError> {
    let (base, local) = checked_base(base).map_err(CommandError::from)?;
    let url = format!("{base}/{}.json", channel.name());
    let mut response = http_agent(local)
        .get(&url)
        .call()
        .map_err(|error| io_failure("update.unreachable", error))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(CommandError::new("update.unreachable").with("detail", status));
    }
    let mut text = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_MANIFEST_BYTES)
        .read_to_end(&mut text)
        .map_err(|error| io_failure("update.unreachable", error))?;
    serde_json::from_slice(&text).map_err(|_| CommandError::new("update.unreadable"))
}

fn check_now(state: &AppState) -> Result<(), CommandError> {
    let base = update_url().ok_or_else(|| CommandError::new("update.notConfigured"))?;
    let keys = trusted_keys();
    if keys.is_empty() {
        return Err(CommandError::new("update.notConfigured"));
    }
    let channel = channel(state)?;
    let signed = fetch_manifest(&base, channel)?;
    let seen = locked(&state.data)?
        .setting(&sequence_key(channel))?
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let decision = decide(
        &signed,
        &Context {
            trusted_keys: &keys,
            channel,
            platform: &this_platform(),
            current: current_version()?,
            seen_sequence: seen,
            now: now(),
        },
    )
    .map_err(error_of)?;
    let (sequence, offer) = match decision {
        Decision::UpToDate { sequence } => (sequence, None),
        Decision::Available(offer) => (offer.sequence, Some(*offer)),
    };
    {
        let data = locked(&state.data)?;
        // Only a manifest that passed everything moves this on.
        data.set_setting(&sequence_key(channel), &sequence.to_string())?;
        data.set_setting(CHECKED_KEY, &now().to_string())?;
    }
    let same = locked(&state.update.available)?
        .as_ref()
        .map(|old| old.artifact.sha256.clone())
        == offer.as_ref().map(|new| new.artifact.sha256.clone());
    if !same {
        discard_staged(state)?;
    }
    *locked(&state.update.available)? = offer;
    Ok(())
}

/// A failed check keeps nothing that was offered before: the offer can no longer be
/// trusted to be the current statement.
#[tauri::command(async)]
pub fn update_check(state: State<'_, AppState>) -> Result<(), CommandError> {
    let result = check_now(&state);
    *locked(&state.update.last_error)? = result.as_ref().err().cloned();
    if result.is_err() {
        *locked(&state.update.available)? = None;
        discard_staged(&state)?;
    }
    result
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

fn download(app: &AppHandle, state: &AppState, offer: &Offer) -> Result<PathBuf, CommandError> {
    let (_, local) = checked_base(&offer.artifact.url).map_err(CommandError::from)?;
    let mut response = http_agent(local)
        .get(&offer.artifact.url)
        .call()
        .map_err(|error| io_failure("update.unreachable", error))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(CommandError::new("update.unreachable").with("detail", status));
    }
    let directory = updates_dir(state)?;
    let partial = directory.join("download.part");
    let mut file =
        std::fs::File::create(&partial).map_err(|error| io_failure("update.io", error))?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut reported = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER];
    // One byte more than signed is enough to know that it is wrong.
    let mut reader = response
        .body_mut()
        .as_reader()
        .take(offer.artifact.size + 1);
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| io_failure("update.unreachable", error))?;
        if count == 0 {
            break;
        }
        let part = buffer.get(..count).unwrap_or_default();
        hash.update(part);
        file.write_all(part)
            .map_err(|error| io_failure("update.io", error))?;
        size += count as u64;
        if size - reported >= PROGRESS_STEP {
            reported = size;
            let _ = app.emit(
                "update-progress",
                Progress {
                    done: size,
                    total: offer.artifact.size,
                },
            );
        }
    }
    file.sync_all()
        .map_err(|error| io_failure("update.io", error))?;
    drop(file);
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if let Err(error) = check_artifact(&offer.artifact, size, &digest) {
        // Nothing that failed the check may stay around.
        let _ = std::fs::remove_file(&partial);
        return Err(error_of(error));
    }
    let staged = directory.join(format!("update-{}.bin", offer.version));
    std::fs::rename(&partial, &staged).map_err(|error| io_failure("update.io", error))?;
    Ok(staged)
}

#[tauri::command(async)]
pub fn update_download(app: AppHandle, state: State<'_, AppState>) -> Result<(), CommandError> {
    let offer = locked(&state.update.available)?
        .clone()
        .ok_or_else(|| CommandError::new("update.nothingAvailable"))?;
    discard_staged(&state)?;
    let staged = download(&app, &state, &offer)?;
    *locked(&state.update.staged)? = Some(staged);
    Ok(())
}

fn copy_with_limit(from: &Path, to: &Path, used: &mut u64) -> Result<bool, CommandError> {
    let size = std::fs::metadata(from)
        .map_err(|error| io_failure("update.backupFailed", error))?
        .len();
    if *used + size > MAX_BACKUP_BYTES {
        return Ok(false);
    }
    std::fs::copy(from, to).map_err(|error| io_failure("update.backupFailed", error))?;
    *used += size;
    Ok(true)
}

/// Settings and all known project files, before a larger step. If this fails, nothing
/// gets installed.
fn make_backup(state: &AppState, from: &Version, to: &Version) -> Result<PathBuf, CommandError> {
    let directory = backups_dir(state)?.join(format!("{from}-to-{to}-{}", now()));
    let projects_dir = directory.join("projects");
    std::fs::create_dir_all(&projects_dir)
        .map_err(|error| io_failure("update.backupFailed", error))?;
    let settings = directory.join("settings.db");
    let mut paths: Vec<PathBuf> = {
        let data = locked(&state.data)?;
        data.snapshot(&settings)
            .map_err(|error| io_failure("update.backupFailed", error))?;
        data.recent()?
            .into_iter()
            .map(|recent| PathBuf::from(recent.path))
            .collect()
    };
    if let Some(session) = locked(&state.session)?.as_ref() {
        if !paths.iter().any(|path| path == session.path()) {
            paths.push(session.path().to_path_buf());
        }
    }
    let mut used = 0;
    let (mut copied, mut skipped) = (Vec::new(), Vec::new());
    for (index, path) in paths.iter().enumerate() {
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().map_or_else(
            || "project".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        let target = projects_dir.join(format!("{index}-{name}"));
        if copy_with_limit(path, &target, &mut used)? {
            copied.push(serde_json::json!({"from": path.display().to_string(), "copy": target.display().to_string()}));
        } else {
            skipped.push(path.display().to_string());
        }
    }
    let index = serde_json::json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "created": now(),
        "projects": copied,
        "skipped": skipped,
    });
    store::replace_atomic(&directory.join("backup.json"), index.to_string().as_bytes())
        .map_err(|error| io_failure("update.backupFailed", error))?;
    prune_backups(state)?;
    Ok(directory)
}

fn prune_backups(state: &AppState) -> Result<(), CommandError> {
    for old in list_backups(state)?.into_iter().skip(KEEP_BACKUPS) {
        let _ = std::fs::remove_dir_all(old.path);
    }
    Ok(())
}

fn program_path() -> Result<PathBuf, CommandError> {
    std::env::current_exe().map_err(|error| io_failure("update.io", error))
}

fn previous_path() -> Result<PathBuf, CommandError> {
    Ok(program_path()?.with_extension("previous"))
}

fn pending_path(state: &AppState) -> Result<PathBuf, CommandError> {
    Ok(updates_dir(state)?.join("pending.json"))
}

fn confirmed_path(state: &AppState) -> Result<PathBuf, CommandError> {
    Ok(updates_dir(state)?.join("confirmed"))
}

fn refuse_with_unsaved_work(state: &AppState) -> Result<(), CommandError> {
    if locked(&state.session)?
        .as_ref()
        .is_some_and(|session| session.is_dirty())
    {
        return Err(CommandError::new("update.saveFirst"));
    }
    Ok(())
}

/// Puts `new_file` in place of the running program. The running program can be renamed
/// on Windows but not overwritten. Undone completely if a step fails.
fn exchange(new_file: &Path) -> Result<(), CommandError> {
    let program = program_path()?;
    let previous = previous_path()?;
    let incoming = program.with_extension("incoming");
    let write_error = |error: std::io::Error| io_failure("update.notWritable", error);
    std::fs::copy(new_file, &incoming).map_err(write_error)?;
    let _ = std::fs::remove_file(&previous);
    if let Err(error) = std::fs::rename(&program, &previous) {
        let _ = std::fs::remove_file(&incoming);
        return Err(write_error(error));
    }
    if let Err(error) = std::fs::rename(&incoming, &program) {
        let _ = std::fs::rename(&previous, &program);
        let _ = std::fs::remove_file(&incoming);
        return Err(write_error(error));
    }
    Ok(())
}

/// Puts the previous program back after a failed start.
fn take_back(failed: &Path) -> Result<(), CommandError> {
    let program = program_path()?;
    let previous = previous_path()?;
    let _ = std::fs::remove_file(failed);
    // The failed program may still be shutting down.
    let mut last = None;
    for _ in 0..50 {
        match std::fs::rename(&program, failed) {
            Ok(()) => {
                last = None;
                break;
            }
            Err(error) => {
                last = Some(error);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    if let Some(error) = last {
        return Err(io_failure("update.takeBackFailed", error));
    }
    std::fs::rename(&previous, &program)
        .map_err(|error| io_failure("update.takeBackFailed", error))?;
    let _ = std::fs::remove_file(failed);
    Ok(())
}

enum Outcome {
    Confirmed,
    Failed,
}

/// Starts the new program and waits for it to report that it works.
fn start_and_wait(state: &AppState, to: &Version) -> Result<Outcome, CommandError> {
    let confirmed = confirmed_path(state)?;
    let _ = std::fs::remove_file(&confirmed);
    let mut command = std::process::Command::new(program_path()?);
    // A test's stand-in for what a really newer build would report by itself.
    #[cfg(debug_assertions)]
    command.env(test_names::VERSION, to.to_string());
    #[cfg(not(debug_assertions))]
    let _ = to;
    let Ok(mut child) = command.spawn() else {
        return Ok(Outcome::Failed);
    };
    let started = Instant::now();
    while started.elapsed() < CONFIRM_TIMEOUT {
        if confirmed.exists() {
            return Ok(Outcome::Confirmed);
        }
        if matches!(child.try_wait(), Ok(Some(_))) {
            return Ok(Outcome::Failed);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(Outcome::Failed)
}

fn run_exchange(app: &AppHandle, state: &AppState, to: &Version) -> Result<(), CommandError> {
    let window = app.get_webview_window("main");
    if let Some(window) = &window {
        let _ = window.hide();
    }
    let outcome = start_and_wait(state, to)?;
    if matches!(outcome, Outcome::Confirmed) {
        let _ = std::fs::remove_file(pending_path(state)?);
        app.exit(0);
        return Ok(());
    }
    let failed = program_path()?.with_extension("failed");
    let result = take_back(&failed);
    let _ = std::fs::remove_file(pending_path(state)?);
    if let Some(window) = &window {
        let _ = window.show();
    }
    result?;
    Err(CommandError::new("update.startFailed").with("version", to))
}

/// Installs what was fetched and checked. Asks for unsaved work to be saved first and
/// makes a backup before a larger step. The fetched file is checked once more.
#[tauri::command(async)]
pub fn update_install(app: AppHandle, state: State<'_, AppState>) -> Result<(), CommandError> {
    if !cfg!(windows) {
        return Err(CommandError::new("update.installUnsupported"));
    }
    refuse_with_unsaved_work(&state)?;
    let offer = locked(&state.update.available)?
        .clone()
        .ok_or_else(|| CommandError::new("update.nothingAvailable"))?;
    let staged = locked(&state.update.staged)?
        .clone()
        .ok_or_else(|| CommandError::new("update.notDownloaded"))?;
    let bytes = std::fs::read(&staged).map_err(|error| io_failure("update.io", error))?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    check_artifact(&offer.artifact, bytes.len() as u64, &digest).map_err(error_of)?;
    let from = current_version()?;
    if offer.backup_first {
        make_backup(&state, &from, &offer.version)?;
    }
    let pending = serde_json::json!({"from": from.to_string(), "to": offer.version.to_string()});
    store::replace_atomic(&pending_path(&state)?, pending.to_string().as_bytes())
        .map_err(|error| io_failure("update.io", error))?;
    if let Err(error) = exchange(&staged) {
        let _ = std::fs::remove_file(pending_path(&state)?);
        return Err(error);
    }
    let result = run_exchange(&app, &state, &offer.version);
    if result.is_err() {
        // So that the same broken build is not recommended again without a word.
        locked(&state.data)?.set_setting(FAILED_KEY, &offer.version.to_string())?;
        discard_staged(&state)?;
    }
    result
}

/// The installer told Windows a version when it ran. An update changes the program
/// without it, so that entry is kept true here. Without an entry nothing is added:
/// then the program was not installed by the installer.
#[cfg(windows)]
fn list_version_in_windows(version: &Version) {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\TronicCAD";
    let run = |arguments: &[&str]| {
        std::process::Command::new("reg")
            .args(arguments)
            .creation_flags(NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    };
    if run(&["query", KEY, "/v", "DisplayVersion"]) {
        // Only a list entry: a failure here takes nothing from the update.
        let _ = run(&[
            "add",
            KEY,
            "/v",
            "DisplayVersion",
            "/t",
            "REG_SZ",
            "/d",
            &version.to_string(),
            "/f",
        ]);
    }
}

#[cfg(not(windows))]
fn list_version_in_windows(_version: &Version) {}

/// The window of this program came up: called by the interface once it is running.
/// This is what the previous program waits for.
#[tauri::command(async)]
pub fn update_confirm(state: State<'_, AppState>) -> Result<(), CommandError> {
    let path = pending_path(&state)?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let pending: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if pending["to"].as_str() != Some(current_version()?.to_string().as_str()) {
        return Ok(());
    }
    if let Some(from) = pending["from"].as_str() {
        locked(&state.data)?.set_setting(PREVIOUS_KEY, from)?;
    }
    list_version_in_windows(&current_version()?);
    store::replace_atomic(&confirmed_path(&state)?, b"ok")
        .map_err(|error| io_failure("update.io", error))
}

/// Back to the program kept from before the last update, on request. This is the one
/// way to go back, and it uses only what is on this computer.
#[tauri::command(async)]
pub fn update_rollback(app: AppHandle, state: State<'_, AppState>) -> Result<(), CommandError> {
    if !cfg!(windows) {
        return Err(CommandError::new("update.installUnsupported"));
    }
    refuse_with_unsaved_work(&state)?;
    let from = current_version()?;
    let previous_version = locked(&state.data)?
        .setting(PREVIOUS_KEY)?
        .ok_or_else(|| CommandError::new("update.noPrevious"))?;
    let previous_version = Version::parse(&previous_version).map_err(error_of)?;
    let previous = previous_path()?;
    if !previous.exists() {
        return Err(CommandError::new("update.noPrevious"));
    }
    let holding = program_path()?.with_extension("holding");
    std::fs::copy(&previous, &holding).map_err(|error| io_failure("update.notWritable", error))?;
    let pending = serde_json::json!({"from": from.to_string(), "to": previous_version.to_string()});
    store::replace_atomic(&pending_path(&state)?, pending.to_string().as_bytes())
        .map_err(|error| io_failure("update.io", error))?;
    let exchanged = exchange(&holding);
    let _ = std::fs::remove_file(&holding);
    if let Err(error) = exchanged {
        let _ = std::fs::remove_file(pending_path(&state)?);
        return Err(error);
    }
    run_exchange(&app, &state, &previous_version)
}

#[tauri::command(async)]
pub fn update_open_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), CommandError> {
    // Only what is listed as a backup, whatever the interface asks for.
    if !list_backups(&state)?
        .iter()
        .any(|backup| backup.path == path)
    {
        return Err(CommandError::new("update.io").with("detail", path));
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|error| io_failure("update.io", error))
}

/// Looks once a day when that is switched on and a server is built in.
pub fn start_checking(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_AFTER);
        loop {
            let state = app.state::<AppState>();
            if update_url().is_some() && automatic(&state).unwrap_or(false) {
                let result = check_now(&state);
                if let Ok(mut slot) = locked(&state.update.last_error) {
                    *slot = result.err();
                }
                let _ = app.emit("update-changed", ());
            }
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_of_the_checks_has_a_message() {
        for (error, code) in [
            (UpdateError::BadSignature, "update.badSignature"),
            (UpdateError::Malformed, "update.unreadable"),
            (UpdateError::Expired, "update.expired"),
            (UpdateError::WrongContent, "update.wrongContent"),
            (UpdateError::BadAddress, "update.badAddress"),
            (
                UpdateError::Replayed { seen: 2, found: 1 },
                "update.replayed",
            ),
        ] {
            assert_eq!(error_of(error).code, code);
        }
    }

    #[test]
    fn a_channel_is_stored_by_its_own_name() {
        assert_eq!(sequence_key(Channel::Beta), "update.sequence.beta");
    }
}
