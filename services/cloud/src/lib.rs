//! The TronicCAD cloud service.
//!
//! Instances hold no state of their own: accounts and metadata live in PostgreSQL,
//! content in an object store, and change events travel through PostgreSQL's
//! NOTIFY. So more instances behind a load balancer serve the same data.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::body::{to_bytes, Body};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_postgres::{AsyncMessage, NoTls};
use tronic_storage::{
    check_key, sha256_hex, FolderProvider, S3Provider, StorageError, StorageProvider,
};
use uuid::Uuid;

mod audit;
pub mod permissions;
mod team;

use audit::Entry;
use permissions::{allows, Action};

const SCHEMA: &str = include_str!("schema.sql");
const MAX_OBJECT_BYTES: usize = 600 * 1024 * 1024;
/// Everything but objects is small JSON.
const MAX_JSON_BYTES: usize = 64 * 1024;
/// Password hashing is slow on purpose, so it must not be able to take all processors.
const MAX_HASHING: usize = 8;
const MAX_TRANSFERS: usize = 16;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1800);
const TOKEN_DAYS: i32 = 90;
const MIN_PASSWORD_CHARS: usize = 12;
const MAX_PASSWORD_CHARS: usize = 256;
const MAX_FAILED_LOGINS: i64 = 10;
const FAILED_LOGIN_MINUTES: i32 = 15;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(3600);
const EVENT_CHANNEL: &str = "tronic_objects";

/// Where content lives. A provider has no delete on purpose, but an account's data
/// must be removable, so that exists here and only here.
pub enum Store {
    Folder(FolderProvider, std::path::PathBuf),
    S3(S3Provider),
}

impl Store {
    pub fn folder(root: impl Into<std::path::PathBuf>) -> Self {
        let root = root.into();
        Self::Folder(FolderProvider::new(root.clone()), root)
    }

    pub fn s3(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, StorageError> {
        Ok(Self::S3(S3Provider::new(
            endpoint, bucket, region, access_key, secret_key,
        )?))
    }

    pub(crate) fn provider(&self) -> &dyn StorageProvider {
        match self {
            Self::Folder(provider, _) => provider,
            Self::S3(provider) => provider,
        }
    }

    fn remove(&self, key: &str) -> Result<(), StorageError> {
        check_key(key)?;
        match self {
            Self::Folder(_, root) => {
                let path = key
                    .split('/')
                    .fold(root.clone(), |path, part| path.join(part));
                match std::fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(StorageError::Io(error.to_string())),
                }
            }
            Self::S3(provider) => provider.remove(key),
        }
    }
}

pub struct Config {
    pub default_quota: i64,
    pub allow_registration: bool,
}

pub(crate) struct Inner {
    db: deadpool_postgres::Pool,
    pub(crate) store: Store,
    config: Config,
    events: broadcast::Sender<String>,
    hashing: tokio::sync::Semaphore,
    transfers: tokio::sync::Semaphore,
    /// Compared against when an account does not exist, so that the answer takes
    /// as long as for one that does.
    dummy_hash: String,
}

#[derive(Clone)]
pub struct AppState(pub(crate) Arc<Inner>);

/// What goes wrong, as the client sees it. Details stay in the server's log.
#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest(&'static str),
    Conflict(&'static str),
    TooMany,
    Quota,
    /// The role does not allow changing this area of a project.
    AreaDenied(&'static str),
    /// Someone else holds the lock on this area.
    Locked {
        area: &'static str,
        holder: String,
    },
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::AreaDenied(area) => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "area_denied", "area": area})),
                )
                    .into_response();
            }
            Self::Locked { area, holder } => {
                return (
                    StatusCode::LOCKED,
                    Json(json!({"error": "area_locked", "area": area, "holder": holder})),
                )
                    .into_response();
            }
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(code) => (StatusCode::BAD_REQUEST, code),
            Self::Conflict(code) => (StatusCode::CONFLICT, code),
            Self::TooMany => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::Quota => (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded"),
            Self::Internal(detail) => {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("internal error: {detail}");
                }
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        (status, Json(json!({"error": code}))).into_response()
    }
}

impl From<tokio_postgres::Error> for ApiError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        Self::Internal(error.to_string())
    }
}

/// Runs blocking work (hashing, object storage) without holding up other requests.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))
}

async fn limited_hashing<T: Send + 'static>(
    state: &AppState,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    let _permit = state
        .0
        .hashing
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    blocking(work).await
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| ApiError::Internal(error.to_string()))
}

fn password_matches(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
        .unwrap_or(false)
}

pub(crate) fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub(crate) fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let parts: Vec<&str> = email.split('@').collect();
    let fine = email.len() <= 254
        && parts.len() == 2
        && !parts[0].is_empty()
        && parts[1].contains('.')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    if fine {
        Ok(email)
    } else {
        Err(ApiError::BadRequest("invalid_email"))
    }
}

fn check_password(password: &str) -> Result<(), ApiError> {
    let count = password.chars().count();
    if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&count) {
        Ok(())
    } else {
        Err(ApiError::BadRequest("weak_password"))
    }
}

pub(crate) async fn client(state: &AppState) -> Result<deadpool_postgres::Object, ApiError> {
    state
        .0
        .db
        .get()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))
}

async fn new_session(state: &AppState, account: Uuid) -> Result<String, ApiError> {
    let random: [u8; 32] = rand::random();
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    client(state)
        .await?
        .execute(
            "INSERT INTO sessions (token_hash, account, expires) VALUES ($1, $2, now() + make_interval(days => $3))",
            &[&token_hash(&token), &account, &TOKEN_DAYS],
        )
        .await?;
    Ok(token)
}

pub(crate) async fn authed(state: &AppState, headers: &HeaderMap) -> Result<Uuid, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let row = client(state)
        .await?
        .query_opt(
            "SELECT account FROM sessions WHERE token_hash = $1 AND expires > now()",
            &[&token_hash(token)],
        )
        .await?;
    row.map(|row| row.get(0)).ok_or(ApiError::Unauthorized)
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
    /// Lets someone with an invitation register on a server with closed registration.
    #[serde(default)]
    invitation: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&body.email)?;
    if !state.0.config.allow_registration {
        let code = body.invitation.as_deref().ok_or(ApiError::Forbidden)?;
        let valid = client(&state)
            .await?
            .query_opt(
                "SELECT 1 FROM invitations WHERE code_hash = $1 AND email = $2
                 AND used_at IS NULL AND expires > now()",
                &[&token_hash(code), &email],
            )
            .await?;
        if valid.is_none() {
            return Err(ApiError::Forbidden);
        }
    }
    check_password(&body.password)?;
    let password = body.password;
    let hash = limited_hashing(&state, move || hash_password(&password)).await??;
    let id = Uuid::new_v4();
    let mut db = client(&state).await?;
    let transaction = db.transaction().await?;
    let inserted = transaction
        .execute(
            "INSERT INTO accounts (id, email, password_hash, quota_bytes) VALUES ($1, $2, $3, $4) ON CONFLICT (email) DO NOTHING",
            &[&id, &email, &hash, &state.0.config.default_quota],
        )
        .await?;
    if inserted == 0 {
        return Err(ApiError::Conflict("email_taken"));
    }
    audit::record(
        &transaction,
        Entry {
            actor: Some(id),
            actor_email: &email,
            action: "account.create",
            ..Entry::default()
        },
    )
    .await?;
    transaction.commit().await?;
    let token = new_session(&state, id).await?;
    Ok((StatusCode::CREATED, Json(json!({"token": token}))).into_response())
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&body.email).map_err(|_| ApiError::Unauthorized)?;
    let mut db = client(&state).await?;
    let failures: i64 = db
        .query_one(
            "SELECT count(*) FROM failed_logins WHERE email = $1 AND at > now() - make_interval(mins => $2)",
            &[&email, &FAILED_LOGIN_MINUTES],
        )
        .await?
        .get(0);
    if failures >= MAX_FAILED_LOGINS {
        return Err(ApiError::TooMany);
    }
    let found = db
        .query_opt(
            "SELECT id, password_hash FROM accounts WHERE email = $1",
            &[&email],
        )
        .await?
        .map(|row| (row.get::<_, Uuid>(0), row.get::<_, String>(1)));
    let stored = found
        .as_ref()
        .map_or_else(|| state.0.dummy_hash.clone(), |(_, hash)| hash.clone());
    let password = body.password;
    let matches = limited_hashing(&state, move || password_matches(&password, &stored)).await?;
    match found {
        Some((id, _)) if matches => {
            let transaction = db.transaction().await?;
            transaction
                .execute("DELETE FROM failed_logins WHERE email = $1", &[&email])
                .await?;
            audit::record(
                &transaction,
                Entry {
                    actor: Some(id),
                    actor_email: &email,
                    action: "login.success",
                    ..Entry::default()
                },
            )
            .await?;
            transaction.commit().await?;
            drop(db);
            let token = new_session(&state, id).await?;
            Ok(Json(json!({"token": token})).into_response())
        }
        _ => {
            let transaction = db.transaction().await?;
            transaction
                .execute("INSERT INTO failed_logins (email) VALUES ($1)", &[&email])
                .await?;
            audit::record(
                &transaction,
                Entry {
                    actor_email: &email,
                    action: "login.failure",
                    ..Entry::default()
                },
            )
            .await?;
            transaction.commit().await?;
            Err(ApiError::Unauthorized)
        }
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    authed(&state, &headers).await?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let email: String = db
        .query_one("SELECT email FROM accounts WHERE id = $1", &[&account])
        .await?
        .get(0);
    let transaction = db.transaction().await?;
    transaction
        .execute(
            "DELETE FROM sessions WHERE token_hash = $1",
            &[&token_hash(token)],
        )
        .await?;
    audit::record(
        &transaction,
        Entry {
            actor: Some(account),
            actor_email: &email,
            action: "logout",
            ..Entry::default()
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn account(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let id = authed(&state, &headers).await?;
    let row = client(&state)
        .await?
        .query_one(
            "SELECT email, used_bytes, quota_bytes FROM accounts WHERE id = $1",
            &[&id],
        )
        .await?;
    let (email, used, quota): (String, i64, i64) = (row.get(0), row.get(1), row.get(2));
    Ok(Json(json!({"email": email, "used_bytes": used, "quota_bytes": quota})).into_response())
}

#[derive(Deserialize)]
struct Confirmation {
    password: String,
}

/// Removes the account, its projects and their content. Asks for the password,
/// because a stolen token must not be able to do this.
async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Confirmation>,
) -> Result<StatusCode, ApiError> {
    let id = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let stored: String = db
        .query_one("SELECT password_hash FROM accounts WHERE id = $1", &[&id])
        .await?
        .get(0);
    let password = body.password;
    if !limited_hashing(&state, move || password_matches(&password, &stored)).await? {
        return Err(ApiError::Unauthorized);
    }
    let names: Vec<(Uuid, String)> = db
        .query(
            "SELECT o.repo, o.name FROM objects o JOIN repos r ON r.id = o.repo
             WHERE r.owner = $1 AND r.team IS NULL",
            &[&id],
        )
        .await?
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    // A team goes on without this account, so it must keep an administrator, and
    // its projects must not disappear with their creator.
    let sole_admin = db
        .query_opt(
            "SELECT 1 FROM members m WHERE m.account = $1 AND m.role = 'admin'
             AND NOT EXISTS (SELECT 1 FROM members o WHERE o.team = m.team
                             AND o.role = 'admin' AND o.account <> $1)
             AND EXISTS (SELECT 1 FROM members o WHERE o.team = m.team AND o.account <> $1)",
            &[&id],
        )
        .await?;
    if sole_admin.is_some() {
        return Err(ApiError::Conflict("last_admin"));
    }
    let worker = state.clone();
    blocking(move || {
        names
            .iter()
            .try_for_each(|(repo, name)| worker.0.store.remove(&format!("{repo}/{name}")))
    })
    .await??;
    let email: String = db
        .query_one("SELECT email FROM accounts WHERE id = $1", &[&id])
        .await?
        .get(0);
    let transaction = db.transaction().await?;
    transaction
        .execute(
            "UPDATE repos r SET owner = (SELECT m.account FROM members m
                 WHERE m.team = r.team AND m.role = 'admin' AND m.account <> $1 LIMIT 1)
             WHERE r.owner = $1 AND r.team IS NOT NULL
             AND EXISTS (SELECT 1 FROM members m WHERE m.team = r.team
                         AND m.role = 'admin' AND m.account <> $1)",
            &[&id],
        )
        .await?;
    audit::record(
        &transaction,
        Entry {
            actor: Some(id),
            actor_email: &email,
            action: "account.delete",
            ..Entry::default()
        },
    )
    .await?;
    transaction
        .execute("DELETE FROM accounts WHERE id = $1", &[&id])
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd)]
enum Role {
    Read,
    Write,
}

async fn owner_of(state: &AppState, repo: Uuid) -> Result<Option<Uuid>, ApiError> {
    Ok(client(state)
        .await?
        .query_opt("SELECT owner FROM repos WHERE id = $1", &[&repo])
        .await?
        .map(|row| row.get(0)))
}

async fn role_of(state: &AppState, repo: Uuid, account: Uuid) -> Result<Option<Role>, ApiError> {
    // In a team project the team decides, whoever created it.
    if let Some((_, role)) = team::project_role(&client(state).await?, repo, account).await? {
        return Ok(role.map(|role| {
            if allows(role, Action::WriteAssignedArea) {
                Role::Write
            } else {
                Role::Read
            }
        }));
    }
    if owner_of(state, repo).await? == Some(account) {
        return Ok(Some(Role::Write));
    }
    let share = client(state)
        .await?
        .query_opt(
            "SELECT can_write FROM shares WHERE repo = $1 AND account = $2",
            &[&repo, &account],
        )
        .await?
        .map(|row| row.get::<_, bool>(0));
    Ok(share.map(|write| if write { Role::Write } else { Role::Read }))
}

/// Splits `projects/<uuid>/<rest>`.
fn parse_key(key: &str) -> Result<(Uuid, &str), ApiError> {
    check_key(key).map_err(|_| ApiError::BadRequest("invalid_key"))?;
    let rest = key
        .strip_prefix("projects/")
        .ok_or(ApiError::BadRequest("invalid_key"))?;
    let (id, name) = rest
        .split_once('/')
        .ok_or(ApiError::BadRequest("invalid_key"))?;
    let repo = Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("invalid_key"))?;
    Ok((repo, name))
}

/// Only what the sync stores, and only under its own checksum. So stored objects
/// cannot be overwritten with something else, and damaged uploads do not get in.
fn check_content(name: &str, body: &[u8]) -> Result<(), ApiError> {
    let hash = sha256_hex(body);
    let fits = name.strip_prefix("blobs/") == Some(hash.as_str())
        || name
            .strip_prefix("commits/")
            .and_then(|file| file.strip_suffix(".json"))
            == Some(hash.as_str());
    if fits {
        Ok(())
    } else {
        Err(ApiError::BadRequest("checksum_mismatch"))
    }
}

async fn put_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Body,
) -> Result<StatusCode, ApiError> {
    // The login comes first: nobody gets to make the server read a large body
    // without one.
    let account = authed(&state, &headers).await?;
    let (repo, name) = parse_key(&key)?;
    // Bodies are held in memory, so only so many at a time.
    let _permit = state
        .0
        .transfers
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let body = tokio::time::timeout(TRANSFER_TIMEOUT, to_bytes(body, MAX_OBJECT_BYTES))
        .await
        .map_err(|_| ApiError::BadRequest("too_slow"))?
        .map_err(|_| ApiError::Quota)?;
    check_content(name, &body)?;
    let mut db = client(&state).await?;
    db.execute(
        "INSERT INTO repos (id, owner) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING",
        &[&repo, &account],
    )
    .await?;
    if role_of(&state, repo, account).await? != Some(Role::Write) {
        return Err(ApiError::Forbidden);
    }
    let exists = db
        .query_opt(
            "SELECT size FROM objects WHERE repo = $1 AND name = $2",
            &[&repo, &name],
        )
        .await?;
    // The same name means the same content, so a repeated upload is already done.
    if exists.is_some() {
        return Ok(StatusCode::OK);
    }
    let size = i64::try_from(body.len()).map_err(|_| ApiError::Quota)?;
    let owner = owner_of(&state, repo).await?.ok_or(ApiError::NotFound)?;
    let room: bool = db
        .query_one(
            "SELECT used_bytes + $2 <= quota_bytes FROM accounts WHERE id = $1",
            &[&owner, &size],
        )
        .await?
        .get(0);
    if !room {
        return Err(ApiError::Quota);
    }
    let team_context = team::project_role(&db, repo, account).await?;
    let changed_areas = match &team_context {
        Some((_, Some(role))) if name.starts_with("commits/") => {
            team::check_commit(&state, repo, account, *role, &body).await?
        }
        _ => Vec::new(),
    };
    // Content first: metadata naming missing content would be a broken version.
    // An upload that loses a race below only leaves an unlisted file.
    let worker = state.clone();
    let store_key = format!("{repo}/{name}");
    blocking(move || worker.0.store.provider().put(&store_key, &body)).await??;

    let transaction = db.transaction().await?;
    let room: bool = transaction
        .query_one(
            "SELECT used_bytes + $2 <= quota_bytes FROM accounts WHERE id = $1 FOR UPDATE",
            &[&owner, &size],
        )
        .await?
        .get(0);
    if !room {
        return Err(ApiError::Quota);
    }
    let inserted = transaction
        .execute(
            "INSERT INTO objects (repo, name, size) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            &[&repo, &name, &size],
        )
        .await?;
    if inserted == 1 {
        transaction
            .execute(
                "UPDATE accounts SET used_bytes = used_bytes + $2 WHERE id = $1",
                &[&owner, &size],
            )
            .await?;
        if name.starts_with("commits/") {
            let email: String = transaction
                .query_one("SELECT email FROM accounts WHERE id = $1", &[&account])
                .await?
                .get(0);
            audit::record(
                &transaction,
                Entry {
                    actor: Some(account),
                    actor_email: &email,
                    team: team_context.as_ref().map(|(team, _)| *team),
                    repo: Some(repo),
                    action: "version.add",
                    detail: &changed_areas.join(","),
                },
            )
            .await?;
            let event = json!({"repo": repo, "name": name}).to_string();
            transaction
                .execute("SELECT pg_notify($1, $2)", &[&EVENT_CHANNEL, &event])
                .await?;
        }
    }
    transaction.commit().await?;
    Ok(StatusCode::CREATED)
}

async fn get_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let (repo, name) = parse_key(&key)?;
    if owner_of(&state, repo).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    if role_of(&state, repo, account).await?.is_none() {
        return Err(ApiError::Forbidden);
    }
    let listed = client(&state)
        .await?
        .query_opt(
            "SELECT size FROM objects WHERE repo = $1 AND name = $2",
            &[&repo, &name],
        )
        .await?;
    if listed.is_none() {
        return Err(ApiError::NotFound);
    }
    let _permit = state
        .0
        .transfers
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let worker = state.clone();
    let store_key = format!("{repo}/{name}");
    let bytes = blocking(move || worker.0.store.provider().get(&store_key))
        .await??
        .ok_or_else(|| ApiError::Internal(format!("content missing: {repo}")))?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

async fn list_objects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(prefix): Path<String>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let (repo, name) = parse_key(&prefix)?;
    if owner_of(&state, repo).await?.is_none() {
        return Ok(Json(json!({"keys": []})).into_response());
    }
    if role_of(&state, repo, account).await?.is_none() {
        return Err(ApiError::Forbidden);
    }
    let keys: Vec<String> = client(&state)
        .await?
        .query(
            "SELECT name FROM objects WHERE repo = $1 AND starts_with(name, $2) ORDER BY name",
            &[&repo, &format!("{name}/")],
        )
        .await?
        .iter()
        .map(|row| format!("projects/{repo}/{}", row.get::<_, String>(0)))
        .collect();
    Ok(Json(json!({"keys": keys})).into_response())
}

#[derive(Serialize)]
struct RepoEntry {
    id: Uuid,
    owned: bool,
    can_write: bool,
    bytes: i64,
}

async fn list_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let list: Vec<RepoEntry> = client(&state)
        .await?
        .query(
            "SELECT r.id, r.owner = $1, r.owner = $1 OR coalesce(s.can_write, false),
                    coalesce((SELECT sum(size)::bigint FROM objects o WHERE o.repo = r.id), 0)
             FROM repos r LEFT JOIN shares s ON s.repo = r.id AND s.account = $1
             WHERE r.owner = $1 OR s.account = $1 ORDER BY r.id",
            &[&account],
        )
        .await?
        .iter()
        .map(|row| RepoEntry {
            id: row.get(0),
            owned: row.get(1),
            can_write: row.get(2),
            bytes: row.get(3),
        })
        .collect();
    Ok(Json(json!({"repos": list})).into_response())
}

#[derive(Deserialize)]
struct ShareRequest {
    email: String,
    can_write: bool,
}

async fn require_owner(state: &AppState, repo: Uuid, account: Uuid) -> Result<(), ApiError> {
    if owner_of(state, repo).await? == Some(account) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<ShareRequest>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    require_owner(&state, repo, account).await?;
    let email = normalize_email(&body.email)?;
    let mut db = client(&state).await?;
    if team::project_role(&db, repo, account).await?.is_some() {
        return Err(ApiError::Conflict("team_project"));
    }
    let target: Uuid = db
        .query_opt("SELECT id FROM accounts WHERE email = $1", &[&email])
        .await?
        .ok_or(ApiError::NotFound)?
        .get(0);
    let actor: String = db
        .query_one("SELECT email FROM accounts WHERE id = $1", &[&account])
        .await?
        .get(0);
    let transaction = db.transaction().await?;
    transaction
        .execute(
            "INSERT INTO shares (repo, account, can_write) VALUES ($1, $2, $3)
             ON CONFLICT (repo, account) DO UPDATE SET can_write = $3",
            &[&repo, &target, &body.can_write],
        )
        .await?;
    audit::record(
        &transaction,
        Entry {
            actor: Some(account),
            actor_email: &actor,
            repo: Some(repo),
            action: "share.set",
            detail: &format!("{email}: write={}", body.can_write),
            ..Entry::default()
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct UnshareRequest {
    email: String,
}

async fn unshare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<UnshareRequest>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    require_owner(&state, repo, account).await?;
    let email = normalize_email(&body.email)?;
    let mut db = client(&state).await?;
    let actor: String = db
        .query_one("SELECT email FROM accounts WHERE id = $1", &[&account])
        .await?
        .get(0);
    let transaction = db.transaction().await?;
    transaction
        .execute(
            "DELETE FROM shares WHERE repo = $1 AND account = (SELECT id FROM accounts WHERE email = $2)",
            &[&repo, &email],
        )
        .await?;
    audit::record(
        &transaction,
        Entry {
            actor: Some(account),
            actor_email: &actor,
            repo: Some(repo),
            action: "share.remove",
            detail: &email,
            ..Entry::default()
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let session = token_hash(token);
    Ok(upgrade.on_upgrade(move |socket| forward_events(socket, state, account, session)))
}

/// Sends the names of new versions in projects this account can read. It says that
/// something changed, not what: content still goes through the checks above. Access
/// and the login are asked anew for each event, so that ending a share or logging
/// out ends the events on connections that are already open.
async fn forward_events(mut socket: WebSocket, state: AppState, account: Uuid, session: Vec<u8>) {
    let mut receiver = state.0.events.subscribe();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                if !matches!(incoming, Some(Ok(_))) {
                    return;
                }
            }
            event = receiver.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                let repo = serde_json::from_str::<serde_json::Value>(&event)
                    .ok()
                    .and_then(|value| value["repo"].as_str().and_then(|id| Uuid::parse_str(id).ok()));
                let Some(repo) = repo else { continue };
                let Ok(database) = client(&state).await else { continue };
                let valid = database
                    .query_opt(
                        "SELECT 1 FROM sessions WHERE token_hash = $1 AND expires > now()",
                        &[&session],
                    )
                    .await;
                drop(database);
                if !matches!(valid, Ok(Some(_))) {
                    return;
                }
                if matches!(role_of(&state, repo, account).await, Ok(Some(_)))
                    && socket.send(Message::text(event)).await.is_err()
                {
                    return;
                }
            }
        }
    }
}

async fn health(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    client(&state).await?.execute("SELECT 1", &[]).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Passes on notifications of all instances. Comes back after losing the database.
async fn listen_for_events(config: tokio_postgres::Config, events: broadcast::Sender<String>) {
    loop {
        if let Ok((listener, mut connection)) = config.connect(NoTls).await {
            let mut messages =
                futures_util::stream::poll_fn(move |context| connection.poll_message(context));
            let (ready, wait) = tokio::sync::oneshot::channel::<()>();
            let forward = async {
                let mut ready = Some(ready);
                while let Some(Ok(message)) = messages.next().await {
                    if let AsyncMessage::Notification(notification) = message {
                        let _ = events.send(notification.payload().to_owned());
                    }
                    if let Some(signal) = ready.take() {
                        let _ = signal.send(());
                    }
                }
            };
            let subscribe = async {
                let _ = listener
                    .batch_execute(&format!("LISTEN {EVENT_CHANNEL}"))
                    .await;
                let _ = wait.await;
                std::future::pending::<()>().await;
            };
            tokio::select! {
                () = forward => {}
                () = subscribe => {}
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Brings the database to this version's schema, once.
///
/// Only when the schema differs from the one applied last: changing tables needs
/// exclusive locks, and other instances at work would deadlock against that.
/// While a new version does change it, such a deadlock can still happen, so that
/// is tried again.
async fn prepare_schema(db: &deadpool_postgres::Pool) -> Result<(), String> {
    const ATTEMPTS: u64 = 8;
    let describe = |error: &tokio_postgres::Error| match error.as_db_error() {
        Some(found) => format!("{}: {}", found.code().code(), found.message()),
        None => error.to_string(),
    };
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut connection = db.get().await.map_err(|error| error.to_string())?;
        match apply_schema(&mut connection).await {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < ATTEMPTS
                    && error.code()
                        == Some(&tokio_postgres::error::SqlState::T_R_DEADLOCK_DETECTED) =>
            {
                tokio::time::sleep(Duration::from_millis(100 * attempt)).await;
            }
            Err(error) => return Err(describe(&error)),
        }
    }
}

async fn apply_schema(
    connection: &mut deadpool_postgres::Object,
) -> Result<(), tokio_postgres::Error> {
    let transaction = connection.transaction().await?;
    // A lock, because several instances may start at once.
    transaction
        .batch_execute(
            "SELECT pg_advisory_xact_lock(727274);
             CREATE TABLE IF NOT EXISTS schema_applied (
                 hash text PRIMARY KEY,
                 applied timestamptz NOT NULL DEFAULT now()
             );",
        )
        .await?;
    let hash = tronic_storage::sha256_hex(SCHEMA.as_bytes());
    let known = transaction
        .query_opt("SELECT 1 FROM schema_applied WHERE hash = $1", &[&hash])
        .await?
        .is_some();
    if !known {
        transaction.batch_execute(SCHEMA).await?;
        transaction
            .execute("INSERT INTO schema_applied (hash) VALUES ($1)", &[&hash])
            .await?;
    }
    transaction.commit().await
}

/// Prepares the database and starts listening for events of all instances.
pub async fn connect(database: &str, store: Store, config: Config) -> Result<AppState, String> {
    let settings: tokio_postgres::Config = database
        .parse()
        .map_err(|_| "invalid DATABASE_URL".to_owned())?;
    let manager = deadpool_postgres::Manager::new(settings.clone(), NoTls);
    let db = deadpool_postgres::Pool::builder(manager)
        .max_size(32)
        .build()
        .map_err(|error| error.to_string())?;
    prepare_schema(&db).await?;
    let cleaner = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CLEANUP_INTERVAL).await;
            if let Ok(connection) = cleaner.get().await {
                let _ = connection
                    .batch_execute(
                        "DELETE FROM sessions WHERE expires < now();
                         DELETE FROM failed_logins WHERE at < now() - interval '1 day';",
                    )
                    .await;
            }
        }
    });
    let (events, _) = broadcast::channel(1024);
    tokio::spawn(listen_for_events(settings, events.clone()));
    let dummy_hash = hash_password("not a real password, only used for equal timing")
        .map_err(|_| "hashing failed".to_owned())?;
    Ok(AppState(Arc::new(Inner {
        db,
        store,
        config,
        events,
        hashing: tokio::sync::Semaphore::new(MAX_HASHING),
        transfers: tokio::sync::Semaphore::new(MAX_TRANSFERS),
        dummy_hash,
    })))
}

/// Adds an account directly, for setting up a server.
pub async fn add_account(state: &AppState, email: &str, password: &str) -> Result<(), ApiError> {
    let email = normalize_email(email)?;
    check_password(password)?;
    let password = password.to_owned();
    let hash = blocking(move || hash_password(&password)).await??;
    let id = Uuid::new_v4();
    let mut db = client(state).await?;
    let transaction = db.transaction().await?;
    let inserted = transaction
        .execute(
            "INSERT INTO accounts (id, email, password_hash, quota_bytes) VALUES ($1, $2, $3, $4) ON CONFLICT (email) DO NOTHING",
            &[&id, &email, &hash, &state.0.config.default_quota],
        )
        .await?;
    if inserted == 0 {
        return Err(ApiError::Conflict("email_taken"));
    }
    audit::record(
        &transaction,
        Entry {
            actor: Some(id),
            actor_email: &email,
            action: "account.create",
            detail: "by the server administrator",
            ..Entry::default()
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

/// Reads a small request completely before anything answers. Handlers that need no
/// body would otherwise answer early, and the client, still sending, can then lose
/// the connection and see an error for a request that worked.
async fn read_small_body(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let (parts, body) = request.into_parts();
    match to_bytes(body, MAX_JSON_BYTES).await {
        Ok(bytes) => {
            next.run(axum::extract::Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(_) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    }
}

pub fn router(state: AppState) -> Router {
    let small = Router::new()
        .route("/healthz", get(health))
        .route("/v1/auth/register", post(register))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/account", get(account).delete(delete_account))
        .route("/v1/repos", get(list_repos))
        .route("/v1/repos/{repo}/shares", post(share).delete(unshare))
        .route("/v1/list/{*prefix}", get(list_objects))
        .route("/v1/events", get(events))
        .merge(team::routes())
        .layer(axum::middleware::from_fn(read_small_body));
    Router::new()
        .route(
            "/v1/objects/{*key}",
            put(put_object)
                .get(get_object)
                .layer(DefaultBodyLimit::disable()),
        )
        .merge(small)
        .layer(DefaultBodyLimit::max(MAX_JSON_BYTES))
        .with_state(state)
}
