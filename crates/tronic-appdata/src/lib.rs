//! Per-user application data in a local SQLite database.
//!
//! Holds settings and the list of recent projects. Project content is never
//! stored here. Everything in this database can be rebuilt, so a damaged file
//! is moved aside and replaced instead of blocking the application start.

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use std::path::Path;
use thiserror::Error;

/// Number of entries kept in the recent projects list.
pub const MAX_RECENT: i64 = 20;

const MIGRATIONS: &[&str] = &["
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    CREATE TABLE recent_projects (
        path      TEXT PRIMARY KEY NOT NULL,
        name      TEXT NOT NULL,
        opened_at TEXT NOT NULL,
        seq       INTEGER NOT NULL
    );
"];

#[derive(Debug, Error)]
pub enum AppDataError {
    #[error("application database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("application database could not be moved aside: {0}")]
    Quarantine(std::io::Error),
    #[error(
        "application database has schema version {found}, this build supports up to {supported}"
    )]
    NewerSchema { found: i64, supported: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    /// RFC 3339 timestamp, as supplied by the caller.
    pub opened_at: String,
}

pub struct AppData {
    connection: Connection,
}

impl AppData {
    /// Opens or creates the database. A damaged file is renamed to
    /// `<name>.corrupt` and a new database takes its place.
    pub fn open(path: &Path) -> Result<Self, AppDataError> {
        match Self::try_open(path) {
            Err(AppDataError::Database(error)) if is_corruption(&error) => {
                let mut quarantine = path.as_os_str().to_owned();
                quarantine.push(".corrupt");
                std::fs::rename(path, quarantine).map_err(AppDataError::Quarantine)?;
                Self::try_open(path)
            }
            other => other,
        }
    }

    /// Writes a consistent copy of the database to a new file.
    pub fn snapshot(&self, target: &Path) -> Result<(), AppDataError> {
        self.connection
            .execute("VACUUM INTO ?1", [target.to_string_lossy()])?;
        Ok(())
    }

    /// Opens a database that lives in memory only.
    pub fn in_memory() -> Result<Self, AppDataError> {
        Self::prepared(Connection::open_in_memory()?)
    }

    fn try_open(path: &Path) -> Result<Self, AppDataError> {
        Self::prepared(Connection::open(path)?)
    }

    fn prepared(mut connection: Connection) -> Result<Self, AppDataError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let mut schema: i64 = 0;
        for migration in MIGRATIONS {
            schema += 1;
            if schema > version {
                let transaction = connection.transaction()?;
                transaction.execute_batch(migration)?;
                transaction.pragma_update(None, "user_version", schema)?;
                transaction.commit()?;
            }
        }
        if version > schema {
            return Err(AppDataError::NewerSchema {
                found: version,
                supported: schema,
            });
        }
        Ok(Self { connection })
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, AppDataError> {
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), AppDataError> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Moves a project to the top of the recent list.
    pub fn touch_recent(
        &self,
        path: &str,
        name: &str,
        opened_at: &str,
    ) -> Result<(), AppDataError> {
        self.connection.execute(
            "INSERT INTO recent_projects (path, name, opened_at, seq)
             VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(seq), 0) + 1 FROM recent_projects))
             ON CONFLICT (path) DO UPDATE SET
               name = excluded.name,
               opened_at = excluded.opened_at,
               seq = excluded.seq",
            params![path, name, opened_at],
        )?;
        self.connection.execute(
            "DELETE FROM recent_projects WHERE path NOT IN
             (SELECT path FROM recent_projects ORDER BY seq DESC LIMIT ?1)",
            params![MAX_RECENT],
        )?;
        Ok(())
    }

    pub fn recent(&self) -> Result<Vec<RecentProject>, AppDataError> {
        let mut statement = self
            .connection
            .prepare("SELECT path, name, opened_at FROM recent_projects ORDER BY seq DESC")?;
        let rows = statement.query_map([], |row| {
            Ok(RecentProject {
                path: row.get(0)?,
                name: row.get(1)?,
                opened_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn remove_recent(&self, path: &str) -> Result<(), AppDataError> {
        self.connection
            .execute("DELETE FROM recent_projects WHERE path = ?1", [path])?;
        Ok(())
    }
}

fn is_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn a_snapshot_holds_the_settings_and_can_be_opened() {
        let directory = tempfile::tempdir().unwrap();
        let db = AppData::open(&directory.path().join("a.db")).unwrap();
        db.set_setting("language", "de-DE").unwrap();
        let copy = directory.path().join("copy.db");
        db.snapshot(&copy).unwrap();
        db.set_setting("language", "fr-FR").unwrap();
        let opened = AppData::open(&copy).unwrap();
        assert_eq!(
            opened.setting("language").unwrap().as_deref(),
            Some("de-DE")
        );
        // A snapshot never overwrites anything.
        assert!(db.snapshot(&copy).is_err());
    }

    #[test]
    fn settings_round_trip_and_update() {
        let db = AppData::in_memory().unwrap();
        assert_eq!(db.setting("language").unwrap(), None);
        db.set_setting("language", "de-DE").unwrap();
        db.set_setting("language", "zh-CN").unwrap();
        assert_eq!(db.setting("language").unwrap().as_deref(), Some("zh-CN"));
    }

    #[test]
    fn data_survives_reopening() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        AppData::open(&path)
            .unwrap()
            .set_setting("language", "fr-FR")
            .unwrap();
        assert_eq!(
            AppData::open(&path)
                .unwrap()
                .setting("language")
                .unwrap()
                .as_deref(),
            Some("fr-FR")
        );
    }

    #[test]
    fn recent_orders_by_last_use_without_duplicates() {
        let db = AppData::in_memory().unwrap();
        db.touch_recent("/a", "A", "t1").unwrap();
        db.touch_recent("/b", "B", "t2").unwrap();
        db.touch_recent("/a", "A neu", "t3").unwrap();
        let recent = db.recent().unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/a");
        assert_eq!(recent[0].name, "A neu");
        assert_eq!(recent[0].opened_at, "t3");
        assert_eq!(recent[1].path, "/b");
    }

    #[test]
    fn recent_is_limited_and_drops_the_oldest() {
        let db = AppData::in_memory().unwrap();
        for i in 0..MAX_RECENT + 5 {
            db.touch_recent(&format!("/{i}"), "x", "t").unwrap();
        }
        let recent = db.recent().unwrap();
        assert_eq!(recent.len() as i64, MAX_RECENT);
        assert_eq!(recent[0].path, format!("/{}", MAX_RECENT + 4));
        assert!(recent.iter().all(|entry| entry.path != "/0"));
    }

    #[test]
    fn recent_entries_can_be_removed() {
        let db = AppData::in_memory().unwrap();
        db.touch_recent("/a", "A", "t").unwrap();
        db.remove_recent("/a").unwrap();
        db.remove_recent("/unknown").unwrap();
        assert!(db.recent().unwrap().is_empty());
    }

    #[test]
    fn damaged_database_is_moved_aside() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        let garbage = vec![0x5a_u8; 8192];
        std::fs::write(&path, &garbage).unwrap();

        let db = AppData::open(&path).unwrap();
        db.set_setting("language", "en-US").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("app.db.corrupt")).unwrap(),
            garbage
        );
    }

    #[test]
    fn database_from_a_newer_release_is_left_untouched() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let connection = Connection::open(&path).unwrap();
            connection.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(matches!(
            AppData::open(&path),
            Err(AppDataError::NewerSchema { found: 999, .. })
        ));
        assert!(path.exists());
    }
}
