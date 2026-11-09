//! Finds and runs extension tools for TronicCAD. See `docs/EXTENSIONS.md`.
//!
//! An extension is a folder with an `extension.json` and a program in any language. It
//! runs as a process of its own: it receives the open project as JSON on standard input
//! and answers with JSON on standard output. It cannot change the project: what it
//! produces are messages and files.
//!
//! **An extension is ordinary code with the user's rights.** There is no sandbox, so one
//! runs only after the user has trusted exactly its present files.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The version of what extensions and the program say to each other.
pub const API_VERSION: u32 = 1;
const MANIFEST: &str = "extension.json";
const DEFAULT_TIMEOUT_SECONDS: u64 = 60;
const MAX_TIMEOUT_SECONDS: u64 = 3600;
const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 2000;
const MAX_FILES_IN_FINGERPRINT: usize = 20_000;
/// What a program needs in order to start at all. Nothing else is passed on, so that
/// tokens in the environment do not reach an extension.
const PASSED_ENVIRONMENT: [&str; 9] = [
    "PATH",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "LANG",
    "PATHEXT",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    /// An SPDX expression. Extensions keep their own license.
    pub license: String,
    /// The program and its arguments, run in the extension's folder.
    pub command: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExtensionError {
    #[error("the description cannot be read: {0}")]
    BadManifest(String),
    #[error("this extension is for interface version {0}")]
    UnsupportedApi(u32),
    #[error("the program could not be started: {0}")]
    CannotStart(String),
    #[error("the program ran longer than allowed")]
    TimedOut,
    #[error("the program wrote too much")]
    TooMuchOutput,
    #[error("the program failed with code {code}: {detail}")]
    Failed { code: i32, detail: String },
    #[error("the answer cannot be read: {0}")]
    BadAnswer(String),
    #[error("the file name is not allowed: {0}")]
    BadFileName(String),
    #[error("{0}")]
    Io(String),
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !id.starts_with('-')
}

/// # Errors
/// When the description is unreadable or asks for something impossible.
pub fn parse_manifest(text: &str) -> Result<Manifest, ExtensionError> {
    let manifest: Manifest = serde_json::from_str(text)
        .map_err(|error| ExtensionError::BadManifest(error.to_string()))?;
    if manifest.api != API_VERSION {
        return Err(ExtensionError::UnsupportedApi(manifest.api));
    }
    let bad = |what: &str| Err(ExtensionError::BadManifest(what.to_owned()));
    if !valid_id(&manifest.id) {
        return bad("id: lower case letters, digits and hyphens");
    }
    if manifest.name.trim().is_empty() || manifest.license.trim().is_empty() {
        return bad("name and license are needed");
    }
    if manifest
        .command
        .first()
        .is_none_or(|program| program.trim().is_empty())
    {
        return bad("command is empty");
    }
    if manifest
        .timeout_seconds
        .is_some_and(|seconds| seconds == 0 || seconds > MAX_TIMEOUT_SECONDS)
    {
        return bad("timeout_seconds is out of range");
    }
    Ok(manifest)
}

/// A folder that was found, usable or not, so that the user can be told why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub directory: PathBuf,
    pub manifest: Result<Manifest, String>,
    /// Changes with any file of the extension.
    pub fingerprint: String,
}

fn collect_files(directory: &Path, into: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(&entry.path(), into)?;
        } else {
            // A link is counted as a file, so that changing where it leads is noticed.
            into.push(entry.path());
        }
        if into.len() > MAX_FILES_IN_FINGERPRINT {
            return Err(std::io::Error::other("too many files"));
        }
    }
    Ok(())
}

/// A hash over the names and contents of all files.
///
/// # Errors
/// When the folder cannot be read.
pub fn fingerprint(directory: &Path) -> Result<String, ExtensionError> {
    let failed = |error: std::io::Error| ExtensionError::Io(error.to_string());
    let mut files = Vec::new();
    collect_files(directory, &mut files).map_err(failed)?;
    files.sort();
    let mut total = Sha256::new();
    for file in files {
        let name = file
            .strip_prefix(directory)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        total.update((name.len() as u64).to_le_bytes());
        total.update(name.as_bytes());
        let content = std::fs::read(&file).map_err(failed)?;
        total.update((content.len() as u64).to_le_bytes());
        total.update(content);
    }
    Ok(total
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Every extension folder below `root`, sorted by folder name.
///
/// # Errors
/// When `root` exists but cannot be read.
pub fn discover(root: &Path) -> Result<Vec<Found>, ExtensionError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(root).map_err(|error| ExtensionError::Io(error.to_string()))?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }
        let manifest = std::fs::read_to_string(directory.join(MANIFEST))
            .map_err(|error| error.to_string())
            .and_then(|text| parse_manifest(&text).map_err(|error| error.to_string()));
        let fingerprint = fingerprint(&directory).unwrap_or_default();
        found.push(Found {
            directory,
            manifest,
            fingerprint,
        });
    }
    found.sort_by(|a, b| a.directory.cmp(&b.directory));
    Ok(found)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    #[serde(default)]
    pub messages: Vec<Message>,
    /// File name to text content.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

/// # Errors
/// When the output is not a valid answer or names a file that is not allowed.
pub fn parse_answer(output: &[u8]) -> Result<Answer, ExtensionError> {
    let answer: Answer = serde_json::from_slice(output)
        .map_err(|error| ExtensionError::BadAnswer(error.to_string()))?;
    for name in answer.files.keys() {
        if !safe_file_name(name) {
            return Err(ExtensionError::BadFileName(name.clone()));
        }
    }
    Ok(answer)
}

/// A plain file name: an extension cannot write anywhere else through its answer.
#[must_use]
pub fn safe_file_name(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !RESERVED.contains(&stem.as_str())
        && name.chars().all(|c| {
            !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

fn read_limited(mut from: impl Read) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes = Vec::new();
    from.by_ref()
        .take(MAX_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    // Keeps reading, so that the program does not block on a full pipe.
    std::io::copy(&mut from, &mut std::io::sink())?;
    Ok(bytes)
}

/// Runs an extension on `input` and returns its checked answer.
///
/// # Errors
/// When it cannot start, runs too long, fails or answers nonsense.
pub fn run(manifest: &Manifest, directory: &Path, input: &[u8]) -> Result<Answer, ExtensionError> {
    let program = manifest
        .command
        .first()
        .ok_or_else(|| ExtensionError::BadManifest("command".into()))?;
    // `./tool` means the extension's own file, anything else is looked up as usual.
    let resolved = if program.starts_with("./") {
        directory.join(program.trim_start_matches("./"))
    } else {
        PathBuf::from(program)
    };
    let mut command = Command::new(resolved);
    command
        .args(manifest.command.iter().skip(1))
        .current_dir(directory)
        .env_clear()
        .envs(
            PASSED_ENVIRONMENT
                .iter()
                .filter_map(|name| std::env::var_os(name).map(|value| (name, value))),
        )
        .env("TRONICCAD_EXTENSION_API", API_VERSION.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| ExtensionError::CannotStart(error.to_string()))?;
    let failed = |what: &str| ExtensionError::Io(what.to_owned());
    let mut stdin = child.stdin.take().ok_or_else(|| failed("stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| failed("stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| failed("stderr"))?;
    let data = input.to_vec();
    // A program that does not read its input just ends the pipe, which is no error.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let out = std::thread::spawn(move || read_limited(stdout));
    let err = std::thread::spawn(move || read_limited(stderr));

    let limit = Duration::from_secs(manifest.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS));
    let started = Instant::now();
    let status = loop {
        match child
            .try_wait()
            .map_err(|error| ExtensionError::Io(error.to_string()))?
        {
            Some(status) => break status,
            None if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExtensionError::TimedOut);
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let _ = writer.join();
    let output = out
        .join()
        .map_err(|_| failed("reading the output"))?
        .map_err(|error| ExtensionError::Io(error.to_string()))?;
    let errors = err
        .join()
        .map_err(|_| failed("reading the errors"))?
        .unwrap_or_default();
    if output.len() as u64 > MAX_OUTPUT_BYTES {
        return Err(ExtensionError::TooMuchOutput);
    }
    if !status.success() {
        return Err(ExtensionError::Failed {
            code: status.code().unwrap_or(-1),
            detail: String::from_utf8_lossy(&errors)
                .chars()
                .take(MAX_ERROR_CHARS)
                .collect(),
        });
    }
    parse_answer(&output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(command: &[&str]) -> Manifest {
        Manifest {
            api: API_VERSION,
            id: "demo".into(),
            name: "Demo".into(),
            version: "1.0.0".into(),
            description: String::new(),
            author: "Test".into(),
            license: "MIT".into(),
            command: command.iter().map(|part| (*part).to_owned()).collect(),
            timeout_seconds: Some(20),
        }
    }

    /// A shell command on either system, with `text` printed.
    fn shell(script: &str) -> Vec<String> {
        if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), script.into()]
        } else {
            vec!["sh".into(), "-c".into(), script.into()]
        }
    }

    fn with(command: Vec<String>) -> Manifest {
        let mut found = manifest(&["x"]);
        found.command = command;
        found
    }

    fn text(value: &serde_json::Value) -> String {
        value.to_string()
    }

    #[test]
    fn a_good_description_is_read() {
        let found =
            parse_manifest(&serde_json::to_string(&manifest(&["python", "main.py"])).unwrap())
                .unwrap();
        assert_eq!(found.id, "demo");
    }

    #[test]
    fn a_description_for_another_interface_is_refused() {
        let mut other = manifest(&["x"]);
        other.api = 2;
        assert_eq!(
            parse_manifest(&serde_json::to_string(&other).unwrap()),
            Err(ExtensionError::UnsupportedApi(2))
        );
    }

    #[test]
    fn impossible_descriptions_are_refused() {
        for change in [
            json!({"id": "../evil"}),
            json!({"id": "UPPER"}),
            json!({"id": ""}),
            json!({"command": []}),
            json!({"command": [" "]}),
            json!({"name": " "}),
            json!({"license": ""}),
            json!({"timeout_seconds": 0}),
            json!({"timeout_seconds": 999_999}),
            json!({"run_as_admin": true}),
        ] {
            let mut value = serde_json::to_value(manifest(&["x"])).unwrap();
            for (key, field) in change.as_object().unwrap() {
                value[key] = field.clone();
            }
            assert!(
                matches!(
                    parse_manifest(&text(&value)),
                    Err(ExtensionError::BadManifest(_))
                ),
                "{change}"
            );
        }
        assert!(parse_manifest("no json").is_err());
    }

    #[test]
    fn file_names_cannot_leave_the_folder() {
        for name in [
            "../x", "a/b", "a\\b", "C:x", "", ".hidden", "name.", "nul", "CON.txt", "a*b", "a\nb",
        ] {
            assert!(!safe_file_name(name), "{name:?}");
        }
        for name in ["part-list.csv", "Stückliste 1.txt", "电路.csv"] {
            assert!(safe_file_name(name), "{name}");
        }
    }

    #[test]
    fn an_answer_with_a_bad_file_name_is_refused_whole() {
        let answer = json!({"files": {"good.txt": "a", "../evil.txt": "b"}});
        assert_eq!(
            parse_answer(answer.to_string().as_bytes()),
            Err(ExtensionError::BadFileName("../evil.txt".into()))
        );
    }

    #[test]
    fn unknown_answers_are_refused() {
        assert!(parse_answer(b"plain text").is_err());
        assert!(parse_answer(br#"{"run": "this"}"#).is_err());
    }

    #[test]
    fn the_fingerprint_follows_every_change() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "one").unwrap();
        std::fs::create_dir(directory.path().join("sub")).unwrap();
        std::fs::write(directory.path().join("sub/b.txt"), "two").unwrap();
        let first = fingerprint(directory.path()).unwrap();
        assert_eq!(first, fingerprint(directory.path()).unwrap());
        std::fs::write(directory.path().join("sub/b.txt"), "changed").unwrap();
        let changed = fingerprint(directory.path()).unwrap();
        assert_ne!(first, changed);
        std::fs::write(directory.path().join("new.txt"), "").unwrap();
        assert_ne!(changed, fingerprint(directory.path()).unwrap());
    }

    #[test]
    fn a_broken_folder_is_listed_with_its_reason() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("broken")).unwrap();
        std::fs::write(root.path().join("broken/extension.json"), "{").unwrap();
        std::fs::create_dir(root.path().join("good")).unwrap();
        std::fs::write(
            root.path().join("good/extension.json"),
            serde_json::to_string(&manifest(&["x"])).unwrap(),
        )
        .unwrap();
        let found = discover(root.path()).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].manifest.is_err());
        assert!(found[1].manifest.is_ok());
    }

    #[test]
    fn no_folder_is_no_extensions() {
        assert!(discover(Path::new("no/such/folder")).unwrap().is_empty());
    }

    #[test]
    fn a_tool_answers_and_receives_its_input() {
        let directory = tempfile::tempdir().unwrap();
        // From files, because quotes in an argument mean different things on each system.
        let found = if cfg!(windows) {
            std::fs::write(
                directory.path().join("tool.cmd"),
                "@findstr hello > nul && echo {\"messages\":[{\"level\":\"info\",\"text\":\"read\"}]}
",
            )
            .unwrap();
            with(vec!["cmd".into(), "/C".into(), "tool.cmd".into()])
        } else {
            std::fs::write(
                directory.path().join("tool.sh"),
                "grep -q hello && echo '{\"messages\":[{\"level\":\"info\",\"text\":\"read\"}]}'
",
            )
            .unwrap();
            with(vec!["sh".into(), "tool.sh".into()])
        };
        let answer = run(
            &found,
            directory.path(),
            b"say hello
",
        )
        .unwrap();
        assert_eq!(answer.messages[0].text, "read");
    }

    #[test]
    fn a_failing_tool_is_reported_with_its_message() {
        let directory = tempfile::tempdir().unwrap();
        let found = run(
            &with(shell("echo broken 1>&2 && exit 3")),
            directory.path(),
            b"",
        );
        let Err(ExtensionError::Failed { code, detail }) = found else {
            unreachable!("{found:?}");
        };
        assert_eq!(code, 3);
        assert!(detail.contains("broken"));
    }

    #[test]
    fn a_tool_that_hangs_is_ended() {
        let directory = tempfile::tempdir().unwrap();
        let mut hanging = with(shell(if cfg!(windows) {
            "ping -n 30 127.0.0.1 > nul"
        } else {
            "sleep 30"
        }));
        hanging.timeout_seconds = Some(1);
        let started = Instant::now();
        assert_eq!(
            run(&hanging, directory.path(), b""),
            Err(ExtensionError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn a_program_that_does_not_exist_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            run(&manifest(&["no-such-program-here"]), directory.path(), b""),
            Err(ExtensionError::CannotStart(_))
        ));
    }

    #[test]
    fn secrets_in_the_environment_do_not_reach_a_tool() {
        let directory = tempfile::tempdir().unwrap();
        // Set for this process only, which the environment of a test binary allows.
        let found = if cfg!(windows) {
            std::fs::write(
                directory.path().join("tool.cmd"),
                "@if defined CARGO_PKG_NAME (exit /b 7) else (echo {})
",
            )
            .unwrap();
            with(vec!["cmd".into(), "/C".into(), "tool.cmd".into()])
        } else {
            std::fs::write(
                directory.path().join("tool.sh"),
                "test -z \"$CARGO_PKG_NAME\" && echo '{}'
",
            )
            .unwrap();
            with(vec!["sh".into(), "tool.sh".into()])
        };
        assert_eq!(
            run(&found, directory.path(), b"").unwrap(),
            Answer::default()
        );
    }
}
