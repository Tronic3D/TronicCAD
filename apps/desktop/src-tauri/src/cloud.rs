//! Tauri commands of cloud storage, synchronization and backup. All of it works on
//! the saved project file: local work never depends on any of this.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;
use time::macros::format_description;
use time::OffsetDateTime;
use tronic_project::areas::{merge, Area, Merge};
use tronic_project::compare::{compare, Comparison};
use tronic_project::{format, store, ProjectSession};
use tronic_storage::sync::Version;
use tronic_storage::{
    sha256_hex, CloudProvider, FolderProvider, LinkState, Outcome, Repo, S3Provider, StorageError,
    StorageProvider, WebDavProvider,
};

use crate::error::CommandError;
use crate::{locked, view, AppState, ProjectView};

const TARGETS_KEY: &str = "storage_targets";
const DEVICE_KEY: &str = "device_name";
const KEYRING_VARIABLE: &str = "TRONICCAD_KEYRING_SERVICE";
const KEYRING_SERVICE: &str = "TronicCAD";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetKind {
    Folder {
        path: String,
    },
    WebDav {
        url: String,
        user: String,
    },
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        access_key: String,
    },
    Cloud {
        url: String,
        email: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: TargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Sync,
    Backup,
}

/// One project file connected to one target.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Link {
    target: String,
    mode: Mode,
    auto: bool,
    state: LinkState,
}

fn secret_entry(target: &str) -> Result<keyring::Entry, CommandError> {
    let service = std::env::var(KEYRING_VARIABLE).unwrap_or_else(|_| KEYRING_SERVICE.to_owned());
    keyring::Entry::new(&service, &format!("storage:{target}"))
        .map_err(|error| CommandError::new("ai.keyStore").with("detail", error))
}

fn read_secret(target: &str) -> Result<String, CommandError> {
    match secret_entry(target)?.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(error) => Err(CommandError::new("ai.keyStore").with("detail", error)),
    }
}

fn provider_of(target: &Target) -> Result<Box<dyn StorageProvider>, CommandError> {
    Ok(match &target.kind {
        TargetKind::Folder { path } => Box::new(FolderProvider::new(path)),
        TargetKind::WebDav { url, user } => {
            Box::new(WebDavProvider::new(url, user, &read_secret(&target.id)?)?)
        }
        TargetKind::S3 {
            endpoint,
            bucket,
            region,
            access_key,
        } => Box::new(S3Provider::new(
            endpoint,
            bucket,
            region,
            access_key,
            &read_secret(&target.id)?,
        )?),
        TargetKind::Cloud { url, .. } => {
            Box::new(CloudProvider::new(url, &read_secret(&target.id)?)?)
        }
    })
}

/// A folder stays on hardware the user controls. Anything else needs a yes.
fn leaves_this_computer(target: &Target) -> bool {
    match &target.kind {
        TargetKind::Folder { .. } => false,
        TargetKind::WebDav { url, .. }
        | TargetKind::S3 { endpoint: url, .. }
        | TargetKind::Cloud { url, .. } => {
            !matches!(tronic_ai::classify(url), Ok(tronic_ai::Locality::Local))
        }
    }
}

fn load_targets(state: &AppState) -> Result<Vec<Target>, CommandError> {
    let stored = locked(&state.data)?.setting(TARGETS_KEY)?;
    Ok(stored
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

fn save_targets(state: &AppState, targets: &[Target]) -> Result<(), CommandError> {
    let text = serde_json::to_string(targets)
        .map_err(|error| CommandError::new("internal").with("detail", error))?;
    locked(&state.data)?.set_setting(TARGETS_KEY, &text)?;
    Ok(())
}

fn links_key(path: &Path) -> String {
    format!(
        "storage_links:{}",
        sha256_hex(path.display().to_string().as_bytes())
    )
}

fn load_links(state: &AppState, path: &Path) -> Result<Vec<Link>, CommandError> {
    let stored = locked(&state.data)?.setting(&links_key(path))?;
    Ok(stored
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

fn save_links(state: &AppState, path: &Path, links: &[Link]) -> Result<(), CommandError> {
    let text = serde_json::to_string(links)
        .map_err(|error| CommandError::new("internal").with("detail", error))?;
    locked(&state.data)?.set_setting(&links_key(path), &text)?;
    Ok(())
}

fn device_name(state: &AppState) -> Result<String, CommandError> {
    let stored = locked(&state.data)?.setting(DEVICE_KEY)?;
    Ok(stored
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "device".to_owned()))
}

#[derive(Serialize)]
pub struct CloudOverview {
    targets: Vec<Target>,
    device: String,
}

#[tauri::command(async)]
pub fn cloud_overview(state: State<'_, AppState>) -> Result<CloudOverview, CommandError> {
    Ok(CloudOverview {
        targets: load_targets(&state)?,
        device: device_name(&state)?,
    })
}

#[tauri::command(async)]
pub fn cloud_set_device(state: State<'_, AppState>, device: String) -> Result<(), CommandError> {
    let device = device.trim();
    if device.is_empty() || device.chars().count() > 64 {
        return Err(CommandError::new("storage.invalidDevice"));
    }
    locked(&state.data)?.set_setting(DEVICE_KEY, device)?;
    Ok(())
}

fn new_id(state: &AppState) -> Result<String, CommandError> {
    let existing = load_targets(state)?;
    let stamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
    let id = sha256_hex(format!("{stamp}-{}", existing.len()).as_bytes());
    Ok(id[..16].to_owned())
}

fn store_target(
    state: &AppState,
    id: String,
    name: &str,
    kind: TargetKind,
    secret: &str,
) -> Result<Target, CommandError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(CommandError::new("storage.invalidName"));
    }
    if !secret.is_empty() {
        secret_entry(&id)?
            .set_password(secret)
            .map_err(|error| CommandError::new("ai.keyStore").with("detail", error))?;
    }
    let target = Target {
        id,
        name: name.to_owned(),
        kind,
    };
    let mut targets = load_targets(state)?;
    targets.push(target.clone());
    save_targets(state, &targets)?;
    Ok(target)
}

/// Adds a folder, WebDAV or S3 target, after trying the connection, so that a wrong
/// password shows up here and not at the first sync.
#[tauri::command(async)]
pub fn cloud_add_target(
    state: State<'_, AppState>,
    name: String,
    kind: TargetKind,
    secret: String,
) -> Result<Vec<Target>, CommandError> {
    if matches!(kind, TargetKind::Cloud { .. }) {
        return Err(CommandError::new("storage.invalidName"));
    }
    let id = new_id(&state)?;
    let trial = match &kind {
        TargetKind::Folder { path } => {
            if !Path::new(path).is_dir() {
                return Err(StorageError::Unreachable(path.clone()).into());
            }
            Box::new(FolderProvider::new(path)) as Box<dyn StorageProvider>
        }
        TargetKind::WebDav { url, user } => Box::new(WebDavProvider::new(url, user, &secret)?),
        TargetKind::S3 {
            endpoint,
            bucket,
            region,
            access_key,
        } => Box::new(S3Provider::new(
            endpoint, bucket, region, access_key, &secret,
        )?),
        TargetKind::Cloud { .. } => return Err(CommandError::new("storage.invalidName")),
    };
    trial.list("projects")?;
    store_target(&state, id, &name, kind, &secret)?;
    load_targets(&state)
}

/// Creates an account or logs in. The password is used once and not kept, only the
/// login token, and that in the key store of the operating system.
#[tauri::command(async)]
pub fn cloud_account(
    state: State<'_, AppState>,
    name: String,
    url: String,
    email: String,
    password: String,
    create: bool,
    invitation: Option<String>,
) -> Result<Vec<Target>, CommandError> {
    let answer = if create {
        CloudProvider::register_invited(&url, &email, &password, invitation.as_deref())
    } else {
        CloudProvider::login(&url, &email, &password)
    };
    let token = answer.map_err(|error| match &error {
        StorageError::Server { message, .. } if message.contains("weak_password") => {
            CommandError::new("storage.weakPassword")
        }
        StorageError::Server { message, .. } if message.contains("email_taken") => {
            CommandError::new("storage.emailTaken")
        }
        StorageError::Server { message, .. } if message.contains("invalid_email") => {
            CommandError::new("storage.invalidEmail")
        }
        StorageError::Server { status: 429, .. } => CommandError::new("storage.tooManyAttempts"),
        _ => error.into(),
    })?;
    let id = new_id(&state)?;
    store_target(&state, id, &name, TargetKind::Cloud { url, email }, &token)?;
    load_targets(&state)
}

#[derive(Serialize)]
pub struct Quota {
    used_bytes: i64,
    quota_bytes: i64,
}

#[tauri::command(async)]
pub fn cloud_quota(state: State<'_, AppState>, target: String) -> Result<Quota, CommandError> {
    let found = find_target(&state, &target)?;
    let TargetKind::Cloud { url, .. } = &found.kind else {
        return Err(CommandError::new("storage.unknownTarget"));
    };
    let answer = CloudProvider::new(url, &read_secret(&found.id)?)?.account()?;
    Ok(Quota {
        used_bytes: answer["used_bytes"].as_i64().unwrap_or(0),
        quota_bytes: answer["quota_bytes"].as_i64().unwrap_or(0),
    })
}

/// Forgets a target on this computer and ends the login. Nothing stored there is
/// deleted, and links to it from other project files stop working.
#[tauri::command(async)]
pub fn cloud_remove_target(
    state: State<'_, AppState>,
    target: String,
) -> Result<Vec<Target>, CommandError> {
    let mut targets = load_targets(&state)?;
    if let Some(found) = targets.iter().find(|t| t.id == target) {
        if let TargetKind::Cloud { url, .. } = &found.kind {
            // Offline is no reason to keep the entry: the token stays valid on the
            // server until it expires, which is reported in the known issues.
            if let Ok(client) = CloudProvider::new(url, &read_secret(&found.id)?) {
                let _ = client.logout();
            }
        }
    }
    match secret_entry(&target)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(CommandError::new("ai.keyStore").with("detail", error)),
    }
    targets.retain(|t| t.id != target);
    save_targets(&state, &targets)?;
    Ok(targets)
}

fn find_target(state: &AppState, id: &str) -> Result<Target, CommandError> {
    load_targets(state)?
        .into_iter()
        .find(|target| target.id == id)
        .ok_or_else(|| CommandError::new("storage.unknownTarget"))
}

struct OpenProject {
    id: String,
    path: PathBuf,
    dirty: bool,
}

fn open_project(state: &AppState) -> Result<OpenProject, CommandError> {
    state.with_session(|session| {
        Ok(OpenProject {
            id: session.project().id.to_string(),
            path: session.path().to_owned(),
            dirty: session.is_dirty(),
        })
    })
}

#[derive(Serialize)]
pub struct LinkView {
    target: String,
    mode: Mode,
    auto: bool,
    versions: usize,
    stored_bytes: u64,
    base: Option<String>,
}

fn link_views(links: &[Link]) -> Vec<LinkView> {
    links
        .iter()
        .map(|link| LinkView {
            target: link.target.clone(),
            mode: link.mode,
            auto: link.auto,
            versions: link.state.known.len(),
            stored_bytes: Repo::stored_bytes(&link.state),
            base: link.state.base.clone(),
        })
        .collect()
}

#[tauri::command(async)]
pub fn cloud_links(state: State<'_, AppState>) -> Result<Vec<LinkView>, CommandError> {
    let project = open_project(&state)?;
    Ok(link_views(&load_links(&state, &project.path)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct LinkRequest {
    mode: Mode,
    auto: bool,
    /// The user was told that the project leaves this computer, and agreed.
    confirmed: bool,
}

/// Connects the open project to a target. Copying a project to somewhere else
/// needs the same kind of consent as sending it to an AI provider.
#[tauri::command(async)]
pub fn cloud_link(
    state: State<'_, AppState>,
    target: String,
    request: LinkRequest,
) -> Result<Vec<LinkView>, CommandError> {
    let found = find_target(&state, &target)?;
    if leaves_this_computer(&found) && !request.confirmed {
        return Err(CommandError::new("storage.consentRequired"));
    }
    let project = open_project(&state)?;
    let mut links = load_links(&state, &project.path)?;
    if let Some(link) = links.iter_mut().find(|link| link.target == target) {
        link.mode = request.mode;
        link.auto = request.auto;
    } else {
        links.push(Link {
            target,
            mode: request.mode,
            auto: request.auto,
            state: LinkState::default(),
        });
    }
    save_links(&state, &project.path, &links)?;
    Ok(link_views(&links))
}

/// Ends a link. Nothing stored at the target is deleted.
#[tauri::command(async)]
pub fn cloud_unlink(
    state: State<'_, AppState>,
    target: String,
) -> Result<Vec<LinkView>, CommandError> {
    let project = open_project(&state)?;
    let mut links = load_links(&state, &project.path)?;
    links.retain(|link| link.target != target);
    save_links(&state, &project.path, &links)?;
    Ok(link_views(&links))
}

#[derive(Serialize)]
pub struct SyncResult {
    target: String,
    /// What happened, if the connection worked.
    outcome: Option<Outcome>,
    /// Set if two states were brought together because they touched different areas.
    merged: Option<MergeReport>,
    error: Option<CommandError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergeReport {
    from_here: Vec<&'static str>,
    from_there: Vec<&'static str>,
    /// Differences between schematic and board in the result.
    deviations: usize,
}

/// Tries to end a conflict by areas: what touched different parts of the project
/// comes together, and only the same part changed twice stays for a person.
/// The result is stored first: if replacing the file fails afterwards, the next
/// run finds the same content in the storage and nothing is lost.
fn try_area_merge(
    repo: &Repo<'_>,
    local: &[u8],
    device: &str,
    state: &mut LinkState,
    heads: &[String],
) -> Result<Option<(Vec<u8>, MergeReport)>, CommandError> {
    let local_hash = sha256_hex(local);
    let (theirs_id, base_id) = match heads {
        [head] => (head.clone(), state.base.clone()),
        [a, b] => {
            let blob = |id: &String| state.known.get(id).map(|commit| commit.blob.clone());
            let theirs = if blob(a).as_deref() == Some(local_hash.as_str()) {
                b
            } else if blob(b).as_deref() == Some(local_hash.as_str()) {
                a
            } else {
                return Ok(None);
            };
            (theirs.clone(), Repo::merge_base(state, a, b))
        }
        _ => return Ok(None),
    };
    let Some(base_id) = base_id else {
        return Ok(None);
    };
    let base = repo.read(state, &base_id)?;
    let theirs = repo.read(state, &theirs_id)?;
    let Merge::Merged {
        bytes,
        from_ours,
        from_theirs,
        deviations,
    } = merge(&base, local, &theirs)?
    else {
        return Ok(None);
    };
    let names =
        |areas: &std::collections::BTreeSet<Area>| areas.iter().map(|area| area.name()).collect();
    let report = MergeReport {
        from_here: names(&from_ours),
        from_there: names(&from_theirs),
        deviations,
    };
    repo.resolve_keep_local(&bytes, device, state)?;
    Ok(Some((bytes, report)))
}

#[derive(Serialize)]
pub struct SyncReport {
    results: Vec<SyncResult>,
    /// Set if a newer version was loaded, and the open project with it.
    project: Option<ProjectView>,
    links: Vec<LinkView>,
}

/// Puts stored bytes in place of the local file, if nothing changed in between.
/// The previous file stays as the usual `.bak`.
fn replace_project(
    state: &AppState,
    path: &Path,
    seen: &[u8],
    bytes: &[u8],
) -> Result<ProjectView, CommandError> {
    format::decode(bytes, path)?;
    state.with_session(|session| {
        if session.is_dirty() {
            return Err(CommandError::new("storage.saveFirst"));
        }
        if store::read_file(path)? != seen {
            return Err(CommandError::new("storage.changedMeanwhile"));
        }
        store::write_atomic(path, bytes)?;
        Ok(())
    })?;
    state.ai.reset();
    state.install(ProjectSession::open(path)?)
}

/// Synchronizes the links of the open project. Unsaved changes are not touched and
/// not sent: what counts is the saved file.
#[tauri::command(async)]
pub fn cloud_sync(state: State<'_, AppState>, only_auto: bool) -> Result<SyncReport, CommandError> {
    let project = open_project(&state)?;
    let mut links = load_links(&state, &project.path)?;
    if project.dirty && links.iter().any(|link| link.mode == Mode::Sync) && !only_auto {
        return Err(CommandError::new("storage.saveFirst"));
    }
    let device = device_name(&state)?;
    let mut local = store::read_file(&project.path)?;
    let mut results = Vec::new();
    let mut reloaded = None;
    for link in &mut links {
        if only_auto && !link.auto {
            continue;
        }
        // Waiting is fine for an automatic run: the user is working.
        if project.dirty {
            continue;
        }
        let mut merged = None;
        let run = (|| -> Result<(Outcome, LinkState, Option<ProjectView>), CommandError> {
            let target = find_target(&state, &link.target)?;
            let provider = provider_of(&target)?;
            let repo = Repo::new(provider.as_ref(), &project.id);
            let mut next = link.state.clone();
            let outcome = match link.mode {
                Mode::Sync => repo.sync(&local, &device, &mut next)?,
                Mode::Backup => repo.backup(&local, &device, &mut next)?,
            };
            let mut outcome = outcome;
            let mut view = None;
            if let Outcome::Conflict { heads, .. } = &outcome {
                if let Some((bytes, report)) =
                    try_area_merge(&repo, &local, &device, &mut next, heads)?
                {
                    if bytes != local {
                        view = Some(replace_project(&state, &project.path, &local, &bytes)?);
                    }
                    outcome = Outcome::Pushed {
                        commit: next.base.clone().unwrap_or_default(),
                    };
                    merged = Some(report);
                }
            }
            if let Outcome::Pulled { bytes, .. } = &outcome {
                view = Some(replace_project(&state, &project.path, &local, bytes)?);
            }
            Ok((outcome, next, view))
        })();
        match run {
            Ok((outcome, next, view)) => {
                // Only after the file is in place: an interruption before this is
                // recognized by the next run.
                link.state = next;
                if view.is_some() {
                    local = store::read_file(&project.path)?;
                    reloaded = view;
                }
                results.push(SyncResult {
                    target: link.target.clone(),
                    outcome: Some(outcome),
                    merged,
                    error: None,
                });
            }
            Err(error) => results.push(SyncResult {
                target: link.target.clone(),
                outcome: None,
                merged: None,
                error: Some(error),
            }),
        }
    }
    save_links(&state, &project.path, &links)?;
    Ok(SyncReport {
        results,
        project: reloaded,
        links: link_views(&links),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// The local state becomes the newest version. The other stays in the history.
    KeepLocal,
    /// The other state replaces the local file, which is kept as a copy.
    TakeRemote,
    /// The local state stays, and the other is written beside it as a copy.
    KeepBoth,
}

/// A name beside the project that does not exist yet.
fn copy_path(path: &Path, label: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = OffsetDateTime::now_utc()
        .format(format_description!("[year]-[month]-[day] [hour][minute]"))
        .unwrap_or_default();
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut number = 1;
    loop {
        let suffix = if number == 1 {
            String::new()
        } else {
            format!(" {number}")
        };
        let candidate = directory.join(format!("{stem} ({label} {stamp}{suffix}).tronic"));
        if !candidate.exists() {
            return candidate;
        }
        number += 1;
    }
}

#[derive(Serialize)]
pub struct ResolveReport {
    project: Option<ProjectView>,
    /// A file written beside the project, so that nothing is lost.
    copy: Option<String>,
    links: Vec<LinkView>,
}

/// Ends a conflict the way the user chose. Whatever is not chosen stays: in the
/// history of the storage, or as a copy next to the project.
#[tauri::command(async)]
pub fn cloud_resolve(
    state: State<'_, AppState>,
    target: String,
    head: String,
    choice: Choice,
) -> Result<ResolveReport, CommandError> {
    let project = open_project(&state)?;
    if project.dirty {
        return Err(CommandError::new("storage.saveFirst"));
    }
    let found = find_target(&state, &target)?;
    let provider = provider_of(&found)?;
    let repo = Repo::new(provider.as_ref(), &project.id);
    let device = device_name(&state)?;
    let local = store::read_file(&project.path)?;
    let mut links = load_links(&state, &project.path)?;
    let link = links
        .iter_mut()
        .find(|link| link.target == target)
        .ok_or_else(|| CommandError::new("storage.unknownTarget"))?;
    let mut next = link.state.clone();
    let mut copy = None;
    let mut view_after = None;
    match choice {
        Choice::KeepLocal => {
            repo.resolve_keep_local(&local, &device, &mut next)?;
        }
        Choice::KeepBoth => {
            repo.refresh(&mut next)?;
            let other = repo.read(&next, &head)?;
            let path = copy_path(&project.path, "conflict");
            store::replace_atomic(&path, &other)?;
            copy = Some(path.display().to_string());
            repo.resolve_keep_local(&local, &device, &mut next)?;
        }
        Choice::TakeRemote => {
            let bytes = repo.resolve_take_remote(&head, &device, &mut next)?;
            let path = copy_path(&project.path, "local");
            store::replace_atomic(&path, &local)?;
            copy = Some(path.display().to_string());
            view_after = Some(replace_project(&state, &project.path, &local, &bytes)?);
        }
    }
    link.state = next;
    save_links(&state, &project.path, &links)?;
    Ok(ResolveReport {
        project: view_after,
        copy,
        links: link_views(&links),
    })
}

#[tauri::command(async)]
pub fn cloud_versions(
    state: State<'_, AppState>,
    target: String,
) -> Result<Vec<Version>, CommandError> {
    let project = open_project(&state)?;
    let found = find_target(&state, &target)?;
    let provider = provider_of(&found)?;
    let repo = Repo::new(provider.as_ref(), &project.id);
    let mut links = load_links(&state, &project.path)?;
    let link = links
        .iter_mut()
        .find(|link| link.target == target)
        .ok_or_else(|| CommandError::new("storage.unknownTarget"))?;
    repo.refresh(&mut link.state)?;
    let versions = Repo::versions(&link.state);
    save_links(&state, &project.path, &links)?;
    Ok(versions)
}

/// What differs from the saved project to a stored version.
#[tauri::command(async)]
pub fn cloud_compare(
    state: State<'_, AppState>,
    target: String,
    commit: String,
) -> Result<Comparison, CommandError> {
    let project = open_project(&state)?;
    let found = find_target(&state, &target)?;
    let provider = provider_of(&found)?;
    let repo = Repo::new(provider.as_ref(), &project.id);
    let links = load_links(&state, &project.path)?;
    let link = links
        .iter()
        .find(|link| link.target == target)
        .ok_or_else(|| CommandError::new("storage.unknownTarget"))?;
    let stored = repo.read(&link.state, &commit)?;
    let local = store::read_file(&project.path)?;
    Ok(compare(&local, &stored)?)
}

/// Brings back a stored state. The file is replaced, its previous state stays as a
/// `.bak`, and the next sync stores it as the newest version: history only grows.
#[tauri::command(async)]
pub fn cloud_restore(
    state: State<'_, AppState>,
    target: String,
    commit: String,
) -> Result<ProjectView, CommandError> {
    let project = open_project(&state)?;
    if project.dirty {
        return Err(CommandError::new("storage.saveFirst"));
    }
    let found = find_target(&state, &target)?;
    let provider = provider_of(&found)?;
    let repo = Repo::new(provider.as_ref(), &project.id);
    let links = load_links(&state, &project.path)?;
    let link = links
        .iter()
        .find(|link| link.target == target)
        .ok_or_else(|| CommandError::new("storage.unknownTarget"))?;
    let bytes = repo.read(&link.state, &commit)?;
    let local = store::read_file(&project.path)?;
    replace_project(&state, &project.path, &local, &bytes)
}

/// The view of the open project, for the interface after a change here.
#[tauri::command(async)]
pub fn cloud_project(state: State<'_, AppState>) -> Result<ProjectView, CommandError> {
    state.with_session(|session| Ok(view(session)))
}

/// Sends a request of the team functions to a cloud server, with the login kept
/// here. Only these paths: this is not a general way out of the application.
#[tauri::command(async)]
pub fn team_call(
    state: State<'_, AppState>,
    target: String,
    method: String,
    path: String,
    body: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    const ALLOWED: [&str; 4] = [
        "/v1/teams",
        "/v1/invitations",
        "/v1/projects",
        "/v1/notifications",
    ];
    if !ALLOWED.iter().any(|prefix| path.starts_with(prefix)) || path.contains("..") {
        return Err(CommandError::new("storage.invalidUrl"));
    }
    if !matches!(method.as_str(), "GET" | "POST" | "PATCH" | "DELETE") {
        return Err(CommandError::new("storage.invalidUrl"));
    }
    let found = find_target(&state, &target)?;
    let TargetKind::Cloud { url, .. } = &found.kind else {
        return Err(CommandError::new("storage.unknownTarget"));
    };
    let client = CloudProvider::new(url, &read_secret(&found.id)?)?;
    client.call(&method, &path, &body).map_err(team_error)
}

/// The reason a team server gives, for the interface to translate.
fn team_error(error: StorageError) -> CommandError {
    if let StorageError::Server { message, .. } = &error {
        if let Ok(answer) = serde_json::from_str::<serde_json::Value>(message) {
            if let Some(code) = answer["error"].as_str() {
                return match code {
                    "last_admin" => CommandError::new("team.lastAdmin"),
                    "invalid_invitation" => CommandError::new("team.invalidInvitation"),
                    "invalid_email" => CommandError::new("storage.invalidEmail"),
                    "invalid_name" | "invalid_comment" => CommandError::new("storage.invalidName"),
                    _ => error.into(),
                };
            }
        }
    }
    error.into()
}

/// Fetches the newest state of a team project into a new file, and opens it.
#[tauri::command(async)]
pub fn team_clone(
    state: State<'_, AppState>,
    target: String,
    repo: String,
    path: PathBuf,
) -> Result<ProjectView, CommandError> {
    let found = find_target(&state, &target)?;
    let provider = provider_of(&found)?;
    let store_repo = Repo::new(provider.as_ref(), &repo);
    let mut link_state = LinkState::default();
    store_repo.refresh(&mut link_state)?;
    let newest = Repo::versions(&link_state)
        .into_iter()
        .find(|version| version.is_head)
        .ok_or_else(|| CommandError::new("team.emptyProject"))?;
    let bytes = store_repo.read(&link_state, &newest.id)?;
    format::decode(&bytes, &path)?;
    if path.exists() {
        return Err(CommandError::new("storage.changedMeanwhile"));
    }
    store::replace_atomic(&path, &bytes)?;
    link_state.base = Some(newest.id);
    save_links(
        &state,
        &path,
        &[Link {
            target,
            mode: Mode::Sync,
            auto: false,
            state: link_state,
        }],
    )?;
    state.ai.reset();
    state.install(ProjectSession::open(&path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: TargetKind) -> Target {
        Target {
            id: "t".to_owned(),
            name: "t".to_owned(),
            kind,
        }
    }

    #[test]
    fn only_folders_and_this_computer_need_no_consent() {
        let local = |url: &str| {
            leaves_this_computer(&target(TargetKind::WebDav {
                url: url.to_owned(),
                user: "u".to_owned(),
            }))
        };
        assert!(!leaves_this_computer(&target(TargetKind::Folder {
            path: "D:/x".to_owned()
        })));
        assert!(!local("http://127.0.0.1:8080"));
        assert!(!local("http://localhost"));
        assert!(local("https://dav.example.com"));
        assert!(local("https://192.168.1.5"));
        assert!(local("https://localhost.example.com"));
        assert!(leaves_this_computer(&target(TargetKind::Cloud {
            url: "https://cloud.example.com".to_owned(),
            email: "a@b.c".to_owned()
        })));
    }

    #[test]
    fn copies_do_not_overwrite_anything() {
        let directory = std::env::temp_dir().join(format!("troniccad-copy-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        let project = directory.join("Name.tronic");
        let first = copy_path(&project, "conflict");
        assert!(first.display().to_string().contains("Name (conflict "));
        let _ = std::fs::write(&first, b"x");
        assert_ne!(copy_path(&project, "conflict"), first);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
