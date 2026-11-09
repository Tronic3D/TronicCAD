//! Versioned synchronization on storage that only needs to hold objects by name.
//!
//! Each saved state is an immutable commit naming its parents and a content blob,
//! both addressed by their SHA-256. Nothing is ever overwritten or deleted, and no
//! locks or conditional writes are needed: two devices writing at the same moment
//! simply produce two commits without children, which the next sync reports as a
//! conflict. Interrupting at any point leaves unused objects, never a wrong state.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::provider::{StorageError, StorageProvider};
use crate::sha256_hex;

/// One saved state of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub parents: Vec<String>,
    /// SHA-256 of the project file.
    pub blob: String,
    pub size: u64,
    /// Which device wrote this, as the user named it.
    pub device: String,
    pub created: String,
}

/// What this copy of a project remembers about its link. Kept beside the settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LinkState {
    /// The commit the local file was last the same as.
    pub base: Option<String>,
    /// Commits read so far. They are immutable, so they need no second request.
    pub known: BTreeMap<String, Commit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    /// Nothing to do.
    UpToDate { commit: String },
    /// The local state was stored as a new version.
    Pushed { commit: String },
    /// The remote state is newer, and these are its verified bytes. The caller
    /// replaces the local file, and only then keeps the new state.
    Pulled {
        commit: String,
        #[serde(skip)]
        bytes: Vec<u8>,
    },
    /// Local and remote both changed, or two devices wrote at once. Nothing was
    /// changed anywhere; the user decides.
    Conflict {
        heads: Vec<String>,
        local_changed: bool,
    },
}

/// Versions newest first, for the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Version {
    pub id: String,
    pub commit: Commit,
    pub is_head: bool,
}

/// How often a copy must read wrong before it counts as damaged and may be replaced.
const READS_BEFORE_DAMAGED: usize = 4;

/// One project on one provider.
pub struct Repo<'a> {
    provider: &'a dyn StorageProvider,
    prefix: String,
}

impl<'a> Repo<'a> {
    pub fn new(provider: &'a dyn StorageProvider, project_id: &str) -> Self {
        Self {
            provider,
            prefix: format!("projects/{project_id}"),
        }
    }

    fn commit_key(&self, id: &str) -> String {
        format!("{}/commits/{id}.json", self.prefix)
    }

    fn blob_key(&self, hash: &str) -> String {
        format!("{}/blobs/{hash}", self.prefix)
    }

    /// Brings the known commits up to date. A commit whose content does not match
    /// its name is an interrupted or damaged write and does not exist for the sync.
    pub fn refresh(&self, state: &mut LinkState) -> Result<(), StorageError> {
        let commits_prefix = format!("{}/commits", self.prefix);
        for key in self.provider.list(&commits_prefix)? {
            let Some(id) = key
                .strip_prefix(&format!("{commits_prefix}/"))
                .and_then(|name| name.strip_suffix(".json"))
            else {
                continue;
            };
            if state.known.contains_key(id) {
                continue;
            }
            let Some(bytes) = self.provider.get(&key)? else {
                continue;
            };
            if sha256_hex(&bytes) != id {
                continue;
            }
            if let Ok(commit) = serde_json::from_slice::<Commit>(&bytes) {
                state.known.insert(id.to_owned(), commit);
            }
        }
        Ok(())
    }

    /// Commits no other commit builds on.
    pub fn heads(state: &LinkState) -> Vec<String> {
        let referenced: BTreeSet<&String> = state
            .known
            .values()
            .flat_map(|commit| commit.parents.iter())
            .collect();
        state
            .known
            .keys()
            .filter(|id| !referenced.contains(id))
            .cloned()
            .collect()
    }

    fn descends_from(state: &LinkState, commit: &str, ancestor: &str) -> bool {
        let mut stack = vec![commit];
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            if id == ancestor {
                return true;
            }
            if seen.insert(id) {
                if let Some(found) = state.known.get(id) {
                    stack.extend(found.parents.iter().map(String::as_str));
                }
            }
        }
        false
    }

    /// The newest state two commits share, if any.
    pub fn merge_base(state: &LinkState, a: &str, b: &str) -> Option<String> {
        let ancestors = |start: &str| {
            let mut seen = BTreeSet::new();
            let mut stack = vec![start.to_owned()];
            while let Some(id) = stack.pop() {
                if seen.insert(id.clone()) {
                    if let Some(commit) = state.known.get(&id) {
                        stack.extend(commit.parents.iter().cloned());
                    }
                }
            }
            seen
        };
        let (first, second) = (ancestors(a), ancestors(b));
        first
            .intersection(&second)
            .filter_map(|id| {
                state
                    .known
                    .get(id)
                    .map(|commit| (commit.created.clone(), id.clone()))
            })
            .max()
            .map(|(_, id)| id)
    }

    /// Whether a good copy is there. A read can break off and look like damage, so
    /// a copy counts as damaged only when it reads wrong every time.
    fn has_intact(&self, key: &str, hash: &str) -> Result<bool, StorageError> {
        for _ in 0..READS_BEFORE_DAMAGED {
            match self.provider.get(key)? {
                None => return Ok(false),
                Some(found) if sha256_hex(&found) == hash => return Ok(true),
                Some(_) => {}
            }
        }
        Ok(false)
    }

    /// Stores a state. The blob is read back and checked before any commit names it,
    /// so a broken transfer cannot produce a version that looks fine.
    fn push(
        &self,
        local: &[u8],
        parents: Vec<String>,
        device: &str,
        state: &mut LinkState,
    ) -> Result<String, StorageError> {
        let hash = sha256_hex(local);
        let key = self.blob_key(&hash);
        // A good copy stays untouched. Writing it again could break off midway and
        // replace the only copy of this state with half of it.
        if !self.has_intact(&key, &hash)? {
            self.provider.put(&key, local)?;
        }
        let stored = self.provider.get(&key)?.ok_or(StorageError::Corrupt)?;
        if sha256_hex(&stored) != hash {
            return Err(StorageError::Corrupt);
        }
        let commit = Commit {
            parents,
            blob: hash,
            size: local.len() as u64,
            device: device.to_owned(),
            created: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|_| StorageError::BadResponse)?,
        };
        let bytes = serde_json::to_vec(&commit).map_err(|_| StorageError::BadResponse)?;
        let id = sha256_hex(&bytes);
        self.provider.put(&self.commit_key(&id), &bytes)?;
        state.known.insert(id.clone(), commit);
        state.base = Some(id.clone());
        Ok(id)
    }

    /// Reads a stored state and checks it against its checksum.
    pub fn read(&self, state: &LinkState, commit: &str) -> Result<Vec<u8>, StorageError> {
        let found = state.known.get(commit).ok_or(StorageError::Corrupt)?;
        let bytes = self
            .provider
            .get(&self.blob_key(&found.blob))?
            .ok_or(StorageError::Corrupt)?;
        if sha256_hex(&bytes) != found.blob {
            return Err(StorageError::Corrupt);
        }
        Ok(bytes)
    }

    /// Compares the saved local file with the storage and does what is safe.
    /// `state` changes in memory: keep it after the local file is in place.
    pub fn sync(
        &self,
        local: &[u8],
        device: &str,
        state: &mut LinkState,
    ) -> Result<Outcome, StorageError> {
        self.refresh(state)?;
        let local_hash = sha256_hex(local);
        let heads = Self::heads(state);
        let base_blob = state
            .base
            .as_ref()
            .and_then(|base| state.known.get(base))
            .map(|commit| commit.blob.clone());
        let local_changed = base_blob.as_deref() != Some(local_hash.as_str());

        if heads.is_empty() {
            let commit = self.push(local, Vec::new(), device, state)?;
            return Ok(Outcome::Pushed { commit });
        }

        // All heads hold what is here: an earlier run was interrupted after writing,
        // or a pull was interrupted after replacing the file. Nothing is lost.
        let same: Vec<&String> = heads
            .iter()
            .filter(|id| state.known.get(*id).is_some_and(|c| c.blob == local_hash))
            .collect();
        if same.len() == heads.len() {
            let commit = (*same.first().ok_or(StorageError::Corrupt)?).clone();
            state.base = Some(commit.clone());
            return Ok(Outcome::UpToDate { commit });
        }

        // Heads with the same content are one state written twice, not a conflict.
        let contents: BTreeSet<&String> = heads
            .iter()
            .filter_map(|id| state.known.get(id).map(|c| &c.blob))
            .collect();
        let base_is_head = heads.len() == 1 && state.base.as_ref() == heads.first();

        if contents.len() == 1 {
            let head = heads.first().ok_or(StorageError::Corrupt)?.clone();
            if base_is_head {
                let commit = self.push(local, vec![head], device, state)?;
                return Ok(Outcome::Pushed { commit });
            }
            let moved_on = state
                .base
                .as_ref()
                .is_some_and(|base| Self::descends_from(state, &head, base));
            if moved_on && !local_changed {
                let bytes = self.read(state, &head)?;
                state.base = Some(head.clone());
                return Ok(Outcome::Pulled {
                    commit: head,
                    bytes,
                });
            }
        }
        Ok(Outcome::Conflict {
            heads,
            local_changed,
        })
    }

    /// Stores the local state as the result of a conflict, building on all heads, so
    /// that they all stay in the history.
    pub fn resolve_keep_local(
        &self,
        local: &[u8],
        device: &str,
        state: &mut LinkState,
    ) -> Result<String, StorageError> {
        self.refresh(state)?;
        self.push(local, Self::heads(state), device, state)
    }

    /// Takes one head as the result of a conflict. Returns its bytes; the caller
    /// keeps a copy of the local file before replacing it.
    pub fn resolve_take_remote(
        &self,
        head: &str,
        device: &str,
        state: &mut LinkState,
    ) -> Result<Vec<u8>, StorageError> {
        self.refresh(state)?;
        let heads = Self::heads(state);
        if !heads.iter().any(|id| id == head) {
            return Err(StorageError::Corrupt);
        }
        let bytes = self.read(state, head)?;
        if heads.len() > 1 {
            self.push(&bytes, heads, device, state)?;
        } else {
            state.base = Some(head.to_owned());
        }
        Ok(bytes)
    }

    /// A one-way backup: stores the local state if it is not the newest there. It
    /// never reads anything back into the project and has no conflicts.
    pub fn backup(
        &self,
        local: &[u8],
        device: &str,
        state: &mut LinkState,
    ) -> Result<Outcome, StorageError> {
        self.refresh(state)?;
        let hash = sha256_hex(local);
        let heads = Self::heads(state);
        if let Some(same) = heads
            .iter()
            .find(|id| state.known.get(*id).is_some_and(|c| c.blob == hash))
        {
            let commit = same.clone();
            state.base = Some(commit.clone());
            return Ok(Outcome::UpToDate { commit });
        }
        let commit = self.push(local, heads, device, state)?;
        Ok(Outcome::Pushed { commit })
    }

    pub fn versions(state: &LinkState) -> Vec<Version> {
        let heads = Self::heads(state);
        let mut out: Vec<Version> = state
            .known
            .iter()
            .map(|(id, commit)| Version {
                id: id.clone(),
                commit: commit.clone(),
                is_head: heads.contains(id),
            })
            .collect();
        out.sort_by(|a, b| b.commit.created.cmp(&a.commit.created));
        out
    }

    /// Bytes held by this project, counting each distinct content once.
    pub fn stored_bytes(state: &LinkState) -> u64 {
        let mut seen = BTreeSet::new();
        state
            .known
            .values()
            .filter(|commit| seen.insert(&commit.blob))
            .map(|commit| commit.size)
            .sum()
    }
}
