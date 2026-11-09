//! Errors as seen by the user interface.
//!
//! The backend never sends display text. It sends a stable code plus
//! parameters, and the interface translates them into the active language.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use tronic_3d::ImportError;
use tronic_ai::AiError;
use tronic_appdata::AppDataError;
use tronic_core::ValidationError;
use tronic_library::LibraryError;
use tronic_pcb::PcbError;
use tronic_project::ProjectError;
use tronic_router::RouteError;
use tronic_schematic::EditError;
use tronic_sim::SimError;
use tronic_storage::StorageError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub params: BTreeMap<&'static str, String>,
}

impl CommandError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
        }
    }

    pub fn invalid_library(detail: &str) -> Self {
        Self::new("library.invalidSymbol").with("detail", detail)
    }

    pub(crate) fn with(mut self, key: &'static str, value: impl ToString) -> Self {
        self.params.insert(key, value.to_string());
        self
    }

    fn with_path(self, path: &Path) -> Self {
        self.with("path", path.display())
    }
}

impl From<ValidationError> for CommandError {
    fn from(error: ValidationError) -> Self {
        match error {
            ValidationError::NameEmpty => Self::new("validation.nameEmpty"),
            ValidationError::NameTooLong { max } => {
                Self::new("validation.nameTooLong").with("max", max)
            }
            ValidationError::NameInvalidCharacters => Self::new("validation.nameInvalid"),
            ValidationError::DescriptionTooLong { max } => {
                Self::new("validation.descriptionTooLong").with("max", max)
            }
        }
    }
}

impl From<ProjectError> for CommandError {
    fn from(error: ProjectError) -> Self {
        match error {
            ProjectError::Validation(inner) => inner.into(),
            ProjectError::Io { path, source } => Self::new("project.io")
                .with_path(&path)
                .with("detail", source),
            ProjectError::NotAProject { path } => Self::new("project.notAProject").with_path(&path),
            ProjectError::UnsupportedVersion {
                path,
                found,
                supported,
            } => Self::new("project.unsupportedVersion")
                .with_path(&path)
                .with("found", found)
                .with("supported", supported),
            ProjectError::ChecksumMismatch { path } => {
                Self::new("project.checksumMismatch").with_path(&path)
            }
            ProjectError::TooLarge { path, limit } => Self::new("project.tooLarge")
                .with_path(&path)
                .with("limit", limit),
            ProjectError::AlreadyExists { path } => {
                Self::new("project.alreadyExists").with_path(&path)
            }
            ProjectError::Conflict { path } => Self::new("project.conflict").with_path(&path),
            ProjectError::InvalidContent { path, detail } => Self::new("project.invalidContent")
                .with_path(&path)
                .with("detail", detail),
        }
    }
}

impl From<EditError> for CommandError {
    fn from(error: EditError) -> Self {
        match error {
            EditError::UnknownSheet => Self::new("schematic.unknownSheet"),
            EditError::UnknownElement => Self::new("schematic.unknownElement"),
            EditError::UnknownSymbol(symbol) => {
                Self::new("schematic.unknownSymbol").with("symbol", symbol)
            }
            EditError::NotOrthogonal => Self::new("schematic.notOrthogonal"),
            EditError::EmptyText => Self::new("schematic.emptyText"),
            EditError::InvalidText => Self::new("schematic.invalidText"),
            EditError::LastSheet => Self::new("schematic.lastSheet"),
            EditError::SheetHasBlock => Self::new("schematic.sheetHasBlock"),
            EditError::DuplicateSheetName => Self::new("schematic.duplicateSheetName"),
            EditError::NothingToRestore => Self::new("schematic.nothingToRestore"),
        }
    }
}

impl From<SimError> for CommandError {
    fn from(error: SimError) -> Self {
        match error {
            SimError::NoGround => Self::new("sim.noGround"),
            SimError::Empty => Self::new("sim.empty"),
            SimError::TooLarge => Self::new("sim.tooLarge"),
            SimError::NoConvergence => Self::new("sim.noConvergence"),
            SimError::Singular => Self::new("sim.singular"),
            SimError::InvalidSettings => Self::new("sim.invalidSettings"),
            SimError::UnknownSource(source) => {
                Self::new("sim.unknownSource").with("source", source)
            }
        }
    }
}

impl From<ImportError> for CommandError {
    fn from(error: ImportError) -> Self {
        match error {
            ImportError::UnsupportedFormat => Self::new("model.unsupportedFormat"),
            ImportError::StepNotSupported => Self::new("model.stepNotSupported"),
            ImportError::Invalid => Self::new("model.invalid"),
            ImportError::Empty => Self::new("model.empty"),
            ImportError::TooLarge => Self::new("model.tooLarge"),
        }
    }
}

impl From<AiError> for CommandError {
    fn from(error: AiError) -> Self {
        match error {
            AiError::InvalidUrl => Self::new("ai.invalidUrl"),
            AiError::InsecureUrl => Self::new("ai.insecureUrl"),
            AiError::NoModel => Self::new("ai.noModel"),
            AiError::ConsentRequired => Self::new("ai.consentRequired"),
            AiError::Unreachable(detail) => Self::new("ai.unreachable").with("detail", detail),
            AiError::Unauthorized => Self::new("ai.unauthorized"),
            AiError::NotFound => Self::new("ai.notFound"),
            AiError::RateLimited => Self::new("ai.rateLimited"),
            AiError::Http { status, message } => Self::new("ai.http")
                .with("status", status)
                .with("detail", message),
            AiError::BadResponse => Self::new("ai.badResponse"),
        }
    }
}

impl From<StorageError> for CommandError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Unreachable(detail) => {
                Self::new("storage.unreachable").with("detail", detail)
            }
            StorageError::Unauthorized => Self::new("storage.unauthorized"),
            StorageError::Forbidden => Self::new("storage.forbidden"),
            StorageError::QuotaExceeded => Self::new("storage.quota"),
            // A version refused because someone else holds the lock on an area.
            StorageError::Server {
                status: 423,
                message,
            } => {
                let answer: serde_json::Value = serde_json::from_str(&message).unwrap_or_default();
                Self::new("team.areaLocked")
                    .with("area", answer["area"].as_str().unwrap_or_default())
                    .with("holder", answer["holder"].as_str().unwrap_or_default())
            }
            StorageError::Server { status, message } => Self::new("storage.server")
                .with("status", status)
                .with("detail", message),
            StorageError::InvalidUrl => Self::new("storage.invalidUrl"),
            StorageError::InsecureUrl => Self::new("storage.insecureUrl"),
            StorageError::InvalidKey => Self::new("storage.invalidKey"),
            StorageError::Io(detail) => Self::new("storage.io").with("detail", detail),
            StorageError::Corrupt => Self::new("storage.corrupt"),
            StorageError::BadResponse => Self::new("storage.badResponse"),
        }
    }
}

impl From<RouteError> for CommandError {
    fn from(error: RouteError) -> Self {
        match error {
            RouteError::NoOutline => Self::new("pcb.noOutline"),
            RouteError::BoardTooLarge => Self::new("pcb.boardTooLarge"),
        }
    }
}

impl From<PcbError> for CommandError {
    fn from(error: PcbError) -> Self {
        match error {
            PcbError::UnsupportedLayerCount(count) => {
                Self::new("pcb.unsupportedLayerCount").with("count", count)
            }
            PcbError::UnknownLayer(layer) => Self::new("pcb.unknownLayer").with("layer", layer + 1),
            PcbError::LayerInUse(layer) => Self::new("pcb.layerInUse").with("layer", layer + 1),
            PcbError::NoNet => Self::new("pcb.noNet"),
            PcbError::InvalidSize => Self::new("pcb.invalidSize"),
            PcbError::SingleLayerBoard => Self::new("pcb.singleLayerBoard"),
            PcbError::InvalidPolygon => Self::new("pcb.invalidPolygon"),
            PcbError::UnknownFootprint => Self::new("pcb.unknownFootprint"),
            PcbError::NothingToRestore => Self::new("schematic.nothingToRestore"),
        }
    }
}

impl From<LibraryError> for CommandError {
    fn from(error: LibraryError) -> Self {
        match error {
            LibraryError::NotALibrary => Self::new("library.notALibrary"),
            LibraryError::UnsupportedVersion { found, supported } => {
                Self::new("library.unsupportedVersion")
                    .with("found", found)
                    .with("supported", supported)
            }
            LibraryError::InvalidSymbol(detail) => Self::invalid_library(&detail),
        }
    }
}

impl From<AppDataError> for CommandError {
    fn from(error: AppDataError) -> Self {
        Self::new("appdata.failed").with("detail", error)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn conflict_carries_code_and_path() {
        let error: CommandError = ProjectError::Conflict {
            path: PathBuf::from("a.tronic"),
        }
        .into();
        assert_eq!(error.code, "project.conflict");
        assert_eq!(error.params["path"], "a.tronic");
    }

    #[test]
    fn validation_errors_keep_their_own_code_through_project_error() {
        let error: CommandError =
            ProjectError::Validation(ValidationError::NameTooLong { max: 5 }).into();
        assert_eq!(error.code, "validation.nameTooLong");
        assert_eq!(error.params["max"], "5");
    }

    #[test]
    fn serializes_for_the_interface() {
        let json = serde_json::to_value(CommandError::new("project.none")).unwrap();
        assert_eq!(json["code"], "project.none");
        assert!(json["params"].as_object().unwrap().is_empty());
    }
}
