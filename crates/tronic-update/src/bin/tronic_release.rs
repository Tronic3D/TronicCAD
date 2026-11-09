//! Makes the release key and signs update manifests. See `docs/RELEASE.md`.
//!
//! `tronic-release generate-key <file>`
//! `tronic-release sign --key <file> --channel stable --version 1.2.0 --kind feature
//!     --previous <manifest or none> --valid-days 90 --notes <json file>
//!     --artifact <platform>=<file>=<url> [--artifact ...] --out <manifest>`
//! `tronic-release verify <manifest> <public key>`
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tronic_update::{
    sha256_hex, Artifact, Channel, Manifest, ReleaseKey, SignedManifest, UpdateKind, Version,
    FORMAT,
};

fn fail<T>(message: impl std::fmt::Display) -> Result<T, String> {
    Err(message.to_string())
}

/// The whole point of a separate key: it must not be able to end up in the repository.
fn refuse_inside_a_repository(path: &Path) -> Result<(), String> {
    let absolute = match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => parent.canonicalize(),
        None => std::env::current_dir(),
    }
    .map_err(|error| format!("{}: {error}", path.display()))?;
    for directory in absolute.ancestors() {
        if directory.join(".git").exists() {
            return fail(format!(
                "{} lies inside the repository {}. Keep the release key elsewhere.",
                path.display(),
                directory.display()
            ));
        }
    }
    Ok(())
}

fn generate_key(path: &Path) -> Result<(), String> {
    use std::io::Write;
    refuse_inside_a_repository(path)?;
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).map_err(|error| error.to_string())?;
    let text: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    println!(
        "public key: {}",
        ReleaseKey::from_bytes(secret).public_hex()
    );
    Ok(())
}

#[derive(Default)]
struct SignOptions {
    key: Option<PathBuf>,
    channel: Option<String>,
    version: Option<String>,
    kind: Option<String>,
    previous: Option<String>,
    valid_days: Option<i64>,
    notes: Option<PathBuf>,
    artifacts: Vec<String>,
    out: Option<PathBuf>,
}

fn read_options(arguments: &[String]) -> Result<SignOptions, String> {
    let mut options = SignOptions::default();
    let mut rest = arguments.iter();
    while let Some(name) = rest.next() {
        let value = rest
            .next()
            .ok_or_else(|| format!("{name} needs a value"))?
            .clone();
        match name.as_str() {
            "--key" => options.key = Some(value.into()),
            "--channel" => options.channel = Some(value),
            "--version" => options.version = Some(value),
            "--kind" => options.kind = Some(value),
            "--previous" => options.previous = Some(value),
            "--valid-days" => {
                options.valid_days =
                    Some(value.parse().map_err(|_| "--valid-days needs a number")?);
            }
            "--notes" => options.notes = Some(value.into()),
            "--artifact" => options.artifacts.push(value),
            "--out" => options.out = Some(value.into()),
            other => return fail(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

fn artifact(text: &str) -> Result<Artifact, String> {
    let mut parts = text.splitn(3, '=');
    let (Some(platform), Some(file), Some(url)) = (parts.next(), parts.next(), parts.next()) else {
        return fail("--artifact needs platform=file=url");
    };
    let bytes = std::fs::read(file).map_err(|error| format!("{file}: {error}"))?;
    Ok(Artifact {
        platform: platform.to_owned(),
        url: url.to_owned(),
        sha256: sha256_hex(&bytes),
        size: bytes.len() as u64,
    })
}

/// Reads a manifest as it is, signature unchecked: only its number is used, and a
/// false one can only make a release fail in the clients' checks.
fn previous_sequence(path: &str) -> Result<u64, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let signed: SignedManifest =
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
    let manifest: Manifest =
        serde_json::from_str(&signed.payload).map_err(|error| format!("{path}: {error}"))?;
    Ok(manifest.sequence)
}

fn sign(arguments: &[String]) -> Result<(), String> {
    let options = read_options(arguments)?;
    let key_path = options.key.ok_or("--key is missing")?;
    refuse_inside_a_repository(&key_path)?;
    let key = std::fs::read_to_string(&key_path)
        .map_err(|error| format!("{}: {error}", key_path.display()))?;
    let key = ReleaseKey::from_hex(&key).map_err(|error| error.to_string())?;
    let channel: Channel = serde_json::from_value(serde_json::Value::String(
        options.channel.ok_or("--channel is missing")?,
    ))
    .map_err(|_| "--channel is stable, beta or developer")?;
    let kind: UpdateKind = serde_json::from_value(serde_json::Value::String(
        options.kind.ok_or("--kind is missing")?,
    ))
    .map_err(|_| "--kind is security or feature")?;
    let version = options.version.ok_or("--version is missing")?;
    Version::parse(&version).map_err(|error| error.to_string())?;
    // Deliberately required: forgetting it would silently restart the numbering, and
    // clients would then refuse every new manifest as a replay.
    let sequence = match options
        .previous
        .ok_or("--previous is missing (a manifest, or none)")?
        .as_str()
    {
        "none" => 1,
        path => previous_sequence(path)? + 1,
    };
    let notes: BTreeMap<String, String> = {
        let path = options.notes.ok_or("--notes is missing")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?
    };
    for language in ["de-DE", "en-US", "zh-CN", "fr-FR"] {
        if !notes.contains_key(language) {
            return fail(format!("the change log lacks {language}"));
        }
    }
    if options.artifacts.is_empty() {
        return fail("--artifact is missing");
    }
    let artifacts = options
        .artifacts
        .iter()
        .map(|text| artifact(text))
        .collect::<Result<Vec<_>, _>>()?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let days = options.valid_days.ok_or("--valid-days is missing")?;
    let signed = key
        .sign(&Manifest {
            format: FORMAT,
            channel,
            sequence,
            issued_at: now,
            expires_at: now + days * 86_400,
            version,
            kind,
            notes,
            artifacts,
        })
        .map_err(|error| error.to_string())?;
    let out = options.out.ok_or("--out is missing")?;
    let text = serde_json::to_string_pretty(&signed).map_err(|error| error.to_string())?;
    std::fs::write(&out, text).map_err(|error| format!("{}: {error}", out.display()))?;
    println!("signed: sequence {sequence}");
    Ok(())
}

fn verify(path: &str, key: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let signed: SignedManifest =
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
    let manifest = signed
        .verify(&[key.to_owned()])
        .map_err(|error| error.to_string())?;
    println!(
        "valid: {} {} sequence {}",
        manifest.channel.name(),
        manifest.version,
        manifest.sequence
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command, path] if command == "generate-key" => generate_key(Path::new(path)),
        [command, rest @ ..] if command == "sign" => sign(rest),
        [command, path, key] if command == "verify" => verify(path, key),
        _ => fail("usage: tronic-release generate-key | sign | verify, see docs/RELEASE.md"),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
