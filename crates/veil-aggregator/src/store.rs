//! SQLite-backed session store for the aggregator.
//!
//! Owns the database connection, manages schema migrations, provides CRUD
//! operations, and supports FTS5 full-text search over session metadata.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionSearchResult, SessionStatus};

/// Wraps a `rusqlite::Connection` with session-specific operations.
pub struct SessionStore {
    conn: rusqlite::Connection,
}

/// Errors that can occur during session store operations.
#[derive(Debug)]
pub enum StoreError {
    /// Database engine error.
    Sqlite(rusqlite::Error),
    /// Database file I/O error.
    Io(std::io::Error),
    /// Data conversion/serialization error.
    DataError(String),
    /// Schema migration failed.
    MigrationError(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "SQLite error: {err}"),
            Self::Io(err) => write!(f, "database I/O error: {err}"),
            Self::DataError(msg) => write!(f, "data conversion error: {msg}"),
            Self::MigrationError(msg) => write!(f, "schema migration failed: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::DataError(_) | Self::MigrationError(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// SQL for inserting or replacing a session row.
const UPSERT_SQL: &str = "INSERT OR REPLACE INTO sessions
    (id, agent, title, working_dir, branch, pr_number, pr_url,
     plan_content, status, started_at, ended_at, indexed_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";

/// Build the parameter array for a session upsert statement.
fn upsert_params(entry: &SessionEntry) -> [Box<dyn rusqlite::types::ToSql>; 12] {
    [
        Box::new(entry.id.as_str().to_string()),
        Box::new(entry.agent.to_string()),
        Box::new(entry.title.clone()),
        Box::new(entry.working_dir.to_string_lossy().to_string()),
        Box::new(entry.branch.clone()),
        Box::new(entry.pr_number.map(u64::cast_signed)),
        Box::new(entry.pr_url.clone()),
        Box::new(entry.plan_content.clone()),
        Box::new(entry.status.to_string()),
        Box::new(entry.started_at.to_rfc3339()),
        Box::new(entry.ended_at.map(|dt| dt.to_rfc3339())),
        Box::new(entry.indexed_at.to_rfc3339()),
    ]
}

/// Parse an RFC 3339 timestamp string into a `DateTime<Utc>`, mapping
/// parse failures into `rusqlite::Error` for use inside row-mapping closures.
fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>, rusqlite::Error> {
    DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl SessionStore {
    /// Open or create the database at the given path.
    /// Runs migrations on first open.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open(path)?;
        let store = Self { conn };
        store.run_migrations()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        let store = Self { conn };
        store.run_migrations()?;
        Ok(store)
    }

    /// Run schema migrations. Creates tables if they don't exist.
    fn run_migrations(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| StoreError::MigrationError(e.to_string()))?;
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS sessions (
                id              TEXT PRIMARY KEY,
                agent           TEXT NOT NULL,
                title           TEXT NOT NULL,
                working_dir     TEXT NOT NULL,
                branch          TEXT,
                pr_number       INTEGER,
                pr_url          TEXT,
                plan_content    TEXT,
                status          TEXT NOT NULL DEFAULT 'unknown',
                started_at      TEXT NOT NULL,
                ended_at        TEXT,
                indexed_at      TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                title,
                first_message,
                content='',
                tokenize='porter unicode61'
            );
        ",
            )
            .map_err(|e| StoreError::MigrationError(e.to_string()))?;
        Ok(())
    }

    /// Insert or update a session entry.
    /// Uses INSERT OR REPLACE -- the session ID is the natural key.
    pub fn upsert_session(&self, entry: &SessionEntry) -> Result<(), StoreError> {
        let params = upsert_params(entry);
        self.conn.execute(UPSERT_SQL, params)?;
        Ok(())
    }

    /// Batch upsert multiple sessions in a single transaction.
    pub fn upsert_sessions(&self, entries: &[SessionEntry]) -> Result<usize, StoreError> {
        if entries.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        for entry in entries {
            let params = upsert_params(entry);
            tx.execute(UPSERT_SQL, params)?;
        }
        let count = entries.len();
        tx.commit()?;
        Ok(count)
    }

    /// Parse a row from the sessions table into a `SessionEntry`.
    fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<SessionEntry, rusqlite::Error> {
        let id: String = row.get("id")?;
        let agent_str: String = row.get("agent")?;
        let title: String = row.get("title")?;
        let working_dir_str: String = row.get("working_dir")?;
        let branch: Option<String> = row.get("branch")?;
        let pr_number: Option<i64> = row.get("pr_number")?;
        let pr_url: Option<String> = row.get("pr_url")?;
        let plan_content: Option<String> = row.get("plan_content")?;
        let status_str: String = row.get("status")?;
        let started_at_str: String = row.get("started_at")?;
        let ended_at_str: Option<String> = row.get("ended_at")?;
        let indexed_at_str: String = row.get("indexed_at")?;

        let agent = AgentKind::from_str(&agent_str).unwrap_or(AgentKind::Unknown(agent_str));
        let status = SessionStatus::from_str(&status_str).unwrap_or_default();

        let started_at = parse_rfc3339(&started_at_str)?;
        let ended_at = ended_at_str.map(|s| parse_rfc3339(&s)).transpose()?;
        let indexed_at = parse_rfc3339(&indexed_at_str)?;

        Ok(SessionEntry {
            id: SessionId::new(id),
            agent,
            title,
            working_dir: PathBuf::from(working_dir_str),
            branch,
            pr_number: pr_number.map(i64::cast_unsigned),
            pr_url,
            plan_content,
            status,
            started_at,
            ended_at,
            indexed_at,
        })
    }

    /// Retrieve a session by ID.
    pub fn get_session(&self, id: &SessionId) -> Result<Option<SessionEntry>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT * FROM sessions WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id.as_str()])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::row_to_entry(row)?)),
            None => Ok(None),
        }
    }

    /// List all sessions, ordered by `started_at` descending.
    pub fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT * FROM sessions ORDER BY started_at DESC")?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// List sessions filtered by agent kind.
    pub fn list_sessions_by_agent(
        &self,
        agent: &AgentKind,
    ) -> Result<Vec<SessionEntry>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM sessions WHERE agent = ?1 ORDER BY started_at DESC")?;
        let rows = stmt.query_map(rusqlite::params![agent.to_string()], Self::row_to_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Sanitize a user-provided query string for safe use in FTS5 MATCH.
    /// Strips FTS5 operators and special characters, keeping alphanumeric
    /// characters plus interior hyphens and underscores (which are valid in
    /// tokens like "pre-commit" or "`foo_bar`"). Leading/trailing hyphens are
    /// trimmed to prevent FTS5 operator injection (e.g. `--` as NOT).
    /// Tokens are joined with spaces (implicit AND).
    fn sanitize_fts_query(query: &str) -> String {
        query
            .split_whitespace()
            .filter_map(|word| {
                let cleaned: String = word
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                // Trim leading/trailing hyphens to avoid FTS5 operator injection
                // (e.g. "--" is the NOT operator in FTS5).
                let trimmed = cleaned.trim_matches('-');
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Full-text search across session titles and first messages.
    pub fn search_sessions(&self, query: &str) -> Result<Vec<SessionSearchResult>, StoreError> {
        let sanitized = Self::sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(vec![]);
        }

        // First, query the FTS table for matching rowids and their ranks.
        // We query the FTS table directly so the `rank` column is available.
        let mut fts_stmt = self.conn.prepare(
            "SELECT rowid, rank FROM sessions_fts WHERE sessions_fts MATCH ?1 ORDER BY rank",
        )?;
        let fts_rows = fts_stmt.query_map(rusqlite::params![sanitized], |row| {
            let rowid: i64 = row.get(0)?;
            let rank: f64 = row.get(1)?;
            Ok((rowid, rank))
        })?;

        let mut ranked: Vec<(i64, f64)> = Vec::new();
        for row in fts_rows {
            ranked.push(row?);
        }

        // Now load the full session entries for each matching rowid.
        let mut results = Vec::new();
        let mut sess_stmt = self.conn.prepare("SELECT * FROM sessions WHERE rowid = ?1")?;
        for (rowid, rank) in &ranked {
            let mut rows = sess_stmt.query(rusqlite::params![rowid])?;
            if let Some(row) = rows.next()? {
                let entry = Self::row_to_entry(row)?;
                // FTS5 rank is negative (more negative = more relevant), negate for
                // a positive relevance score where higher = better.
                let relevance = -rank;
                results.push(SessionSearchResult { entry, relevance, snippet: None });
            }
        }
        Ok(results)
    }

    /// Delete a session by ID.
    ///
    /// Also attempts to remove the corresponding FTS index entry so that
    /// `search_sessions` doesn't return stale results.
    ///
    /// NOTE: contentless FTS5 requires the *exact* original indexed values
    /// for its delete command. We can retrieve `title` from the sessions
    /// table, but `first_message` is only stored inside the FTS index
    /// itself (not in sessions). When `first_message` was non-empty at
    /// index time, the FTS delete will fail because the values don't
    /// match. In that case we silently skip FTS cleanup — the stale
    /// entry is harmless because `search_sessions` already skips
    /// rowids whose session row no longer exists.
    pub fn delete_session(&self, id: &SessionId) -> Result<bool, StoreError> {
        // Grab the rowid and title before deleting — needed for FTS cleanup.
        let fts_info: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT rowid, title FROM sessions WHERE id = ?1",
                rusqlite::params![id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let affected = self
            .conn
            .execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id.as_str()])?;

        // Best-effort FTS cleanup. This succeeds when `first_message` was
        // empty at index time (the common case for title-only indexing) and
        // is silently skipped otherwise.
        if let Some((rowid, title)) = fts_info {
            let _ = self.conn.execute(
                "INSERT INTO sessions_fts(sessions_fts, rowid, title, first_message) VALUES('delete', ?1, ?2, '')",
                rusqlite::params![rowid, title],
            );
        }

        Ok(affected > 0)
    }

    /// Look up the integer rowid for a session by its string ID.
    fn get_rowid(&self, id: &SessionId) -> Result<i64, StoreError> {
        self.conn
            .query_row(
                "SELECT rowid FROM sessions WHERE id = ?1",
                rusqlite::params![id.as_str()],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// Update the FTS index for a session (title + first message text).
    ///
    /// Attempts to remove any prior FTS entry for this session before inserting,
    /// making it safe to call multiple times with the same values (idempotent).
    ///
    /// NOTE: Contentless FTS5 delete requires the *exact* original indexed
    /// values. The best-effort delete succeeds when called again with the same
    /// data (the common case) but cannot remove a prior entry if the content
    /// changed between calls. For full re-indexing, use [`clear_all`](Self::clear_all).
    pub fn update_fts(
        &self,
        id: &SessionId,
        title: &str,
        first_message: Option<&str>,
    ) -> Result<(), StoreError> {
        let rowid = self.get_rowid(id)?;
        let msg = first_message.unwrap_or("");

        // Best-effort delete of a prior FTS entry with these exact values.
        // Silently ignored if no matching entry exists.
        let _ = self.conn.execute(
            "INSERT INTO sessions_fts(sessions_fts, rowid, title, first_message) VALUES('delete', ?1, ?2, ?3)",
            rusqlite::params![rowid, title, msg],
        );

        self.conn.execute(
            "INSERT INTO sessions_fts(rowid, title, first_message) VALUES (?1, ?2, ?3)",
            rusqlite::params![rowid, title, msg],
        )?;
        Ok(())
    }

    /// Count total sessions, optionally filtered by agent.
    pub fn count_sessions(&self, agent: Option<&AgentKind>) -> Result<usize, StoreError> {
        let count: i64 = match agent {
            Some(a) => self.conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE agent = ?1",
                rusqlite::params![a.to_string()],
                |row| row.get(0),
            )?,
            None => self.conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?,
        };
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// Delete all sessions (used for cache rebuild scenarios).
    pub fn clear_all(&self) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM sessions", [])?;
        self.conn.execute("INSERT INTO sessions_fts(sessions_fts) VALUES('delete-all')", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;
    use veil_core::session::SessionStatus;

    fn make_entry(id: &str, agent: AgentKind, title: &str) -> SessionEntry {
        SessionEntry {
            id: SessionId::new(id),
            agent,
            title: title.to_string(),
            working_dir: PathBuf::from("/home/user/project"),
            branch: None,
            pr_number: None,
            pr_url: None,
            plan_content: None,
            status: SessionStatus::Active,
            started_at: Utc::now(),
            ended_at: None,
            indexed_at: Utc::now(),
        }
    }

    fn make_full_entry(id: &str) -> SessionEntry {
        SessionEntry {
            id: SessionId::new(id),
            agent: AgentKind::ClaudeCode,
            title: "Full entry test".to_string(),
            working_dir: PathBuf::from("/home/user/project"),
            branch: Some("feature/test".to_string()),
            pr_number: Some(42),
            pr_url: Some("https://github.com/org/repo/pull/42".to_string()),
            plan_content: Some("The plan content".to_string()),
            status: SessionStatus::Completed,
            started_at: Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap(),
            ended_at: Some(Utc.with_ymd_and_hms(2026, 1, 15, 11, 45, 0).unwrap()),
            indexed_at: Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap(),
        }
    }

    // ========================================================================
    // Schema tests
    // ========================================================================

    #[test]
    fn open_in_memory_succeeds() {
        let store = SessionStore::open_in_memory();
        assert!(store.is_ok(), "open_in_memory should succeed");
    }

    #[test]
    fn migrations_are_idempotent() {
        let store = SessionStore::open_in_memory().unwrap();
        // Running migrations again should not fail
        let result = store.run_migrations();
        assert!(result.is_ok(), "second migration run should succeed");
    }

    #[test]
    fn sessions_table_has_expected_columns() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut stmt = store.conn.prepare("PRAGMA table_info(sessions)").unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let expected = [
            "id",
            "agent",
            "title",
            "working_dir",
            "branch",
            "pr_number",
            "pr_url",
            "plan_content",
            "status",
            "started_at",
            "ended_at",
            "indexed_at",
        ];
        for col in &expected {
            assert!(
                columns.contains(&col.to_string()),
                "sessions table should have column '{col}', found: {columns:?}"
            );
        }
    }

    #[test]
    fn sessions_fts_virtual_table_exists() {
        let store = SessionStore::open_in_memory().unwrap();
        let result: Result<String, _> = store.conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='sessions_fts'",
            [],
            |row| row.get(0),
        );
        assert!(result.is_ok(), "sessions_fts virtual table should exist");
    }

    // ========================================================================
    // CRUD tests
    // ========================================================================

    #[test]
    fn upsert_and_get_session_round_trip() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_full_entry("round-trip-1");

        store.upsert_session(&entry).unwrap();
        let retrieved = store.get_session(&SessionId::new("round-trip-1")).unwrap();

        assert!(retrieved.is_some(), "should find the upserted session");
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id.as_str(), "round-trip-1");
        assert_eq!(retrieved.agent, AgentKind::ClaudeCode);
        assert_eq!(retrieved.title, "Full entry test");
        assert_eq!(retrieved.working_dir, PathBuf::from("/home/user/project"));
        assert_eq!(retrieved.branch.as_deref(), Some("feature/test"));
        assert_eq!(retrieved.pr_number, Some(42));
        assert_eq!(retrieved.pr_url.as_deref(), Some("https://github.com/org/repo/pull/42"));
        assert_eq!(retrieved.plan_content.as_deref(), Some("The plan content"));
        assert_eq!(retrieved.status, SessionStatus::Completed);
        assert_eq!(retrieved.started_at, Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap());
        assert_eq!(retrieved.ended_at, Some(Utc.with_ymd_and_hms(2026, 1, 15, 11, 45, 0).unwrap()));
    }

    #[test]
    fn upsert_with_same_id_updates_existing() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut entry = make_entry("update-test", AgentKind::ClaudeCode, "Original title");
        store.upsert_session(&entry).unwrap();

        entry.title = "Updated title".to_string();
        store.upsert_session(&entry).unwrap();

        let retrieved = store
            .get_session(&SessionId::new("update-test"))
            .unwrap()
            .expect("should find updated session");
        assert_eq!(retrieved.title, "Updated title");
    }

    #[test]
    fn get_session_nonexistent_returns_none() {
        let store = SessionStore::open_in_memory().unwrap();
        let result = store.get_session(&SessionId::new("does-not-exist")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_sessions_ordered_by_started_at_descending() {
        let store = SessionStore::open_in_memory().unwrap();

        let mut entry1 = make_entry("s1", AgentKind::ClaudeCode, "First");
        entry1.started_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

        let mut entry2 = make_entry("s2", AgentKind::ClaudeCode, "Second");
        entry2.started_at = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();

        let mut entry3 = make_entry("s3", AgentKind::ClaudeCode, "Third");
        entry3.started_at = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

        store.upsert_session(&entry1).unwrap();
        store.upsert_session(&entry2).unwrap();
        store.upsert_session(&entry3).unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 3);
        // Should be ordered: s2 (June), s3 (March), s1 (January)
        assert_eq!(sessions[0].id.as_str(), "s2");
        assert_eq!(sessions[1].id.as_str(), "s3");
        assert_eq!(sessions[2].id.as_str(), "s1");
    }

    #[test]
    fn list_sessions_empty_database_returns_empty() {
        let store = SessionStore::open_in_memory().unwrap();
        let sessions = store.list_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn list_sessions_by_agent_filters_correctly() {
        let store = SessionStore::open_in_memory().unwrap();
        store.upsert_session(&make_entry("cc1", AgentKind::ClaudeCode, "CC Session 1")).unwrap();
        store.upsert_session(&make_entry("codex1", AgentKind::Codex, "Codex Session")).unwrap();
        store.upsert_session(&make_entry("cc2", AgentKind::ClaudeCode, "CC Session 2")).unwrap();

        let cc_sessions = store.list_sessions_by_agent(&AgentKind::ClaudeCode).unwrap();
        assert_eq!(cc_sessions.len(), 2);
        for s in &cc_sessions {
            assert_eq!(s.agent, AgentKind::ClaudeCode);
        }

        let codex_sessions = store.list_sessions_by_agent(&AgentKind::Codex).unwrap();
        assert_eq!(codex_sessions.len(), 1);
        assert_eq!(codex_sessions[0].agent, AgentKind::Codex);
    }

    #[test]
    fn delete_session_existing_returns_true() {
        let store = SessionStore::open_in_memory().unwrap();
        store.upsert_session(&make_entry("del-me", AgentKind::ClaudeCode, "Delete me")).unwrap();

        let deleted = store.delete_session(&SessionId::new("del-me")).unwrap();
        assert!(deleted, "should return true for existing session");

        let after = store.get_session(&SessionId::new("del-me")).unwrap();
        assert!(after.is_none(), "session should be gone after delete");
    }

    #[test]
    fn delete_session_nonexistent_returns_false() {
        let store = SessionStore::open_in_memory().unwrap();
        let deleted = store.delete_session(&SessionId::new("never-existed")).unwrap();
        assert!(!deleted, "should return false for nonexistent session");
    }

    #[test]
    fn count_sessions_no_filter() {
        let store = SessionStore::open_in_memory().unwrap();
        store.upsert_session(&make_entry("c1", AgentKind::ClaudeCode, "S1")).unwrap();
        store.upsert_session(&make_entry("c2", AgentKind::Codex, "S2")).unwrap();
        store.upsert_session(&make_entry("c3", AgentKind::Aider, "S3")).unwrap();

        let count = store.count_sessions(None).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn count_sessions_with_agent_filter() {
        let store = SessionStore::open_in_memory().unwrap();
        store.upsert_session(&make_entry("c1", AgentKind::ClaudeCode, "S1")).unwrap();
        store.upsert_session(&make_entry("c2", AgentKind::ClaudeCode, "S2")).unwrap();
        store.upsert_session(&make_entry("c3", AgentKind::Codex, "S3")).unwrap();

        let count = store.count_sessions(Some(&AgentKind::ClaudeCode)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn clear_all_removes_all_sessions() {
        let store = SessionStore::open_in_memory().unwrap();
        store.upsert_session(&make_entry("x1", AgentKind::ClaudeCode, "S1")).unwrap();
        store.upsert_session(&make_entry("x2", AgentKind::Codex, "S2")).unwrap();

        // Verify they were inserted first
        let before = store.count_sessions(None).unwrap();
        assert_eq!(before, 2, "should have 2 sessions before clear");

        store.clear_all().unwrap();

        let sessions = store.list_sessions().unwrap();
        assert!(sessions.is_empty(), "all sessions should be cleared");
        let count = store.count_sessions(None).unwrap();
        assert_eq!(count, 0);
    }

    // ========================================================================
    // Batch tests
    // ========================================================================

    #[test]
    fn upsert_sessions_inserts_multiple() {
        let store = SessionStore::open_in_memory().unwrap();
        let entries = vec![
            make_entry("b1", AgentKind::ClaudeCode, "Batch 1"),
            make_entry("b2", AgentKind::Codex, "Batch 2"),
            make_entry("b3", AgentKind::Aider, "Batch 3"),
        ];

        let count = store.upsert_sessions(&entries).unwrap();
        assert_eq!(count, 3);

        let all = store.list_sessions().unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn upsert_sessions_empty_slice_returns_zero() {
        let store = SessionStore::open_in_memory().unwrap();
        let count = store.upsert_sessions(&[]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn upsert_sessions_mix_of_new_and_existing() {
        let store = SessionStore::open_in_memory().unwrap();

        // Insert one first
        store.upsert_session(&make_entry("existing", AgentKind::ClaudeCode, "Original")).unwrap();

        // Batch upsert with the existing one (updated) and a new one
        let entries = vec![
            make_entry("existing", AgentKind::ClaudeCode, "Updated"),
            make_entry("new-one", AgentKind::Codex, "Brand New"),
        ];
        let count = store.upsert_sessions(&entries).unwrap();
        assert_eq!(count, 2);

        let existing =
            store.get_session(&SessionId::new("existing")).unwrap().expect("should exist");
        assert_eq!(existing.title, "Updated");

        let new = store.get_session(&SessionId::new("new-one")).unwrap().expect("should exist");
        assert_eq!(new.title, "Brand New");
    }

    // ========================================================================
    // FTS5 search tests
    // ========================================================================

    #[test]
    fn search_sessions_matching_title() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts1", AgentKind::ClaudeCode, "Fix authentication bug");
        store.upsert_session(&entry).unwrap();
        store.update_fts(&SessionId::new("fts1"), "Fix authentication bug", None).unwrap();

        let results = store.search_sessions("authentication").unwrap();
        assert!(!results.is_empty(), "should find session by title keyword");
        assert_eq!(results[0].entry.id.as_str(), "fts1");
    }

    #[test]
    fn search_sessions_matching_first_message() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts2", AgentKind::ClaudeCode, "Coding session");
        store.upsert_session(&entry).unwrap();
        store
            .update_fts(
                &SessionId::new("fts2"),
                "Coding session",
                Some("Help me refactor the database layer"),
            )
            .unwrap();

        let results = store.search_sessions("refactor database").unwrap();
        assert!(!results.is_empty(), "should find session by first message content");
    }

    #[test]
    fn search_sessions_no_matches_returns_empty() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts3", AgentKind::ClaudeCode, "Fix auth");
        store.upsert_session(&entry).unwrap();
        store.update_fts(&SessionId::new("fts3"), "Fix auth", None).unwrap();

        let results = store.search_sessions("quantum computing").unwrap();
        assert!(results.is_empty(), "should find no matches");
    }

    #[test]
    fn search_sessions_porter_stemming() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts4", AgentKind::ClaudeCode, "Running tests");
        store.upsert_session(&entry).unwrap();
        store.update_fts(&SessionId::new("fts4"), "Running tests", None).unwrap();

        // "run" should match "Running" via porter stemmer
        let results = store.search_sessions("run").unwrap();
        assert!(!results.is_empty(), "porter stemmer should match 'run' to 'Running'");
    }

    #[test]
    fn search_sessions_results_ordered_by_relevance() {
        let store = SessionStore::open_in_memory().unwrap();

        // Entry with "auth" in title only
        let e1 = make_entry("rel1", AgentKind::ClaudeCode, "Fix auth");
        store.upsert_session(&e1).unwrap();
        store.update_fts(&SessionId::new("rel1"), "Fix auth", None).unwrap();

        // Entry with "auth" in both title and message — should rank higher
        let e2 = make_entry("rel2", AgentKind::ClaudeCode, "Auth middleware");
        store.upsert_session(&e2).unwrap();
        store
            .update_fts(
                &SessionId::new("rel2"),
                "Auth middleware",
                Some("Fix the auth token validation"),
            )
            .unwrap();

        let results = store.search_sessions("auth").unwrap();
        assert!(results.len() >= 2, "should find both sessions, got {}", results.len());
        // The one with more matches should have higher relevance
        assert!(
            results[0].relevance >= results[1].relevance,
            "results should be ordered by relevance"
        );
    }

    #[test]
    fn update_fts_and_search_round_trip() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts-rt", AgentKind::ClaudeCode, "Widget refactor");
        store.upsert_session(&entry).unwrap();
        store
            .update_fts(
                &SessionId::new("fts-rt"),
                "Widget refactor",
                Some("Restructure the widget module for better modularity"),
            )
            .unwrap();

        let results = store.search_sessions("widget").unwrap();
        assert!(!results.is_empty(), "FTS round-trip should work");
        assert_eq!(results[0].entry.id.as_str(), "fts-rt");
    }

    #[test]
    fn search_sessions_empty_query_returns_empty() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts-empty", AgentKind::ClaudeCode, "Some session");
        store.upsert_session(&entry).unwrap();
        store.update_fts(&SessionId::new("fts-empty"), "Some session", None).unwrap();

        let results = store.search_sessions("").unwrap();
        assert!(results.is_empty(), "empty query should return no results");
    }

    #[test]
    fn search_sessions_handles_special_characters() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("fts-special", AgentKind::ClaudeCode, "Normal session");
        store.upsert_session(&entry).unwrap();
        store.update_fts(&SessionId::new("fts-special"), "Normal session", None).unwrap();

        // These should not cause SQL injection or panics
        let result = store.search_sessions("'; DROP TABLE sessions; --");
        assert!(result.is_ok(), "special chars should not cause SQL error");

        let result = store.search_sessions("\"unmatched quote");
        assert!(result.is_ok(), "unmatched quotes should not cause SQL error");
    }

    // ========================================================================
    // Data integrity tests
    // ========================================================================

    #[test]
    fn optional_fields_store_and_retrieve_none() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("opt-none", AgentKind::ClaudeCode, "No optionals");
        store.upsert_session(&entry).unwrap();

        let retrieved =
            store.get_session(&SessionId::new("opt-none")).unwrap().expect("should find session");
        assert!(retrieved.branch.is_none());
        assert!(retrieved.pr_number.is_none());
        assert!(retrieved.pr_url.is_none());
        assert!(retrieved.plan_content.is_none());
        assert!(retrieved.ended_at.is_none());
    }

    #[test]
    fn datetime_fields_round_trip_without_precision_loss() {
        let store = SessionStore::open_in_memory().unwrap();
        let started = Utc.with_ymd_and_hms(2026, 3, 15, 14, 30, 45).unwrap();
        let ended = Utc.with_ymd_and_hms(2026, 3, 15, 16, 0, 0).unwrap();
        let indexed = Utc.with_ymd_and_hms(2026, 3, 15, 16, 5, 0).unwrap();

        let mut entry = make_entry("dt-test", AgentKind::ClaudeCode, "DateTime test");
        entry.started_at = started;
        entry.ended_at = Some(ended);
        entry.indexed_at = indexed;

        store.upsert_session(&entry).unwrap();
        let retrieved =
            store.get_session(&SessionId::new("dt-test")).unwrap().expect("should find session");

        assert_eq!(retrieved.started_at, started);
        assert_eq!(retrieved.ended_at, Some(ended));
        assert_eq!(retrieved.indexed_at, indexed);
    }

    #[test]
    fn agent_kind_round_trips_through_text_storage() {
        let store = SessionStore::open_in_memory().unwrap();
        let kinds =
            [AgentKind::ClaudeCode, AgentKind::Codex, AgentKind::OpenCode, AgentKind::Aider];
        for (i, kind) in kinds.iter().enumerate() {
            let id = format!("agent-rt-{i}");
            let entry = make_entry(&id, kind.clone(), "Agent round-trip");
            store.upsert_session(&entry).unwrap();
            let retrieved =
                store.get_session(&SessionId::new(&id)).unwrap().expect("should find session");
            assert_eq!(&retrieved.agent, kind);
        }
    }

    #[test]
    fn session_status_round_trips_through_text_storage() {
        let store = SessionStore::open_in_memory().unwrap();
        let statuses = [
            SessionStatus::Active,
            SessionStatus::Completed,
            SessionStatus::Errored,
            SessionStatus::Unknown,
        ];
        for (i, status) in statuses.iter().enumerate() {
            let id = format!("status-rt-{i}");
            let mut entry = make_entry(&id, AgentKind::ClaudeCode, "Status test");
            entry.status = status.clone();
            store.upsert_session(&entry).unwrap();
            let retrieved =
                store.get_session(&SessionId::new(&id)).unwrap().expect("should find session");
            assert_eq!(&retrieved.status, status);
        }
    }

    #[test]
    fn agent_kind_unknown_custom_round_trips() {
        let store = SessionStore::open_in_memory().unwrap();
        let entry = make_entry("custom-agent", AgentKind::Unknown("MyTool".to_string()), "Custom");
        store.upsert_session(&entry).unwrap();
        let retrieved = store
            .get_session(&SessionId::new("custom-agent"))
            .unwrap()
            .expect("should find session");
        assert_eq!(retrieved.agent, AgentKind::Unknown("MyTool".to_string()));
    }

    // ========================================================================
    // Error handling tests
    // ========================================================================

    #[test]
    fn store_error_display_sqlite() {
        let err = StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows);
        let msg = err.to_string();
        assert!(msg.contains("SQLite"), "should mention SQLite: {msg}");
    }

    #[test]
    fn store_error_display_io() {
        let err =
            StoreError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        let msg = err.to_string();
        assert!(msg.contains("I/O"), "should mention I/O: {msg}");
    }

    #[test]
    fn store_error_display_data() {
        let err = StoreError::DataError("invalid timestamp".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid timestamp"), "should contain the message: {msg}");
    }

    #[test]
    fn store_error_display_migration() {
        let err = StoreError::MigrationError("table already exists".to_string());
        let msg = err.to_string();
        assert!(msg.contains("migration"), "should mention migration: {msg}");
    }

    #[test]
    fn open_invalid_path_returns_io_or_sqlite_error() {
        // Opening a database at a path inside a nonexistent directory should fail
        let result = SessionStore::open(Path::new("/nonexistent/deeply/nested/dir/sessions.db"));
        assert!(result.is_err(), "should fail for invalid path");
    }
}
