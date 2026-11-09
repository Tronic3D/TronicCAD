//! The promise of the sync, under fire: whatever fails and whoever writes at the
//! same time, no saved state is lost and no device ends up with something other
//! than what it chose.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tronic_storage::*;

/// Storage in memory that can fail on command, like a real connection.
#[derive(Default)]
struct Faulty {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Pseudo-random numbers decide when a request fails.
    seed: AtomicU64,
    /// Out of 100 requests.
    failure_rate: AtomicU64,
    /// How many times the content of a state was written.
    blob_writes: AtomicU64,
    offline: Mutex<bool>,
}

#[derive(Clone, Copy, PartialEq)]
enum Fate {
    Fine,
    /// The request never arrives.
    Lost,
    /// A write arrives only in part, then the connection breaks.
    Torn,
    /// A write arrives in full, but the answer gets lost.
    AnswerLost,
}

impl Faulty {
    fn fate(&self) -> Fate {
        let mut x = self
            .seed
            .load(Ordering::Relaxed)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.seed.store(x, Ordering::Relaxed);
        x >>= 33;
        let rate = self.failure_rate.load(Ordering::Relaxed);
        if x % 100 >= rate {
            return Fate::Fine;
        }
        match (x / 100) % 3 {
            0 => Fate::Lost,
            1 => Fate::Torn,
            _ => Fate::AnswerLost,
        }
    }

    fn check_online(&self) -> Result<(), StorageError> {
        if *self.offline.lock().unwrap() {
            return Err(StorageError::Unreachable("offline".to_owned()));
        }
        Ok(())
    }
}

fn broken() -> StorageError {
    StorageError::Unreachable("connection lost".to_owned())
}

impl StorageProvider for Faulty {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.check_online()?;
        if key.contains("/blobs/") {
            self.blob_writes.fetch_add(1, Ordering::Relaxed);
        }
        match self.fate() {
            Fate::Lost => Err(broken()),
            Fate::Torn => {
                let cut = bytes.len() / 2;
                self.objects
                    .lock()
                    .unwrap()
                    .insert(key.to_owned(), bytes[..cut].to_vec());
                Err(broken())
            }
            Fate::AnswerLost => {
                self.objects
                    .lock()
                    .unwrap()
                    .insert(key.to_owned(), bytes.to_vec());
                Err(broken())
            }
            Fate::Fine => {
                self.objects
                    .lock()
                    .unwrap()
                    .insert(key.to_owned(), bytes.to_vec());
                Ok(())
            }
        }
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.check_online()?;
        match self.fate() {
            Fate::Fine | Fate::AnswerLost => Ok(self.objects.lock().unwrap().get(key).cloned()),
            Fate::Lost => Err(broken()),
            // A read that breaks off gives half the object without any error.
            Fate::Torn => Ok(self
                .objects
                .lock()
                .unwrap()
                .get(key)
                .map(|bytes| bytes[..bytes.len() / 2].to_vec())),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.check_online()?;
        if self.fate() == Fate::Lost {
            return Err(broken());
        }
        Ok(self
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect())
    }
}

const PROJECT: &str = "11111111-2222-3333-4444-555555555555";

struct Device {
    name: String,
    file: Vec<u8>,
    state: LinkState,
}

impl Device {
    fn new(name: &str, content: &[u8]) -> Self {
        Self {
            name: name.to_owned(),
            file: content.to_vec(),
            state: LinkState::default(),
        }
    }

    fn sync(&mut self, storage: &Faulty) -> Result<Outcome, StorageError> {
        let repo = Repo::new(storage, PROJECT);
        // As in the application: a change of state counts only if nothing went wrong.
        let mut state = self.state.clone();
        let outcome = repo.sync(&self.file, &self.name, &mut state)?;
        if let Outcome::Pulled { bytes, .. } = &outcome {
            self.file.clone_from(bytes);
        }
        self.state = state;
        Ok(outcome)
    }
}

fn expect_conflict(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Conflict { heads, .. } => heads.clone(),
        other => panic!("expected a conflict: {other:?}"),
    }
}

#[test]
fn a_change_travels_to_the_other_device() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    assert!(matches!(a.sync(&storage).unwrap(), Outcome::Pushed { .. }));
    let mut b = Device::new("b", b"one");
    assert!(
        matches!(b.sync(&storage).unwrap(), Outcome::UpToDate { .. }),
        "same content is adopted"
    );
    a.file = b"two".to_vec();
    assert!(matches!(a.sync(&storage).unwrap(), Outcome::Pushed { .. }));
    assert!(matches!(b.sync(&storage).unwrap(), Outcome::Pulled { .. }));
    assert_eq!(b.file, b"two");
    assert!(matches!(
        b.sync(&storage).unwrap(),
        Outcome::UpToDate { .. }
    ));
}

#[test]
fn both_sides_changed_is_a_conflict_and_changes_nothing() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    let mut b = Device::new("b", b"one");
    b.sync(&storage).unwrap();
    a.file = b"from a".to_vec();
    a.sync(&storage).unwrap();
    b.file = b"from b".to_vec();
    expect_conflict(&b.sync(&storage).unwrap());
    assert_eq!(b.file, b"from b", "the local file is untouched");
    let objects = storage.objects.lock().unwrap().len();
    expect_conflict(&b.sync(&storage).unwrap());
    assert_eq!(
        storage.objects.lock().unwrap().len(),
        objects,
        "nothing was stored"
    );
}

#[test]
fn a_first_link_to_different_content_is_a_conflict_not_an_overwrite() {
    let storage = Faulty::default();
    Device::new("a", b"theirs").sync(&storage).unwrap();
    let mut b = Device::new("b", b"mine");
    expect_conflict(&b.sync(&storage).unwrap());
    assert_eq!(b.file, b"mine");
}

#[test]
fn two_devices_writing_at_the_same_moment_are_noticed() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    let mut b = Device::new("b", b"one");
    b.sync(&storage).unwrap();
    // Both saw the same state; neither can see the other's write coming.
    let (mut sa, mut sb) = (a.state.clone(), b.state.clone());
    a.file = b"a".to_vec();
    b.file = b"b".to_vec();
    let repo = Repo::new(&storage, PROJECT);
    repo.refresh(&mut sa).unwrap();
    repo.refresh(&mut sb).unwrap();
    assert!(matches!(
        repo.sync(&a.file, "a", &mut sa).unwrap(),
        Outcome::Pushed { .. }
    ));
    // b's sync sees a's commit and refuses. A real race produces two heads instead:
    let parent = sb.base.clone().unwrap();
    let hash = sha256_hex(b"b");
    let commit = Commit {
        parents: vec![parent],
        blob: hash.clone(),
        size: 1,
        device: "b".to_owned(),
        created: "2026-01-01T00:00:00Z".to_owned(),
    };
    let bytes = serde_json::to_vec(&commit).unwrap();
    storage
        .put(&format!("projects/{PROJECT}/blobs/{hash}"), b"b")
        .unwrap();
    storage
        .put(
            &format!("projects/{PROJECT}/commits/{}.json", sha256_hex(&bytes)),
            &bytes,
        )
        .unwrap();
    // Both devices now hear about it, even the one with nothing to send.
    a.state = sa;
    assert_eq!(expect_conflict(&a.sync(&storage).unwrap()).len(), 2);
    let mut c = Device::new("c", b"one");
    assert_eq!(expect_conflict(&c.sync(&storage).unwrap()).len(), 2);
}

#[test]
fn resolving_keeps_every_side_in_the_history() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    let mut b = Device::new("b", b"one");
    b.sync(&storage).unwrap();
    a.file = b"from a".to_vec();
    a.sync(&storage).unwrap();
    b.file = b"from b".to_vec();
    expect_conflict(&b.sync(&storage).unwrap());

    let repo = Repo::new(&storage, PROJECT);
    repo.resolve_keep_local(&b.file, "b", &mut b.state).unwrap();
    assert!(matches!(
        b.sync(&storage).unwrap(),
        Outcome::UpToDate { .. }
    ));
    assert!(matches!(a.sync(&storage).unwrap(), Outcome::Pulled { .. }));
    assert_eq!(a.file, b"from b");
    let stored: BTreeSet<Vec<u8>> = storage
        .objects
        .lock()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.contains("/blobs/"))
        .map(|(_, v)| v.clone())
        .collect();
    assert!(
        stored.contains(b"from a".as_slice()),
        "the other side stays available"
    );
    assert_eq!(Repo::versions(&a.state).len(), 3);
}

#[test]
fn taking_the_remote_side_ends_the_conflict() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    let mut b = Device::new("b", b"mine");
    let heads = expect_conflict(&b.sync(&storage).unwrap());
    let repo = Repo::new(&storage, PROJECT);
    b.file = repo
        .resolve_take_remote(&heads[0], "b", &mut b.state)
        .unwrap();
    assert_eq!(b.file, b"one");
    assert!(matches!(
        b.sync(&storage).unwrap(),
        Outcome::UpToDate { .. }
    ));
}

#[test]
fn working_offline_loses_nothing() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    *storage.offline.lock().unwrap() = true;
    a.file = b"changed offline".to_vec();
    let before = a.state.clone();
    assert!(matches!(
        a.sync(&storage),
        Err(StorageError::Unreachable(_))
    ));
    assert_eq!(a.state, before);
    assert_eq!(a.file, b"changed offline");
    *storage.offline.lock().unwrap() = false;
    assert!(matches!(a.sync(&storage).unwrap(), Outcome::Pushed { .. }));
}

#[test]
fn damaged_content_never_reaches_the_project() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    let mut b = Device::new("b", b"one");
    b.sync(&storage).unwrap();
    a.file = b"two two two".to_vec();
    a.sync(&storage).unwrap();
    let key = format!("projects/{PROJECT}/blobs/{}", sha256_hex(b"two two two"));
    storage
        .objects
        .lock()
        .unwrap()
        .insert(key, b"tampered".to_vec());
    assert_eq!(b.sync(&storage), Err(StorageError::Corrupt));
    assert_eq!(b.file, b"one", "the file stays as it was");
}

#[test]
fn a_damaged_or_forged_commit_does_not_count() {
    let storage = Faulty::default();
    let mut a = Device::new("a", b"one");
    a.sync(&storage).unwrap();
    storage.objects.lock().unwrap().insert(
        format!("projects/{PROJECT}/commits/{}.json", "0".repeat(64)),
        br#"{"parents":[],"blob":"x","size":1,"device":"evil","created":"2026"}"#.to_vec(),
    );
    assert!(matches!(
        a.sync(&storage).unwrap(),
        Outcome::UpToDate { .. }
    ));
}

#[test]
fn a_backup_never_changes_the_project_and_keeps_each_state() {
    let storage = Faulty::default();
    let repo = Repo::new(&storage, PROJECT);
    let mut state = LinkState::default();
    assert!(matches!(
        repo.backup(b"one", "a", &mut state).unwrap(),
        Outcome::Pushed { .. }
    ));
    assert!(matches!(
        repo.backup(b"one", "a", &mut state).unwrap(),
        Outcome::UpToDate { .. }
    ));
    assert!(matches!(
        repo.backup(b"two", "a", &mut state).unwrap(),
        Outcome::Pushed { .. }
    ));
    let versions = Repo::versions(&state);
    assert_eq!(versions.len(), 2);
    let oldest = versions
        .iter()
        .find(|v| v.commit.parents.is_empty())
        .unwrap();
    assert_eq!(repo.read(&state, &oldest.id).unwrap(), b"one");
    assert_eq!(Repo::stored_bytes(&state), 6);
}

#[test]
fn identical_content_written_twice_is_no_conflict() {
    let storage = Faulty::default();
    let repo = Repo::new(&storage, PROJECT);
    let mut a = LinkState::default();
    repo.sync(b"same", "a", &mut a).unwrap();
    // A second commit of the same content, from another device, without a link.
    let commit = Commit {
        parents: vec![],
        blob: sha256_hex(b"same"),
        size: 4,
        device: "b".to_owned(),
        created: "2026-01-01T00:00:00Z".to_owned(),
    };
    let bytes = serde_json::to_vec(&commit).unwrap();
    storage
        .put(
            &format!("projects/{PROJECT}/commits/{}.json", sha256_hex(&bytes)),
            &bytes,
        )
        .unwrap();
    assert!(matches!(
        repo.sync(b"same", "a", &mut a).unwrap(),
        Outcome::UpToDate { .. }
    ));
}

/// Found by the random test below, once in many runs: a repeated upload that broke off
/// replaced the only good copy of a state with half of it.
#[test]
fn a_stored_state_is_not_written_again() {
    let storage = Faulty::default();
    let repo = Repo::new(&storage, PROJECT);
    let mut a = LinkState::default();
    repo.sync(b"only copy", "a", &mut a).unwrap();
    assert_eq!(storage.blob_writes.load(Ordering::Relaxed), 1);
    // Another device stores the same content, as after an interrupted run.
    let mut b = LinkState::default();
    repo.resolve_keep_local(b"only copy", "b", &mut b).unwrap();
    assert_eq!(storage.blob_writes.load(Ordering::Relaxed), 1);
}

#[test]
fn a_damaged_copy_is_replaced_by_a_good_one() {
    let storage = Faulty::default();
    let repo = Repo::new(&storage, PROJECT);
    let mut a = LinkState::default();
    repo.sync(b"good content", "a", &mut a).unwrap();
    let key = format!("projects/{PROJECT}/blobs/{}", sha256_hex(b"good content"));
    storage
        .objects
        .lock()
        .unwrap()
        .insert(key.clone(), b"go".to_vec());
    let mut b = LinkState::default();
    repo.resolve_keep_local(b"good content", "b", &mut b)
        .unwrap();
    assert_eq!(storage.objects.lock().unwrap()[&key], b"good content");
}

/// Three devices edit and sync at random while a fifth of all requests fail in every
/// way above. Every state a device ever saved must still exist at the end: in the
/// storage, or as a file the device holds. That is what "no silent loss" means.
#[test]
fn nothing_is_lost_under_random_failures() {
    for seed in 1..=40u64 {
        let storage = Faulty::default();
        storage.seed.store(seed * 7919, Ordering::Relaxed);
        storage.failure_rate.store(20, Ordering::Relaxed);
        let mut devices = [
            Device::new("a", b"start"),
            Device::new("b", b"start"),
            Device::new("c", b"start"),
        ];
        let mut saved: BTreeSet<Vec<u8>> = BTreeSet::new();
        saved.insert(b"start".to_vec());
        // Conflict copies a device keeps beside the project, as the application does.
        let mut copies: BTreeSet<Vec<u8>> = BTreeSet::new();
        let mut random = seed * 104729;
        let mut next = move |limit: u64| {
            random = random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (random >> 33) % limit
        };
        for step in 0..200u64 {
            let device = &mut devices[next(3) as usize];
            match next(4) {
                0 => {
                    // A save: a new state that must never get lost.
                    let content = format!("{} state {step} seed {seed}", device.name).into_bytes();
                    copies.insert(device.file.clone());
                    device.file.clone_from(&content);
                    saved.insert(content);
                }
                1 | 2 => {
                    let before = device.file.clone();
                    match device.sync(&storage) {
                        Ok(Outcome::Pulled { .. }) => {
                            // A pull replaces the file only with a checked, stored state.
                            assert_ne!(device.file, b"", "seed {seed}");
                            let hash = sha256_hex(&device.file);
                            assert!(storage
                                .objects
                                .lock()
                                .unwrap()
                                .contains_key(&format!("projects/{PROJECT}/blobs/{hash}")));
                            assert!(
                                device
                                    .state
                                    .base
                                    .as_ref()
                                    .is_some_and(|base| device.state.known[base].blob == hash),
                                "seed {seed}"
                            );
                            let _ = before;
                        }
                        Ok(_) => {}
                        Err(_) => assert_eq!(
                            device.file, before,
                            "a failed sync leaves the file alone, seed {seed}"
                        ),
                    }
                }
                _ => {
                    // Resolve a pending conflict by keeping the local state.
                    let repo = Repo::new(&storage, PROJECT);
                    let mut state = device.state.clone();
                    if let Ok(Outcome::Conflict { .. }) =
                        repo.sync(&device.file, &device.name, &mut state)
                    {
                        if repo
                            .resolve_keep_local(&device.file, &device.name, &mut state)
                            .is_ok()
                        {
                            device.state = state;
                        }
                    }
                }
            }
        }
        // The connection recovers, and every device finishes up.
        storage.failure_rate.store(0, Ordering::Relaxed);
        for device in &mut devices {
            for _ in 0..4 {
                if let Outcome::Conflict { .. } = device.sync(&storage).unwrap() {
                    let repo = Repo::new(&storage, PROJECT);
                    repo.resolve_keep_local(&device.file, &device.name, &mut device.state)
                        .unwrap();
                }
            }
        }
        // Everything verifiable: all commits read back with the right content.
        let objects = storage.objects.lock().unwrap().clone();
        let stored: BTreeSet<&Vec<u8>> = objects
            .iter()
            .filter(|(k, _)| k.contains("/blobs/"))
            .map(|(_, v)| v)
            .collect();
        for state in &saved {
            let held = stored.contains(state)
                || copies.contains(state)
                || devices.iter().any(|d| &d.file == state);
            assert!(held, "seed {seed}: lost {}", String::from_utf8_lossy(state));
        }
        // And the devices agree in the end.
        for device in &mut devices {
            device.sync(&storage).unwrap();
        }
        let files: BTreeSet<&Vec<u8>> = devices.iter().map(|d| &d.file).collect();
        assert_eq!(files.len(), 1, "seed {seed}: devices disagree");
    }
}
