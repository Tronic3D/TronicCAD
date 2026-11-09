//! Tauri commands of the board editor and manufacturing export.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tronic_drc::Violation;
use tronic_library::{find_footprint, footprints};
use tronic_pcb::{board_view, compare_with_netlist, BoardView, Deviation, PcbCommand, SyncReport};
use tronic_project::{store, ProjectSession};
use tronic_router::{route_board, RouteOptions, RouteResult};
use tronic_schematic::ElementId;

use crate::error::CommandError;
use crate::{locked, AppState};

/// Longest time the router runs before it returns what it has.
const ROUTE_TIME_LIMIT: Duration = Duration::from_secs(120);

#[derive(Serialize)]
pub struct PcbView {
    board: BoardView,
    deviations: Vec<Deviation>,
    can_undo: bool,
    can_redo: bool,
    dirty: bool,
}

fn view(session: &ProjectSession) -> PcbView {
    PcbView {
        board: board_view(session.board()),
        deviations: compare_with_netlist(session.board(), &session.netlist()),
        can_undo: session.pcb_can_undo(),
        can_redo: session.pcb_can_redo(),
        dirty: session.is_dirty(),
    }
}

#[tauri::command(async)]
pub fn pcb_view(state: State<'_, AppState>) -> Result<PcbView, CommandError> {
    state.with_session(|session| Ok(view(session)))
}

#[derive(Serialize)]
pub struct PcbApplyResult {
    view: PcbView,
    created: Vec<ElementId>,
}

#[tauri::command(async)]
pub fn pcb_apply(
    state: State<'_, AppState>,
    command: PcbCommand,
) -> Result<PcbApplyResult, CommandError> {
    state.with_session(|session| {
        let applied = session.apply_pcb(command)?;
        Ok(PcbApplyResult {
            view: view(session),
            created: applied.created,
        })
    })
}

#[tauri::command(async)]
pub fn pcb_undo(state: State<'_, AppState>) -> Result<PcbView, CommandError> {
    state.with_session(|session| {
        session.undo_pcb()?;
        Ok(view(session))
    })
}

#[tauri::command(async)]
pub fn pcb_redo(state: State<'_, AppState>) -> Result<PcbView, CommandError> {
    state.with_session(|session| {
        session.redo_pcb()?;
        Ok(view(session))
    })
}

#[derive(Serialize)]
pub struct PcbSyncResult {
    view: PcbView,
    report: SyncReport,
}

#[tauri::command(async)]
pub fn pcb_sync(state: State<'_, AppState>) -> Result<PcbSyncResult, CommandError> {
    state.with_session(|session| {
        let report = session.sync_board(&find_footprint);
        Ok(PcbSyncResult {
            view: view(session),
            report,
        })
    })
}

#[tauri::command(async)]
pub fn pcb_drc(state: State<'_, AppState>) -> Result<Vec<Violation>, CommandError> {
    state.with_session(|session| Ok(tronic_drc::check(session.board())))
}

/// Writes all manufacturing files and returns their names.
#[tauri::command(async)]
pub fn pcb_export(
    state: State<'_, AppState>,
    directory: PathBuf,
) -> Result<Vec<String>, CommandError> {
    state.with_session(|session| {
        let files = tronic_fab::generate(session.board(), &session.project().name);
        let mut names = Vec::with_capacity(files.len());
        for file in files {
            store::replace_atomic(&directory.join(&file.name), file.content.as_bytes())?;
            names.push(file.name);
        }
        Ok(names)
    })
}

#[derive(Serialize)]
pub struct FootprintEntry {
    id: String,
    name: String,
}

#[tauri::command(async)]
pub fn library_footprints() -> Vec<FootprintEntry> {
    footprints()
        .into_iter()
        .map(|footprint| FootprintEntry {
            id: footprint.id,
            name: footprint.name,
        })
        .collect()
}

#[derive(Clone, Serialize)]
struct RouteProgress {
    done: usize,
    total: usize,
}

/// Routes outside the session lock, so that the application stays responsive,
/// and keeps the board state the proposal belongs to.
#[tauri::command(async)]
pub fn pcb_route_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RouteResult, CommandError> {
    // Before waiting for the session: autosave can hold it for a while, and a
    // cancel arriving during that wait must not be wiped out.
    state.route_cancel.store(false, Ordering::Relaxed);
    let board = state.with_session(|session| Ok(session.board().clone()))?;
    let progress = |done: usize, total: usize| {
        let _ = app.emit("router-progress", RouteProgress { done, total });
    };
    let result = route_board(
        &board,
        &RouteOptions {
            time_limit: ROUTE_TIME_LIMIT,
            cancel: &state.route_cancel,
            progress: &progress,
        },
    )?;
    *locked(&state.route_proposal)? = Some((board, result.clone()));
    Ok(result)
}

#[tauri::command(async)]
pub fn pcb_route_cancel(state: State<'_, AppState>) {
    state.route_cancel.store(true, Ordering::Relaxed);
}

/// Applies the proposal as one undo step, if the board has not changed since.
#[tauri::command(async)]
pub fn pcb_route_accept(state: State<'_, AppState>) -> Result<PcbApplyResult, CommandError> {
    let proposal = locked(&state.route_proposal)?
        .take()
        .ok_or_else(|| CommandError::new("pcb.noProposal"))?;
    state.with_session(|session| {
        if *session.board() != proposal.0 {
            return Err(CommandError::new("pcb.routeStale"));
        }
        let applied = session.apply_pcb(PcbCommand::AddRoutes {
            tracks: proposal.1.tracks,
            vias: proposal.1.vias,
        })?;
        Ok(PcbApplyResult {
            view: view(session),
            created: applied.created,
        })
    })
}

#[tauri::command(async)]
pub fn pcb_route_discard(state: State<'_, AppState>) -> Result<(), CommandError> {
    *locked(&state.route_proposal)? = None;
    Ok(())
}
