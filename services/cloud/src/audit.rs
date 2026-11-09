//! The audit log: every change, written in the same transaction as the change.
//! Entries form a hash chain, and the database refuses to alter them.

use deadpool_postgres::GenericClient;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tronic_storage::sha256_hex;
use uuid::Uuid;

use crate::ApiError;

const CHAIN_LOCK: i64 = 727_275;

#[derive(Debug, Clone, Default)]
pub struct Entry<'a> {
    pub actor: Option<Uuid>,
    pub actor_email: &'a str,
    pub team: Option<Uuid>,
    pub repo: Option<Uuid>,
    pub action: &'a str,
    pub detail: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub seq: i64,
    pub at: String,
    pub actor: Option<Uuid>,
    pub actor_email: String,
    pub team: Option<Uuid>,
    pub repo: Option<Uuid>,
    pub action: String,
    pub detail: String,
    pub hash: String,
}

fn link(prev: &str, row: &[&str]) -> String {
    let mut text = prev.to_owned();
    for part in row {
        text.push('\u{1f}');
        text.push_str(part);
    }
    sha256_hex(text.as_bytes())
}

fn text_of(id: Option<Uuid>) -> String {
    id.map(|id| id.to_string()).unwrap_or_default()
}

/// Writes one entry. Must run inside the transaction of the change it records, so
/// that neither exists without the other.
pub async fn record<C: GenericClient>(client: &C, entry: Entry<'_>) -> Result<(), ApiError> {
    client
        .execute("SELECT pg_advisory_xact_lock($1)", &[&CHAIN_LOCK])
        .await?;
    let prev: String = client
        .query_opt("SELECT hash FROM audit ORDER BY seq DESC LIMIT 1", &[])
        .await?
        .map_or_else(String::new, |row| row.get(0));
    let at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let hash = link(
        &prev,
        &[
            &at,
            &text_of(entry.actor),
            entry.actor_email,
            &text_of(entry.team),
            &text_of(entry.repo),
            entry.action,
            entry.detail,
        ],
    );
    client
        .execute(
            "INSERT INTO audit (at, actor, actor_email, team, repo, action, detail, hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &at,
                &entry.actor,
                &entry.actor_email,
                &entry.team,
                &entry.repo,
                &entry.action,
                &entry.detail,
                &hash,
            ],
        )
        .await?;
    Ok(())
}

fn row_of(row: &tokio_postgres::Row) -> Row {
    Row {
        seq: row.get(0),
        at: row.get(1),
        actor: row.get(2),
        actor_email: row.get(3),
        team: row.get(4),
        repo: row.get(5),
        action: row.get(6),
        detail: row.get(7),
        hash: row.get(8),
    }
}

const COLUMNS: &str = "seq, at, actor, actor_email, team, repo, action, detail, hash";

pub async fn list_team<C: GenericClient>(client: &C, team: Uuid) -> Result<Vec<Row>, ApiError> {
    Ok(client
        .query(
            &format!("SELECT {COLUMNS} FROM audit WHERE team = $1 ORDER BY seq DESC LIMIT 2000"),
            &[&team],
        )
        .await?
        .iter()
        .map(row_of)
        .collect())
}

/// Recalculates the whole chain. Returns the first entry that does not fit.
pub async fn verify<C: GenericClient>(client: &C) -> Result<Option<i64>, ApiError> {
    let rows = client
        .query(&format!("SELECT {COLUMNS} FROM audit ORDER BY seq"), &[])
        .await?;
    let mut prev = String::new();
    for row in rows.iter().map(row_of) {
        let expected = link(
            &prev,
            &[
                &row.at,
                &text_of(row.actor),
                &row.actor_email,
                &text_of(row.team),
                &text_of(row.repo),
                &row.action,
                &row.detail,
            ],
        );
        if expected != row.hash {
            return Ok(Some(row.seq));
        }
        prev = row.hash;
    }
    Ok(None)
}
