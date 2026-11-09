//! TronicCAD desktop backend: a thin command layer over the core crates.
//! Application logic belongs into the crates, not here.

mod ai;
mod cloud;
mod error;
mod extensions;
mod pcb;
mod schematic;
mod sim;
mod update;

use error::CommandError;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{Emitter, Manager, State};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tronic_appdata::AppData;
use tronic_pcb::Board;
use tronic_project::{AutosaveStore, ProjectSession, RecoveryEntry, SaveMode, FORMAT_VERSION};
use tronic_router::RouteResult;

const LANGUAGE_KEY: &str = "language";
const SUPPORTED_LANGUAGES: [&str; 4] = ["de-DE", "en-US", "zh-CN", "fr-FR"];
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);
/// Overrides the data directory, for portable use and for isolated tests.
const DATA_DIR_VARIABLE: &str = "TRONICCAD_DATA_DIR";

pub(crate) struct AppState {
    session: Mutex<Option<ProjectSession>>,
    data: Mutex<AppData>,
    /// False if settings could not be opened and only live in memory.
    settings_persistent: bool,
    /// None if the snapshot directory is unavailable; the interface warns.
    autosave: Option<AutosaveStore>,
    route_cancel: AtomicBool,
    /// The last router result, with the board state it was computed for.
    route_proposal: Mutex<Option<(Board, RouteResult)>>,
    ai: ai::AiState,
    update: update::UpdateState,
    /// None if there is none: updates are then not possible.
    data_directory: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectView {
    id: String,
    path: String,
    name: String,
    description: String,
    created_at: String,
    modified_at: String,
    dirty: bool,
    format_version: u32,
    /// Unsaved work of an ended application for this file, if any.
    recovery: Option<RecoveryEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentView {
    path: String,
    name: String,
    opened_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    /// The name of a beta build, for example "Beta 1".
    beta: Option<&'static str>,
    settings_persistent: bool,
    autosave_available: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutosaveEvent {
    saved_at: Option<String>,
    error: Option<CommandError>,
}

fn rfc3339(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}

fn view(session: &ProjectSession) -> ProjectView {
    let project = session.project();
    ProjectView {
        id: project.id.to_string(),
        path: session.path().display().to_string(),
        name: project.name.clone(),
        description: project.description.clone(),
        created_at: rfc3339(project.created_at),
        modified_at: rfc3339(project.modified_at),
        dirty: session.is_dirty(),
        format_version: FORMAT_VERSION,
        recovery: None,
    }
}

pub(crate) fn locked<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, CommandError> {
    mutex
        .lock()
        .map_err(|_| CommandError::new("internal.state"))
}

impl AppState {
    /// Records a project as recently used. A failure here must not fail the
    /// file operation that already succeeded.
    fn remember(&self, session: &ProjectSession) {
        let now = rfc3339(OffsetDateTime::now_utc());
        let path = session.path().display().to_string();
        let result = locked(&self.data).and_then(|data| {
            data.touch_recent(&path, &session.project().name, &now)
                .map_err(CommandError::from)
        });
        if let Err(error) = result {
            eprintln!("recent projects not updated: {error:?}");
        }
    }

    /// Drops this application's snapshot of a project that no longer needs one.
    fn forget_snapshot(&self, path: &Path) {
        if let Some(store) = &self.autosave {
            if let Err(error) = store.remove_own(path) {
                eprintln!("snapshot not removed: {error}");
            }
        }
    }

    fn install(&self, session: ProjectSession) -> Result<ProjectView, CommandError> {
        self.remember(&session);
        let result = view(&session);
        let mut guard = locked(&self.session)?;
        // Whoever replaces a project has already decided about its changes.
        if let Some(previous) = guard.as_ref() {
            if previous.path() != session.path() {
                self.forget_snapshot(previous.path());
            }
        }
        *guard = Some(session);
        Ok(result)
    }

    /// Writes a snapshot if there are new unsaved changes.
    fn autosave_tick(&self) -> Result<Option<String>, CommandError> {
        let Some(store) = &self.autosave else {
            return Ok(None);
        };
        let mut guard = locked(&self.session)?;
        let Some(session) = guard.as_mut() else {
            return Ok(None);
        };
        Ok(store.write_if_needed(session)?)
    }

    pub(crate) fn with_session<R>(
        &self,
        action: impl FnOnce(&mut ProjectSession) -> Result<R, CommandError>,
    ) -> Result<R, CommandError> {
        let mut guard = locked(&self.session)?;
        let session = guard
            .as_mut()
            .ok_or_else(|| CommandError::new("project.none"))?;
        action(session)
    }
}

#[tauri::command(async)]
fn app_info(state: State<'_, AppState>) -> Result<AppInfo, CommandError> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        beta: option_env!("TRONICCAD_BETA_LABEL"),
        settings_persistent: state.settings_persistent,
        autosave_available: state.autosave.is_some(),
    })
}

/// What helps support, and nothing about projects or accounts.
#[derive(Serialize)]
struct SupportInfo {
    version: &'static str,
    platform: String,
    settings_persistent: bool,
    contact: Option<&'static str>,
}

#[tauri::command(async)]
fn support_info(state: State<'_, AppState>) -> Result<SupportInfo, CommandError> {
    Ok(SupportInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: tronic_update::this_platform(),
        settings_persistent: state.settings_persistent,
        contact: option_env!("TRONICCAD_SUPPORT_CONTACT"),
    })
}

#[tauri::command(async)]
fn project_create(
    state: State<'_, AppState>,
    path: PathBuf,
    name: String,
    description: String,
) -> Result<ProjectView, CommandError> {
    state.ai.reset();
    state.install(ProjectSession::create(&path, &name, &description)?)
}

#[tauri::command(async)]
fn project_open(state: State<'_, AppState>, path: PathBuf) -> Result<ProjectView, CommandError> {
    state.ai.reset();
    let session = ProjectSession::open(&path)?;
    let recovery = state
        .autosave
        .as_ref()
        .and_then(|store| store.recoverable_for(&path));
    if recovery.is_none() {
        // Reopening from disk after declining to save discards the snapshot.
        state.forget_snapshot(&path);
    }
    let mut result = state.install(session)?;
    result.recovery = recovery;
    Ok(result)
}

#[tauri::command(async)]
fn recovery_list(state: State<'_, AppState>) -> Vec<RecoveryEntry> {
    state
        .autosave
        .as_ref()
        .map(AutosaveStore::recoverable)
        .unwrap_or_default()
}

#[tauri::command(async)]
fn project_recover(state: State<'_, AppState>, path: PathBuf) -> Result<ProjectView, CommandError> {
    state.ai.reset();
    let store = state
        .autosave
        .as_ref()
        .ok_or_else(|| CommandError::new("project.none"))?;
    state.install(store.recover(&path)?)
}

#[tauri::command(async)]
fn recovery_discard(state: State<'_, AppState>, path: PathBuf) -> Result<(), CommandError> {
    if let Some(store) = &state.autosave {
        store.discard(&path)?;
    }
    Ok(())
}

#[tauri::command(async)]
fn autosave_now(state: State<'_, AppState>) -> Result<Option<String>, CommandError> {
    state.autosave_tick()
}

#[tauri::command(async)]
fn project_update(
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Result<ProjectView, CommandError> {
    state.with_session(|session| {
        session.update_metadata(&name, &description)?;
        Ok(view(session))
    })
}

#[tauri::command(async)]
fn project_save(state: State<'_, AppState>, overwrite: bool) -> Result<ProjectView, CommandError> {
    let mode = if overwrite {
        SaveMode::Overwrite
    } else {
        SaveMode::IfUnchanged
    };
    state.with_session(|session| {
        session.save(mode)?;
        state.forget_snapshot(session.path());
        state.remember(session);
        Ok(view(session))
    })
}

#[tauri::command(async)]
fn project_save_as(state: State<'_, AppState>, path: PathBuf) -> Result<ProjectView, CommandError> {
    state.with_session(|session| {
        let previous = session.path().to_owned();
        session.save_as(&path)?;
        state.forget_snapshot(&previous);
        state.remember(session);
        Ok(view(session))
    })
}

#[tauri::command(async)]
fn project_close(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.ai.reset();
    let closed = locked(&state.session)?.take();
    if let Some(session) = closed {
        state.forget_snapshot(session.path());
    }
    Ok(())
}

#[tauri::command(async)]
fn recent_list(state: State<'_, AppState>) -> Result<Vec<RecentView>, CommandError> {
    let entries = locked(&state.data)?.recent()?;
    Ok(entries
        .into_iter()
        .map(|entry| RecentView {
            path: entry.path,
            name: entry.name,
            opened_at: entry.opened_at,
        })
        .collect())
}

#[tauri::command(async)]
fn recent_remove(state: State<'_, AppState>, path: String) -> Result<(), CommandError> {
    locked(&state.data)?.remove_recent(&path)?;
    Ok(())
}

#[tauri::command(async)]
fn language_get(state: State<'_, AppState>) -> Result<Option<String>, CommandError> {
    let stored = locked(&state.data)?.setting(LANGUAGE_KEY)?;
    Ok(stored.filter(|value| SUPPORTED_LANGUAGES.contains(&value.as_str())))
}

#[tauri::command(async)]
fn language_set(state: State<'_, AppState>, language: String) -> Result<(), CommandError> {
    if !SUPPORTED_LANGUAGES.contains(&language.as_str()) {
        return Err(CommandError::new("language.unsupported"));
    }
    locked(&state.data)?.set_setting(LANGUAGE_KEY, &language)?;
    Ok(())
}

fn open_app_data(directory: &Path) -> Result<AppData, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(directory)?;
    Ok(AppData::open(&directory.join("troniccad.db"))?)
}

/// Runs the application until its window closes.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg_attr(not(debug_assertions), allow(unused_mut))]
    let mut context = tauri::generate_context!();
    // In development only: the web view does not honor this variable everywhere, and
    // the end-to-end tests reach the interface through what it asks for.
    #[cfg(debug_assertions)]
    if let Ok(arguments) = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        for window in &mut context.config_mut().app.windows {
            window.additional_browser_args = Some(format!(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {arguments}"
            ));
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = match std::env::var_os(DATA_DIR_VARIABLE) {
                Some(directory) => Ok(PathBuf::from(directory)),
                None => app.path().app_data_dir(),
            };
            let data_directory = directory.as_ref().ok().cloned();
            let (data, settings_persistent, autosave) = match directory {
                Ok(directory) => {
                    let autosave = AutosaveStore::open(&directory.join("autosave"))
                        .map_err(|error| eprintln!("autosave unavailable: {error}"))
                        .ok();
                    match open_app_data(&directory) {
                        Ok(data) => (data, true, autosave),
                        Err(error) => {
                            eprintln!("settings database unavailable: {error}");
                            (AppData::in_memory()?, false, autosave)
                        }
                    }
                }
                // The application stays usable; the interface shows warnings.
                Err(error) => {
                    eprintln!("data directory unavailable: {error}");
                    (AppData::in_memory()?, false, None)
                }
            };
            app.manage(AppState {
                session: Mutex::new(None),
                data: Mutex::new(data),
                settings_persistent,
                autosave,
                route_cancel: AtomicBool::new(false),
                route_proposal: Mutex::new(None),
                ai: ai::AiState::default(),
                update: update::UpdateState::default(),
                data_directory,
            });
            update::start_checking(app.handle().clone());
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(AUTOSAVE_INTERVAL);
                let event = match handle.state::<AppState>().autosave_tick() {
                    Ok(None) => continue,
                    Ok(Some(saved_at)) => AutosaveEvent {
                        saved_at: Some(saved_at),
                        error: None,
                    },
                    Err(error) => AutosaveEvent {
                        saved_at: None,
                        error: Some(error),
                    },
                };
                let _ = handle.emit("autosave", event);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            support_info,
            extensions::extensions_list,
            extensions::extensions_trust,
            extensions::extensions_untrust,
            extensions::extensions_run,
            extensions::extensions_open_folder,
            extensions::third_party_components,
            extensions::third_party_notices,
            extensions::own_license,
            project_create,
            project_open,
            project_update,
            project_save,
            project_save_as,
            project_close,
            project_recover,
            recovery_list,
            recovery_discard,
            autosave_now,
            recent_list,
            recent_remove,
            language_get,
            language_set,
            schematic::schematic_view,
            schematic::schematic_apply,
            schematic::schematic_undo,
            schematic::schematic_redo,
            schematic::schematic_erc,
            schematic::schematic_netlist,
            schematic::schematic_export_netlist,
            schematic::schematic_net_of,
            schematic::library_list,
            schematic::library_create_part,
            schematic::library_import,
            schematic::library_export,
            pcb::pcb_view,
            pcb::pcb_apply,
            pcb::pcb_undo,
            pcb::pcb_redo,
            pcb::pcb_sync,
            pcb::pcb_drc,
            pcb::pcb_export,
            pcb::library_footprints,
            pcb::pcb_route_start,
            pcb::pcb_route_cancel,
            pcb::pcb_route_accept,
            pcb::pcb_route_discard,
            sim::sim_prepare,
            sim::sim_run,
            sim::sim_save_text,
            sim::view3d_scene,
            sim::view3d_import_model,
            ai::ai_settings,
            ai::ai_set_provider,
            ai::ai_set_key,
            ai::ai_delete_key,
            ai::ai_status,
            ai::ai_analysis,
            ai::ai_prepare,
            ai::ai_send,
            ai::ai_cancel,
            ai::ai_apply,
            ai::ai_discard,
            cloud::cloud_overview,
            cloud::cloud_set_device,
            cloud::cloud_add_target,
            cloud::cloud_account,
            cloud::cloud_quota,
            cloud::cloud_remove_target,
            cloud::cloud_links,
            cloud::cloud_link,
            cloud::cloud_unlink,
            cloud::cloud_sync,
            cloud::cloud_resolve,
            cloud::cloud_versions,
            cloud::cloud_compare,
            cloud::cloud_restore,
            cloud::cloud_project,
            cloud::team_call,
            cloud::team_clone,
            update::update_status,
            update::update_set_channel,
            update::update_set_automatic,
            update::update_check,
            update::update_download,
            update::update_install,
            update::update_confirm,
            update::update_rollback,
            update::update_open_backup,
        ])
        .run(context)?;
    Ok(())
}
