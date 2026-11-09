//! TronicCAD domain model.
//!
//! This crate holds pure domain logic. It performs no I/O and knows nothing
//! about the user interface, storage or network, so that every other module
//! can depend on it without pulling those concerns along.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// Maximum project name length, counted in Unicode scalar values.
pub const MAX_NAME_CHARS: usize = 120;
/// Maximum project description length, counted in Unicode scalar values.
pub const MAX_DESCRIPTION_CHARS: usize = 4000;

/// A rule violated by user-supplied project metadata.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("project name must not be empty")]
    NameEmpty,
    #[error("project name exceeds {max} characters")]
    NameTooLong { max: usize },
    #[error("project name contains control characters")]
    NameInvalidCharacters,
    #[error("project description exceeds {max} characters")]
    DescriptionTooLong { max: usize },
}

/// A TronicCAD project.
///
/// Schematic and PCB content will be added here in later phases; each such
/// change bumps the file format version in `tronic-project`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub modified_at: OffsetDateTime,
}

impl Project {
    /// Creates a project with a fresh identity after validating the metadata.
    pub fn new(
        name: &str,
        description: &str,
        now: OffsetDateTime,
    ) -> Result<Self, ValidationError> {
        let (name, description) = normalize_metadata(name, description)?;
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            description,
            created_at: now,
            modified_at: now,
        })
    }

    /// Replaces name and description. On error the project stays unchanged.
    /// Returns whether anything actually changed.
    pub fn set_metadata(&mut self, name: &str, description: &str) -> Result<bool, ValidationError> {
        let (name, description) = normalize_metadata(name, description)?;
        let changed = name != self.name || description != self.description;
        self.name = name;
        self.description = description;
        Ok(changed)
    }

    /// Checks invariants of a project that was read from outside.
    pub fn validate(&self) -> Result<(), ValidationError> {
        normalize_metadata(&self.name, &self.description).map(|_| ())
    }
}

fn normalize_metadata(name: &str, description: &str) -> Result<(String, String), ValidationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ValidationError::NameEmpty);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ValidationError::NameTooLong {
            max: MAX_NAME_CHARS,
        });
    }
    if name.chars().any(char::is_control) {
        return Err(ValidationError::NameInvalidCharacters);
    }
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(ValidationError::DescriptionTooLong {
            max: MAX_DESCRIPTION_CHARS,
        });
    }
    Ok((name.to_owned(), description.to_owned()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn now() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }

    #[test]
    fn new_trims_the_name_and_sets_both_timestamps() {
        let p = Project::new("  Netzteil 5V  ", "Beschreibung", now()).unwrap();
        assert_eq!(p.name, "Netzteil 5V");
        assert_eq!(p.created_at, p.modified_at);
    }

    #[test]
    fn new_generates_distinct_ids() {
        let a = Project::new("A", "", now()).unwrap();
        let b = Project::new("A", "", now()).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn rejects_blank_names() {
        assert_eq!(
            Project::new(" \t ", "", now()),
            Err(ValidationError::NameEmpty)
        );
    }

    #[test]
    fn name_limit_counts_characters_not_bytes() {
        // Each CJK character takes three bytes in UTF-8.
        let at_limit = "电".repeat(MAX_NAME_CHARS);
        assert!(Project::new(&at_limit, "", now()).is_ok());
        let over = "电".repeat(MAX_NAME_CHARS + 1);
        assert_eq!(
            Project::new(&over, "", now()),
            Err(ValidationError::NameTooLong {
                max: MAX_NAME_CHARS
            })
        );
    }

    #[test]
    fn rejects_control_characters_inside_the_name() {
        assert_eq!(
            Project::new("a\nb", "", now()),
            Err(ValidationError::NameInvalidCharacters)
        );
    }

    #[test]
    fn rejects_overlong_description() {
        let long = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
        assert!(matches!(
            Project::new("A", &long, now()),
            Err(ValidationError::DescriptionTooLong { .. })
        ));
    }

    #[test]
    fn set_metadata_reports_changes_and_keeps_state_on_error() {
        let mut p = Project::new("A", "d", now()).unwrap();
        assert!(!p.set_metadata(" A ", "d").unwrap());
        assert!(p.set_metadata("B", "d").unwrap());
        assert!(p.set_metadata("", "x").is_err());
        assert_eq!(p.name, "B");
        assert_eq!(p.description, "d");
    }

    #[test]
    fn json_round_trip_preserves_unicode() {
        let p = Project::new("电源模块 Größe é", "说明", now()).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
    }
}
