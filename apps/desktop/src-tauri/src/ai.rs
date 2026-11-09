//! Tauri commands of the AI assistant. The rules of the module are enforced here
//! and in `tronic-ai`, not in the interface: without consent nothing is sent, the
//! key never leaves the backend, and what was shown is exactly what goes out.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, Emitter, State};
use tronic_ai::context::{drc_counts, erc_counts};
use tronic_ai::proposal::{Change, Rejected};
use tronic_ai::provider::{cut_suspected, origin, LoadedModel};
use tronic_ai::{
    analyze, apply_changes, build_request, classify, extract_block, parse_changes, preview,
    AiProvider, ChatMessage, ChatRequest, CheckCounts, Consent, ConsentStore, Finding,
    HttpProvider, Locality, Preview, ProviderConfig, Sections, StopReason, Task,
};
use tronic_schematic::Schematic;

use crate::error::CommandError;
use crate::{locked, AppState};

const CONFIG_KEY: &str = "ai_provider";
/// Overrides the entry name in the key store, so that tests stay apart.
const KEYRING_VARIABLE: &str = "TRONICCAD_KEYRING_SERVICE";
const KEYRING_SERVICE: &str = "TronicCAD";
/// A guard against pasting a whole book by accident. Nothing is shortened silently.
const MAX_REQUEST_CHARS: usize = 2_000_000;

struct Pending {
    id: u64,
    request: ChatRequest,
    config: ProviderConfig,
    project: String,
    doc: Schematic,
}

/// What the assistant holds between commands. None of it is saved.
#[derive(Default)]
pub(crate) struct AiState {
    consents: Mutex<ConsentStore>,
    pending: Mutex<Option<Pending>>,
    proposal: Mutex<Option<(Schematic, Vec<Change>)>>,
    cancel: AtomicBool,
    next_id: AtomicU64,
}

impl AiState {
    /// Consent and open requests end with the project they belong to.
    pub(crate) fn reset(&self) {
        if let Ok(mut consents) = self.consents.lock() {
            consents.clear();
        }
        if let Ok(mut pending) = self.pending.lock() {
            *pending = None;
        }
        if let Ok(mut proposal) = self.proposal.lock() {
            *proposal = None;
        }
    }
}

fn keyring_entry(config: &ProviderConfig) -> Result<keyring::Entry, CommandError> {
    let service = std::env::var(KEYRING_VARIABLE).unwrap_or_else(|_| KEYRING_SERVICE.to_owned());
    let user = origin(&config.base_url)?;
    keyring::Entry::new(&service, &user)
        .map_err(|error| CommandError::new("ai.keyStore").with("detail", error))
}

fn stored_key(config: &ProviderConfig) -> Result<Option<String>, CommandError> {
    match keyring_entry(config)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(CommandError::new("ai.keyStore").with("detail", error)),
    }
}

fn load_config(state: &AppState) -> Result<Option<ProviderConfig>, CommandError> {
    let stored = locked(&state.data)?.setting(CONFIG_KEY)?;
    Ok(stored.and_then(|text| serde_json::from_str(&text).ok()))
}

fn required_config(state: &AppState) -> Result<ProviderConfig, CommandError> {
    load_config(state)?.ok_or_else(|| CommandError::new("ai.noProvider"))
}

#[derive(Serialize)]
pub struct AiSettings {
    config: Option<ProviderConfig>,
    locality: Option<Locality>,
    has_key: bool,
    /// False if the key store cannot be read on this system.
    key_store_available: bool,
}

fn settings_of(config: Option<ProviderConfig>) -> AiSettings {
    let locality = config.as_ref().and_then(|c| classify(&c.base_url).ok());
    let key = config.as_ref().map(stored_key);
    AiSettings {
        has_key: matches!(key, Some(Ok(Some(_)))),
        key_store_available: !matches!(key, Some(Err(_))),
        config,
        locality,
    }
}

#[tauri::command(async)]
pub fn ai_settings(state: State<'_, AppState>) -> Result<AiSettings, CommandError> {
    Ok(settings_of(load_config(&state)?))
}

/// None means the assistant works with its rules only.
#[tauri::command(async)]
pub fn ai_set_provider(
    state: State<'_, AppState>,
    config: Option<ProviderConfig>,
) -> Result<AiSettings, CommandError> {
    let text = match &config {
        Some(config) => {
            classify(&config.base_url)?;
            if config.model.trim().is_empty() {
                return Err(tronic_ai::AiError::NoModel.into());
            }
            serde_json::to_string(config)
                .map_err(|error| CommandError::new("internal").with("detail", error))?
        }
        None => String::new(),
    };
    locked(&state.data)?.set_setting(CONFIG_KEY, &text)?;
    Ok(settings_of(config))
}

/// Keeps the key in the key store of the operating system. There is no way back
/// to the interface, and no file or database ever holds it.
#[tauri::command(async)]
pub fn ai_set_key(state: State<'_, AppState>, key: String) -> Result<AiSettings, CommandError> {
    let config = required_config(&state)?;
    let key = key.trim();
    if key.is_empty() {
        return Err(CommandError::new("ai.emptyKey"));
    }
    keyring_entry(&config)?
        .set_password(key)
        .map_err(|error| CommandError::new("ai.keyStore").with("detail", error))?;
    Ok(settings_of(Some(config)))
}

#[tauri::command(async)]
pub fn ai_delete_key(state: State<'_, AppState>) -> Result<AiSettings, CommandError> {
    let config = required_config(&state)?;
    match keyring_entry(&config)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(CommandError::new("ai.keyStore").with("detail", error)),
    }
    Ok(settings_of(Some(config)))
}

#[derive(Serialize)]
pub struct AiStatus {
    reachable: Option<bool>,
    error: Option<CommandError>,
    models: Vec<String>,
    loaded: Vec<LoadedModel>,
    memory_total: u64,
    memory_available: u64,
}

/// Asks the provider what it offers. Contains no project data, but it is a
/// connection, so it only runs when the user asks for it.
#[tauri::command(async)]
pub fn ai_status(state: State<'_, AppState>) -> Result<AiStatus, CommandError> {
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let mut status = AiStatus {
        reachable: None,
        error: None,
        models: Vec::new(),
        loaded: Vec::new(),
        memory_total: system.total_memory(),
        memory_available: system.available_memory(),
    };
    if let Some(config) = load_config(&state)? {
        let key = stored_key(&config)?;
        let provider = HttpProvider::new(config, key)?;
        match provider.models() {
            Ok(models) => {
                status.reachable = Some(true);
                status.models = models;
                status.loaded = provider.loaded_models();
            }
            Err(error) => {
                status.reachable = Some(false);
                status.error = Some(error.into());
            }
        }
    }
    Ok(status)
}

#[derive(Serialize)]
pub struct AiChecks {
    erc: CheckCounts,
    drc: CheckCounts,
}

#[derive(Serialize)]
pub struct AiAnalysis {
    findings: Vec<Finding>,
    checks: AiChecks,
}

/// The rules and the real checks. Needs neither a model nor a network.
#[tauri::command(async)]
pub fn ai_analysis(state: State<'_, AppState>) -> Result<AiAnalysis, CommandError> {
    state.with_session(|session| {
        Ok(AiAnalysis {
            findings: analyze(session.schematic(), session.board()),
            checks: AiChecks {
                erc: erc_counts(session.schematic()),
                drc: drc_counts(session.board()),
            },
        })
    })
}

#[derive(Deserialize)]
pub struct AiAsk {
    task: Task,
    text: String,
    document: String,
    sections: Sections,
    language: String,
    history: Vec<ChatMessage>,
}

#[derive(Serialize)]
pub struct AiPrepared {
    id: u64,
    /// Exactly what would be sent.
    request: ChatRequest,
    locality: Locality,
    origin: String,
    model: String,
    /// True if the backend will refuse to send without an answer from the user.
    needs_consent: bool,
}

/// Builds the request and holds it. Nothing has left the computer at this point.
#[tauri::command(async)]
pub fn ai_prepare(state: State<'_, AppState>, ask: AiAsk) -> Result<AiPrepared, CommandError> {
    let config = required_config(&state)?;
    let locality = classify(&config.base_url)?;
    let provider_origin = origin(&config.base_url)?;
    let (request, project, doc) = state.with_session(|session| {
        let request = build_request(
            ask.task,
            &ask.text,
            &ask.document,
            ask.sections,
            &ask.language,
            &ask.history,
            session.schematic(),
            session.board(),
        );
        Ok((
            request,
            session.project().id.to_string(),
            session.schematic().clone(),
        ))
    })?;
    let size: usize = request.system.chars().count()
        + request
            .messages
            .iter()
            .map(|message| message.content.chars().count())
            .sum::<usize>();
    if size > MAX_REQUEST_CHARS {
        return Err(CommandError::new("ai.tooLarge").with("limit", MAX_REQUEST_CHARS));
    }
    let needs_consent = locality == Locality::External
        && !locked(&state.ai.consents)?.has(&project, &provider_origin);
    let id = state.ai.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    *locked(&state.ai.pending)? = Some(Pending {
        id,
        request: request.clone(),
        config: config.clone(),
        project,
        doc,
    });
    Ok(AiPrepared {
        id,
        request,
        locality,
        origin: provider_origin,
        model: config.model,
        needs_consent,
    })
}

#[derive(Clone, Serialize)]
struct AiText {
    id: u64,
    text: String,
}

#[derive(Serialize)]
pub struct AiReply {
    id: u64,
    text: String,
    stop: StopReason,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    seconds: f64,
    /// The server may have dropped part of the request without saying so.
    possibly_cut: bool,
    context_length: Option<u64>,
    proposal: Option<Preview>,
}

/// Sends the prepared request, if the user allows it.
#[tauri::command(async)]
pub fn ai_send(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    consent: Consent,
) -> Result<AiReply, CommandError> {
    let current_project = state.with_session(|session| Ok(session.project().id.to_string()))?;
    let pending = {
        let mut slot = locked(&state.ai.pending)?;
        let matches = slot
            .as_ref()
            .is_some_and(|p| p.id == id && p.project == current_project);
        if !matches {
            return Err(CommandError::new("ai.noRequest"));
        }
        let config = &slot
            .as_ref()
            .ok_or_else(|| CommandError::new("ai.noRequest"))?
            .config;
        let locality = classify(&config.base_url)?;
        let provider_origin = origin(&config.base_url)?;
        // The request stays held if this refuses, so that the user can still answer.
        locked(&state.ai.consents)?.authorize(
            locality,
            &current_project,
            &provider_origin,
            consent,
        )?;
        slot.take()
            .ok_or_else(|| CommandError::new("ai.noRequest"))?
    };
    let key = stored_key(&pending.config)?;
    let model_name = pending.config.model.clone();
    let provider = HttpProvider::new(pending.config, key)?;
    state.ai.cancel.store(false, Ordering::Relaxed);
    let started = Instant::now();
    let mut on_text = |text: &str| {
        let _ = app.emit(
            "ai-text",
            AiText {
                id,
                text: text.to_owned(),
            },
        );
    };
    let outcome = provider.chat(&pending.request, &state.ai.cancel, &mut on_text)?;
    let seconds = started.elapsed().as_secs_f64();
    let sent_chars = pending.request.system.chars().count()
        + pending
            .request
            .messages
            .iter()
            .map(|message| message.content.chars().count())
            .sum::<usize>();
    let context_length = provider
        .loaded_models()
        .into_iter()
        .find(|model| model.name == model_name)
        .and_then(|model| model.context_length);
    let possibly_cut = cut_suspected(sent_chars, outcome.input_tokens, context_length);

    let (text, block) = extract_block(&outcome.text);
    let proposal = block.map(|block| {
        let (changes, rejected) = parse_changes(&block);
        let shown = preview(&pending.doc, changes, rejected);
        if let Ok(mut slot) = state.ai.proposal.lock() {
            *slot = Some((pending.doc.clone(), shown.changes.clone()));
        }
        shown
    });
    Ok(AiReply {
        id,
        text,
        stop: outcome.stop,
        input_tokens: outcome.input_tokens,
        output_tokens: outcome.output_tokens,
        seconds,
        possibly_cut,
        context_length,
        proposal,
    })
}

#[tauri::command(async)]
pub fn ai_cancel(state: State<'_, AppState>) {
    state.ai.cancel.store(true, Ordering::Relaxed);
}

#[derive(Serialize)]
pub struct AiApplied {
    dirty: bool,
    /// Chosen changes that depended on one that was left out.
    skipped: Vec<Rejected>,
}

/// Applies the chosen changes as one undo step, if the schematic has not changed
/// since the proposal was made.
#[tauri::command(async)]
pub fn ai_apply(
    state: State<'_, AppState>,
    indices: Vec<usize>,
) -> Result<AiApplied, CommandError> {
    let (basis, changes) = locked(&state.ai.proposal)?
        .take()
        .ok_or_else(|| CommandError::new("ai.noProposal"))?;
    let chosen: Vec<Change> = changes
        .iter()
        .enumerate()
        .filter(|(index, _)| indices.contains(index))
        .map(|(_, change)| change.clone())
        .collect();
    state.with_session(|session| {
        if *session.schematic() != basis {
            return Err(CommandError::new("ai.stale"));
        }
        let (after, skipped) = apply_changes(&basis, &chosen);
        session.commit_schematic(after);
        Ok(AiApplied {
            dirty: session.is_dirty(),
            skipped,
        })
    })
}

#[tauri::command(async)]
pub fn ai_discard(state: State<'_, AppState>) -> Result<(), CommandError> {
    *locked(&state.ai.proposal)? = None;
    Ok(())
}
