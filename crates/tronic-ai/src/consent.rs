//! Whether project data may leave this computer.
//!
//! The check belongs to the backend. A mistake in the interface must not be able
//! to send anything: without consent, the request is never built into a call.

use serde::Deserialize;
use std::collections::BTreeSet;

use crate::provider::{AiError, Locality};

/// What the user answered to the consent question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    /// No answer given.
    None,
    /// This one request.
    Once,
    /// This project and this provider, until the project is closed.
    Project,
}

/// Consent lasts as long as the open project and is never written to disk.
#[derive(Debug, Default)]
pub struct ConsentStore {
    granted: BTreeSet<(String, String)>,
}

impl ConsentStore {
    pub fn has(&self, project: &str, origin: &str) -> bool {
        self.granted
            .contains(&(project.to_owned(), origin.to_owned()))
    }

    /// Decides whether a request may go out, and remembers lasting consent.
    pub fn authorize(
        &mut self,
        locality: Locality,
        project: &str,
        origin: &str,
        consent: Consent,
    ) -> Result<(), AiError> {
        if locality == Locality::Local {
            return Ok(());
        }
        match consent {
            Consent::Project => {
                self.granted.insert((project.to_owned(), origin.to_owned()));
                Ok(())
            }
            Consent::Once => Ok(()),
            Consent::None if self.has(project, origin) => Ok(()),
            Consent::None => Err(AiError::ConsentRequired),
        }
    }

    pub fn clear(&mut self) {
        self.granted.clear();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const P: &str = "project";
    const O: &str = "https://a.example";

    #[test]
    fn local_needs_no_consent() {
        let mut store = ConsentStore::default();
        store
            .authorize(Locality::Local, P, "http://127.0.0.1:1", Consent::None)
            .unwrap();
    }

    #[test]
    fn external_without_consent_is_refused() {
        let mut store = ConsentStore::default();
        assert_eq!(
            store.authorize(Locality::External, P, O, Consent::None),
            Err(AiError::ConsentRequired)
        );
    }

    #[test]
    fn consent_for_one_request_does_not_last() {
        let mut store = ConsentStore::default();
        store
            .authorize(Locality::External, P, O, Consent::Once)
            .unwrap();
        assert_eq!(
            store.authorize(Locality::External, P, O, Consent::None),
            Err(AiError::ConsentRequired)
        );
    }

    #[test]
    fn project_consent_binds_project_and_provider() {
        let mut store = ConsentStore::default();
        store
            .authorize(Locality::External, P, O, Consent::Project)
            .unwrap();
        store
            .authorize(Locality::External, P, O, Consent::None)
            .unwrap();
        assert_eq!(
            store.authorize(Locality::External, "other", O, Consent::None),
            Err(AiError::ConsentRequired)
        );
        assert_eq!(
            store.authorize(Locality::External, P, "https://b.example", Consent::None),
            Err(AiError::ConsentRequired)
        );
    }

    #[test]
    fn clearing_ends_consent() {
        let mut store = ConsentStore::default();
        store
            .authorize(Locality::External, P, O, Consent::Project)
            .unwrap();
        store.clear();
        assert!(!store.has(P, O));
    }
}
