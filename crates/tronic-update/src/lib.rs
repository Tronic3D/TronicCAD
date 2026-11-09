//! Signed update manifests and the checks before anything is installed.
//!
//! The signature covers everything that decides about an update: version, channel,
//! platform, size, content hash and how recent the statement is. That is deliberate:
//! CVE-2026-95625 describes an updater whose signature covered only the binary, which
//! allows forcing an older version. This crate knows nothing about networks or files,
//! so that all rules can be tested. See `docs/specs/updates.md`.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

pub const FORMAT: u32 = 1;
/// What an installer may be at most, against an answer that never ends.
pub const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
/// A manifest is at most this long, however the server behaves.
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Beta,
    Developer,
}

impl Channel {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Developer => "developer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    Security,
    Feature,
}

/// `major.minor.patch`, optionally `-pre`, ordered as in Semantic Versioning.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
}

impl Version {
    /// # Errors
    /// When the text is not a version.
    pub fn parse(text: &str) -> Result<Self, UpdateError> {
        let bad = || UpdateError::BadVersion(text.to_owned());
        let text_only = text.split('+').next().ok_or_else(bad)?;
        let (core, pre) = match text_only.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text_only, None),
        };
        let numbers: Vec<&str> = core.split('.').collect();
        let [major, minor, patch] = numbers.as_slice() else {
            return Err(bad());
        };
        let number = |part: &str| -> Result<u64, UpdateError> {
            if part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => {
                let parts: Vec<String> = pre.split('.').map(str::to_owned).collect();
                let fine = parts.iter().all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                });
                if !fine {
                    return Err(bad());
                }
                parts
            }
        };
        Ok(Self {
            major: number(major)?,
            minor: number(minor)?,
            patch: number(patch)?,
            pre,
        })
    }

    /// Worth a backup first: more than a correction.
    #[must_use]
    pub fn is_larger_step_from(&self, older: &Self) -> bool {
        self.major != older.major || self.minor != older.minor
    }
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        // A release is newer than its previews.
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (left, right) in a.iter().zip(b) {
        let found = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left.cmp(right),
        };
        if found != Ordering::Equal {
            return found;
        }
    }
    a.len().cmp(&b.len())
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| compare_pre(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// For example `windows-x86_64`.
    pub platform: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// What the release key signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub channel: Channel,
    /// Grows with every manifest of a channel. Keeps an older one from being replayed.
    pub sequence: u64,
    pub issued_at: i64,
    /// Keeps someone from hiding updates by showing an old manifest for ever.
    pub expires_at: i64,
    pub version: String,
    pub kind: UpdateKind,
    /// Change log by language code.
    pub notes: BTreeMap<String, String>,
    pub artifacts: Vec<Artifact>,
}

/// The file on the server: the exact signed bytes and their signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("the manifest cannot be read")]
    Malformed,
    #[error("the signature does not match any trusted key")]
    BadSignature,
    #[error("manifest format {0} is not known")]
    UnknownFormat(u32),
    #[error("{0} is not a version")]
    BadVersion(String),
    #[error("the manifest is for channel {found}, not {expected}")]
    WrongChannel { expected: String, found: String },
    #[error("the manifest is older than one already seen ({found} after {seen})")]
    Replayed { seen: u64, found: u64 },
    #[error("the manifest expired")]
    Expired,
    #[error("no build for this platform: {0}")]
    WrongPlatform(String),
    #[error("the download address is not acceptable")]
    BadAddress,
    #[error("the size does not match: {found} bytes instead of {expected}")]
    WrongSize { expected: u64, found: u64 },
    #[error("the content does not match its signed hash")]
    WrongContent,
    #[error("invalid key")]
    BadKey,
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0_u8; N];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The release key. Never on a server, and never in the source code.
pub struct ReleaseKey(SigningKey);

impl ReleaseKey {
    /// # Errors
    /// When the text is not a hex key of 32 bytes.
    pub fn from_hex(secret: &str) -> Result<Self, UpdateError> {
        let bytes = unhex::<32>(secret.trim()).ok_or(UpdateError::BadKey)?;
        Ok(Self(SigningKey::from_bytes(&bytes)))
    }

    #[must_use]
    pub fn from_bytes(secret: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&secret))
    }

    #[must_use]
    pub fn public_hex(&self) -> String {
        hex(self.0.verifying_key().as_bytes())
    }

    /// # Errors
    /// When the manifest cannot be written, which does not happen for valid data.
    pub fn sign(&self, manifest: &Manifest) -> Result<SignedManifest, UpdateError> {
        let payload = serde_json::to_string(manifest).map_err(|_| UpdateError::Malformed)?;
        let signature = self.0.sign(payload.as_bytes());
        Ok(SignedManifest {
            payload,
            signature: hex(&signature.to_bytes()),
        })
    }
}

impl SignedManifest {
    /// Any of the trusted keys may have signed: the build carries the current key and
    /// the next one, which makes changing the key possible. Nothing is read from the
    /// payload before the signature holds.
    ///
    /// # Errors
    /// When no trusted key signed exactly these bytes.
    pub fn verify(&self, trusted: &[String]) -> Result<Manifest, UpdateError> {
        let signature = unhex::<64>(&self.signature).ok_or(UpdateError::BadSignature)?;
        let signature = Signature::from_bytes(&signature);
        let holds = trusted.iter().any(|key| {
            unhex::<32>(key.trim())
                .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
                .is_some_and(|key| {
                    key.verify_strict(self.payload.as_bytes(), &signature)
                        .is_ok()
                })
        });
        if !holds {
            return Err(UpdateError::BadSignature);
        }
        let manifest: Manifest =
            serde_json::from_str(&self.payload).map_err(|_| UpdateError::Malformed)?;
        if manifest.format != FORMAT {
            return Err(UpdateError::UnknownFormat(manifest.format));
        }
        Ok(manifest)
    }
}

/// What this installation knows when it decides.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    pub trusted_keys: &'a [String],
    pub channel: Channel,
    pub platform: &'a str,
    pub current: Version,
    /// The highest sequence seen on this channel.
    pub seen_sequence: u64,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// This installation is up to date. An older version in a manifest ends here too,
    /// and it is never offered.
    UpToDate {
        sequence: u64,
    },
    Available(Box<Offer>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: Version,
    pub kind: UpdateKind,
    pub sequence: u64,
    pub notes: BTreeMap<String, String>,
    pub artifact: Artifact,
    pub backup_first: bool,
}

/// Plain http only to this computer, for tests and nothing else.
fn acceptable_address(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("http://localhost:")
}

/// All checks on a manifest, in one place.
///
/// # Errors
/// When the manifest must not be believed, or offers nothing usable here.
pub fn decide(signed: &SignedManifest, context: &Context<'_>) -> Result<Decision, UpdateError> {
    let manifest = signed.verify(context.trusted_keys)?;
    if manifest.channel != context.channel {
        return Err(UpdateError::WrongChannel {
            expected: context.channel.name().to_owned(),
            found: manifest.channel.name().to_owned(),
        });
    }
    if manifest.sequence < context.seen_sequence {
        return Err(UpdateError::Replayed {
            seen: context.seen_sequence,
            found: manifest.sequence,
        });
    }
    if manifest.expires_at <= context.now {
        return Err(UpdateError::Expired);
    }
    let version = Version::parse(&manifest.version)?;
    if version <= context.current {
        return Ok(Decision::UpToDate {
            sequence: manifest.sequence,
        });
    }
    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.platform == context.platform)
        .cloned()
        .ok_or_else(|| UpdateError::WrongPlatform(context.platform.to_owned()))?;
    if !acceptable_address(&artifact.url) {
        return Err(UpdateError::BadAddress);
    }
    if artifact.size == 0
        || artifact.size > MAX_ARTIFACT_BYTES
        || unhex::<32>(&artifact.sha256).is_none()
    {
        return Err(UpdateError::Malformed);
    }
    Ok(Decision::Available(Box::new(Offer {
        backup_first: version.is_larger_step_from(&context.current),
        version,
        kind: manifest.kind,
        sequence: manifest.sequence,
        notes: manifest.notes,
        artifact,
    })))
}

/// The downloaded bytes against what was signed.
///
/// # Errors
/// When they are not exactly what was signed.
pub fn check_artifact(artifact: &Artifact, size: u64, sha256: &str) -> Result<(), UpdateError> {
    if size != artifact.size {
        return Err(UpdateError::WrongSize {
            expected: artifact.size,
            found: size,
        });
    }
    if !sha256.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(UpdateError::WrongContent);
    }
    Ok(())
}

/// The platform this program was built for, as manifests name it.
#[must_use]
pub fn this_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    const NOW: i64 = 2_000_000_000;
    const PLATFORM: &str = "windows-x86_64";

    fn key() -> ReleaseKey {
        ReleaseKey::from_bytes([5; 32])
    }

    fn manifest(version: &str) -> Manifest {
        Manifest {
            format: FORMAT,
            channel: Channel::Stable,
            sequence: 10,
            issued_at: NOW - 100,
            expires_at: NOW + 1000,
            version: version.to_owned(),
            kind: UpdateKind::Feature,
            notes: BTreeMap::from([("de-DE".to_owned(), "Neu".to_owned())]),
            artifacts: vec![Artifact {
                platform: PLATFORM.to_owned(),
                url: "https://updates.example.test/a.exe".to_owned(),
                sha256: sha256_hex(b"content"),
                size: 7,
            }],
        }
    }

    fn trusted() -> Vec<String> {
        vec![key().public_hex()]
    }

    fn decide_as(signed: &SignedManifest, current: &str) -> Result<Decision, UpdateError> {
        let keys = trusted();
        decide(
            signed,
            &Context {
                trusted_keys: &keys,
                channel: Channel::Stable,
                platform: PLATFORM,
                current: Version::parse(current).unwrap(),
                seen_sequence: 5,
                now: NOW,
            },
        )
    }

    fn signed(version: &str) -> SignedManifest {
        key().sign(&manifest(version)).unwrap()
    }

    #[test]
    fn a_newer_version_is_offered() {
        let Decision::Available(offer) = decide_as(&signed("1.2.0"), "1.1.0").unwrap() else {
            panic!();
        };
        assert_eq!(offer.version.to_string(), "1.2.0");
        assert_eq!(offer.sequence, 10);
        assert!(offer.backup_first);
    }

    /// The point of CVE-2026-95625: an older package with a valid signature.
    #[test]
    fn a_validly_signed_older_version_is_never_offered() {
        assert_eq!(
            decide_as(&signed("1.0.0"), "1.1.0"),
            Ok(Decision::UpToDate { sequence: 10 })
        );
        assert_eq!(
            decide_as(&signed("1.1.0"), "1.1.0"),
            Ok(Decision::UpToDate { sequence: 10 })
        );
    }

    #[test]
    fn a_changed_version_breaks_the_signature() {
        let mut changed = signed("1.0.0");
        changed.payload = changed.payload.replace("1.0.0", "9.9.9");
        assert_eq!(decide_as(&changed, "1.1.0"), Err(UpdateError::BadSignature));
    }

    #[test]
    fn every_signed_field_is_covered() {
        let original = signed("2.0.0");
        for (from, to) in [
            ("stable", "beta"),
            ("\"sequence\":10", "\"sequence\":99"),
            ("https://updates.example.test", "https://evil.example.test"),
            ("\"size\":7", "\"size\":8"),
            ("feature", "security"),
            ("windows-x86_64", "linux-x86_64"),
        ] {
            let mut changed = original.clone();
            assert!(changed.payload.contains(from), "{from}");
            changed.payload = changed.payload.replace(from, to);
            assert_eq!(
                decide_as(&changed, "1.0.0"),
                Err(UpdateError::BadSignature),
                "{from}"
            );
        }
    }

    #[test]
    fn a_foreign_key_is_refused() {
        let foreign = ReleaseKey::from_bytes([6; 32])
            .sign(&manifest("2.0.0"))
            .unwrap();
        assert_eq!(decide_as(&foreign, "1.0.0"), Err(UpdateError::BadSignature));
    }

    #[test]
    fn the_next_key_is_accepted_too() {
        let next = ReleaseKey::from_bytes([6; 32]);
        let keys = vec![key().public_hex(), next.public_hex()];
        let found = decide(
            &next.sign(&manifest("2.0.0")).unwrap(),
            &Context {
                trusted_keys: &keys,
                channel: Channel::Stable,
                platform: PLATFORM,
                current: Version::parse("1.0.0").unwrap(),
                seen_sequence: 0,
                now: NOW,
            },
        );
        assert!(matches!(found, Ok(Decision::Available(_))));
    }

    #[test]
    fn no_trusted_key_trusts_nothing() {
        let found = decide(
            &signed("2.0.0"),
            &Context {
                trusted_keys: &[],
                channel: Channel::Stable,
                platform: PLATFORM,
                current: Version::parse("1.0.0").unwrap(),
                seen_sequence: 0,
                now: NOW,
            },
        );
        assert_eq!(found, Err(UpdateError::BadSignature));
    }

    #[test]
    fn broken_input_is_refused_without_panic() {
        for signature in ["", "zz", "é", &"é".repeat(64), &"a".repeat(128)] {
            let mut broken = signed("2.0.0");
            broken.signature = signature.to_owned();
            assert_eq!(
                decide_as(&broken, "1.0.0"),
                Err(UpdateError::BadSignature),
                "{signature}"
            );
        }
        for trusted_key in ["", "é", "zz"] {
            let keys = vec![trusted_key.to_owned()];
            assert_eq!(
                signed("2.0.0").verify(&keys),
                Err(UpdateError::BadSignature)
            );
        }
    }

    #[test]
    fn an_unknown_format_is_refused() {
        let mut other = manifest("2.0.0");
        other.format = 2;
        assert_eq!(
            decide_as(&key().sign(&other).unwrap(), "1.0.0"),
            Err(UpdateError::UnknownFormat(2))
        );
    }

    #[test]
    fn another_channel_is_refused() {
        let mut other = manifest("2.0.0");
        other.channel = Channel::Developer;
        assert!(matches!(
            decide_as(&key().sign(&other).unwrap(), "1.0.0"),
            Err(UpdateError::WrongChannel { .. })
        ));
    }

    #[test]
    fn an_older_manifest_is_refused_even_with_a_new_version() {
        let mut old = manifest("2.0.0");
        old.sequence = 4;
        assert_eq!(
            decide_as(&key().sign(&old).unwrap(), "1.0.0"),
            Err(UpdateError::Replayed { seen: 5, found: 4 })
        );
    }

    #[test]
    fn an_expired_manifest_is_refused() {
        let mut old = manifest("2.0.0");
        old.expires_at = NOW;
        assert_eq!(
            decide_as(&key().sign(&old).unwrap(), "1.0.0"),
            Err(UpdateError::Expired)
        );
    }

    #[test]
    fn another_platform_is_refused() {
        let mut other = manifest("2.0.0");
        other.artifacts[0].platform = "linux-x86_64".to_owned();
        assert_eq!(
            decide_as(&key().sign(&other).unwrap(), "1.0.0"),
            Err(UpdateError::WrongPlatform(PLATFORM.to_owned()))
        );
    }

    #[test]
    fn plain_http_elsewhere_is_refused_even_when_signed() {
        for url in [
            "http://updates.example.test/a.exe",
            "ftp://x/a",
            "file:///C:/a.exe",
            "http://127.0.0.1.evil.test/a",
        ] {
            let mut other = manifest("2.0.0");
            other.artifacts[0].url = url.to_owned();
            assert_eq!(
                decide_as(&key().sign(&other).unwrap(), "1.0.0"),
                Err(UpdateError::BadAddress),
                "{url}"
            );
        }
    }

    #[test]
    fn impossible_artifacts_are_refused() {
        for (size, hash) in [
            (0, sha256_hex(b"x")),
            (MAX_ARTIFACT_BYTES + 1, sha256_hex(b"x")),
            (7, "short".to_owned()),
        ] {
            let mut other = manifest("2.0.0");
            other.artifacts[0].size = size;
            other.artifacts[0].sha256 = hash;
            assert_eq!(
                decide_as(&key().sign(&other).unwrap(), "1.0.0"),
                Err(UpdateError::Malformed)
            );
        }
    }

    #[test]
    fn a_small_step_needs_no_backup_and_a_larger_one_does() {
        let new = |text| Version::parse(text).unwrap();
        assert!(!new("1.2.4").is_larger_step_from(&new("1.2.3")));
        assert!(new("1.3.0").is_larger_step_from(&new("1.2.3")));
        assert!(new("2.0.0").is_larger_step_from(&new("1.9.9")));
    }

    #[test]
    fn the_downloaded_content_must_be_what_was_signed() {
        let artifact = &manifest("2.0.0").artifacts[0];
        assert_eq!(check_artifact(artifact, 7, &sha256_hex(b"content")), Ok(()));
        assert_eq!(
            check_artifact(artifact, 7, &sha256_hex(b"CONTENT")),
            Err(UpdateError::WrongContent)
        );
        assert_eq!(
            check_artifact(artifact, 8, &sha256_hex(b"content")),
            Err(UpdateError::WrongSize {
                expected: 7,
                found: 8
            })
        );
    }

    #[test]
    fn versions_are_ordered_as_semantic_versioning_says() {
        let ordered = [
            "0.1.0",
            "0.9.0",
            "0.10.0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            let (a, b) = (
                Version::parse(pair[0]).unwrap(),
                Version::parse(pair[1]).unwrap(),
            );
            assert!(a < b, "{} < {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn versions_that_are_none_are_refused() {
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "a.b.c",
            "01.2.3",
            "1.2.3-",
            "1.2.-3",
            "1..3",
            "1.2.3-a b",
            "-1.2.3",
            "99999999999999999999.0.0",
        ] {
            assert!(Version::parse(text).is_err(), "{text}");
        }
        assert_eq!(Version::parse("1.2.3+build5").unwrap().to_string(), "1.2.3");
    }

    #[test]
    fn a_version_that_is_none_in_a_signed_manifest_is_refused() {
        assert!(matches!(
            decide_as(&signed("latest"), "1.0.0"),
            Err(UpdateError::BadVersion(_))
        ));
    }

    #[test]
    fn unknown_fields_are_refused() {
        let mut value: serde_json::Value = serde_json::to_value(manifest("2.0.0")).unwrap();
        value["install_without_asking"] = serde_json::json!(true);
        let payload = value.to_string();
        let signature = hex(&ReleaseKey::from_bytes([5; 32])
            .0
            .sign(payload.as_bytes())
            .to_bytes());
        let found = SignedManifest { payload, signature };
        assert_eq!(decide_as(&found, "1.0.0"), Err(UpdateError::Malformed));
    }
}
