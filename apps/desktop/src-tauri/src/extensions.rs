//! Extension tools in the application. See `docs/EXTENSIONS.md`.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tronic_extensions::{
    discover, fingerprint, run, Answer, ExtensionError, Found, Message, API_VERSION,
};

use crate::error::CommandError;
use crate::{locked, AppState};

fn root(state: &AppState) -> Result<PathBuf, CommandError> {
    Ok(state
        .data_directory
        .as_ref()
        .ok_or_else(|| CommandError::new("extensions.noDataDirectory"))?
        .join("extensions"))
}

fn trust_key(id: &str) -> String {
    format!("extensions.trusted.{id}")
}

fn error_of(error: ExtensionError) -> CommandError {
    let code = match &error {
        ExtensionError::BadManifest(_) | ExtensionError::UnsupportedApi(_) => {
            "extensions.badManifest"
        }
        ExtensionError::CannotStart(_) => "extensions.cannotStart",
        ExtensionError::TimedOut => "extensions.timedOut",
        ExtensionError::TooMuchOutput => "extensions.tooMuchOutput",
        ExtensionError::Failed { .. } => "extensions.failed",
        ExtensionError::BadAnswer(_) | ExtensionError::BadFileName(_) => "extensions.badAnswer",
        ExtensionError::Io(_) => "extensions.io",
    };
    CommandError::new(code).with("detail", error)
}

#[derive(Serialize)]
pub struct ExtensionView {
    id: String,
    folder: String,
    name: String,
    version: String,
    description: String,
    author: String,
    license: String,
    command: String,
    fingerprint: String,
    trusted: bool,
    /// It was trusted, and its files have changed since.
    changed: bool,
    problem: Option<String>,
}

fn view(state: &AppState, found: Found) -> Result<ExtensionView, CommandError> {
    let folder = found
        .directory
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Ok(match found.manifest {
        Ok(manifest) => {
            let stored = locked(&state.data)?.setting(&trust_key(&manifest.id))?;
            let trusted = stored.as_deref() == Some(found.fingerprint.as_str())
                && !found.fingerprint.is_empty();
            ExtensionView {
                changed: stored.is_some() && !trusted,
                trusted,
                command: manifest.command.join(" "),
                id: manifest.id,
                folder,
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                author: manifest.author,
                license: manifest.license,
                fingerprint: found.fingerprint,
                problem: None,
            }
        }
        Err(problem) => ExtensionView {
            id: folder.clone(),
            name: folder.clone(),
            folder,
            version: String::new(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            command: String::new(),
            fingerprint: found.fingerprint,
            trusted: false,
            changed: false,
            problem: Some(problem),
        },
    })
}

#[derive(Serialize)]
pub struct ExtensionsOverview {
    folder: String,
    api: u32,
    extensions: Vec<ExtensionView>,
}

#[tauri::command(async)]
pub fn extensions_list(state: State<'_, AppState>) -> Result<ExtensionsOverview, CommandError> {
    let folder = root(&state)?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    let extensions = discover(&folder)
        .map_err(error_of)?
        .into_iter()
        .map(|found| view(&state, found))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExtensionsOverview {
        folder: folder.display().to_string(),
        api: API_VERSION,
        extensions,
    })
}

fn find(state: &AppState, id: &str) -> Result<Found, CommandError> {
    discover(&root(state)?)
        .map_err(error_of)?
        .into_iter()
        .find(|found| {
            found
                .manifest
                .as_ref()
                .is_ok_and(|manifest| manifest.id == id)
        })
        .ok_or_else(|| CommandError::new("extensions.notFound").with("id", id))
}

/// The fingerprint comes from what the user was shown. If the files changed in
/// between, this is refused instead of trusting something else.
#[tauri::command(async)]
pub fn extensions_trust(
    state: State<'_, AppState>,
    id: String,
    shown_fingerprint: String,
) -> Result<(), CommandError> {
    let found = find(&state, &id)?;
    if found.fingerprint.is_empty() || found.fingerprint != shown_fingerprint {
        return Err(CommandError::new("extensions.changed").with("id", id));
    }
    locked(&state.data)?.set_setting(&trust_key(&id), &found.fingerprint)?;
    Ok(())
}

#[tauri::command(async)]
pub fn extensions_untrust(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    locked(&state.data)?.set_setting(&trust_key(&id), "")?;
    Ok(())
}

#[derive(Serialize)]
pub struct RunResult {
    messages: Vec<Message>,
    folder: Option<String>,
    files: Vec<String>,
}

/// Where the files of an extension go: beside the project, never over anything else
/// than its own earlier results.
fn save_files(
    project: &std::path::Path,
    id: &str,
    answer: &Answer,
) -> Result<Option<PathBuf>, CommandError> {
    if answer.files.is_empty() {
        return Ok(None);
    }
    let stem = project.file_stem().map_or_else(
        || "project".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let directory = project
        .parent()
        .ok_or_else(|| CommandError::new("extensions.io").with("detail", "project folder"))?
        .join(format!("{stem}-extensions"))
        .join(id);
    std::fs::create_dir_all(&directory)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    for (name, content) in &answer.files {
        std::fs::write(directory.join(name), content)
            .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    }
    Ok(Some(directory))
}

#[tauri::command(async)]
pub fn extensions_run(state: State<'_, AppState>, id: String) -> Result<RunResult, CommandError> {
    let found = find(&state, &id)?;
    let manifest = found
        .manifest
        .clone()
        .map_err(|detail| CommandError::new("extensions.badManifest").with("detail", detail))?;
    // Checked here again, on the files as they are at this moment.
    let stored = locked(&state.data)?.setting(&trust_key(&id))?;
    let present = fingerprint(&found.directory).map_err(error_of)?;
    if stored.as_deref() != Some(present.as_str()) {
        return Err(CommandError::new("extensions.notTrusted").with("id", id));
    }
    let (input, project): (Value, PathBuf) = state.with_session(|session| {
        Ok((
            json!({
                "api": API_VERSION,
                "project": {
                    "name": session.project().name,
                    "description": session.project().description,
                },
                "schematic": session.schematic(),
                "board": session.board(),
                "netlist": session.netlist(),
            }),
            session.path().to_path_buf(),
        ))
    })?;
    let bytes = serde_json::to_vec(&input)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    let answer = run(&manifest, &found.directory, &bytes).map_err(error_of)?;
    let folder = save_files(&project, &id, &answer)?;
    Ok(RunResult {
        messages: answer.messages,
        folder: folder.map(|path| path.display().to_string()),
        files: answer.files.into_keys().collect(),
    })
}

#[tauri::command(async)]
pub fn extensions_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let folder = root(&state)?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    app.opener()
        .open_path(folder.display().to_string(), None::<&str>)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))
}

// What the program consists of: generated from the real dependency graph.
const COMPONENTS: &str = include_str!("../../../../sbom.cdx.json");
const NOTICES: &str = include_str!("../../../../THIRD_PARTY_NOTICES.md");
const OWN_LICENSE: &str = include_str!("../../../../LICENSE");

#[derive(Serialize)]
pub struct Component {
    name: String,
    version: String,
    license: String,
    author: String,
}

#[tauri::command(async)]
pub fn third_party_components() -> Result<Vec<Component>, CommandError> {
    let bom: Value = serde_json::from_str(COMPONENTS)
        .map_err(|error| CommandError::new("extensions.io").with("detail", error))?;
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    Ok(bom["components"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|component| Component {
                    name: text(&component["name"]),
                    version: text(&component["version"]),
                    license: text(&component["licenses"][0]["expression"]),
                    author: text(&component["author"]),
                })
                .collect()
        })
        .unwrap_or_default())
}

#[tauri::command(async)]
pub fn third_party_notices() -> &'static str {
    NOTICES
}

#[tauri::command(async)]
pub fn own_license() -> &'static str {
    OWN_LICENSE
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_list_of_components_is_there_and_complete() {
        let list = third_party_components().unwrap();
        assert!(list.len() > 100);
        assert!(list.iter().any(|component| component.name == "tauri"));
        assert!(list.iter().any(|component| component.name == "react"));
        assert!(list
            .iter()
            .all(|component| !component.name.is_empty() && !component.version.is_empty()));
    }

    #[test]
    fn the_full_texts_are_built_in() {
        assert!(third_party_notices().contains("NSIS"));
        assert!(own_license().contains("Apache License"));
    }

    #[test]
    fn results_of_an_extension_stay_beside_the_project() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("Board.tronic");
        let answer = Answer {
            messages: Vec::new(),
            files: [("list.csv".to_owned(), "a,b".to_owned())].into(),
        };
        let folder = save_files(&project, "demo", &answer).unwrap().unwrap();
        assert_eq!(
            folder,
            directory.path().join("Board-extensions").join("demo")
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("list.csv")).unwrap(),
            "a,b"
        );
        assert_eq!(
            save_files(&project, "demo", &Answer::default()).unwrap(),
            None
        );
    }
}
