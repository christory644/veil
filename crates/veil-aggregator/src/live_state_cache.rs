//! SQLite-backed cache for live state check results.
//!
//! Stores branch, PR, and directory check results with TTL-based
//! expiration. Shares the `SQLite` database with `SessionStore`.

use chrono::{DateTime, Duration, Utc};

use crate::store::StoreError;

/// Cache key types for live state checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckType {
    /// Git branch existence check.
    Branch,
    /// Pull request state check.
    Pr,
    /// Directory existence check.
    Dir,
}

impl CheckType {
    /// String representation for storage in the database.
    pub fn as_str(&self) -> &str {
        match self {
            CheckType::Branch => "branch",
            CheckType::Pr => "pr",
            CheckType::Dir => "dir",
        }
    }
}

/// A cached check result.
#[derive(Debug, Clone)]
pub struct CachedCheck {
    /// The type of check.
    pub check_type: CheckType,
    /// The cache key.
    pub check_key: String,
    /// The state value (e.g., "exists", "deleted", "open", "merged", "closed", "missing", "unknown").
    pub state: String,
    /// When the check was performed.
    pub checked_at: DateTime<Utc>,
}

/// Operations for the `live_state_cache` table.
///
/// Takes a reference to a `rusqlite::Connection` (shared with `SessionStore`).
pub struct LiveStateCache<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> LiveStateCache<'a> {
    /// Create a new cache handle over an existing connection.
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Run migrations (CREATE TABLE IF NOT EXISTS).
    pub fn run_migrations(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS live_state_cache (
                check_type  TEXT NOT NULL,
                check_key   TEXT NOT NULL,
                state       TEXT NOT NULL,
                checked_at  TEXT NOT NULL,
                PRIMARY KEY (check_type, check_key)
            );",
        )?;
        Ok(())
    }

    /// Look up a cached result. Returns None if not found or if the
    /// entry is older than `max_age`.
    pub fn get(
        &self,
        check_type: CheckType,
        key: &str,
        max_age: Duration,
    ) -> Result<Option<CachedCheck>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT state, checked_at
             FROM live_state_cache
             WHERE check_type = ?1 AND check_key = ?2",
        )?;

        let mut rows = stmt.query(rusqlite::params![check_type.as_str(), key])?;

        match rows.next()? {
            Some(row) => {
                let state: String = row.get(0)?;
                let checked_at_str: String = row.get(1)?;

                let checked_at = DateTime::parse_from_rfc3339(&checked_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        StoreError::DataError(format!("invalid checked_at timestamp: {e}"))
                    })?;

                let cutoff = Utc::now() - max_age;
                if checked_at > cutoff {
                    Ok(Some(CachedCheck {
                        check_type,
                        check_key: key.to_owned(),
                        state,
                        checked_at,
                    }))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Store a check result (upsert).
    pub fn put(
        &self,
        check_type: CheckType,
        key: &str,
        state: &str,
        checked_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO live_state_cache (check_type, check_key, state, checked_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![check_type.as_str(), key, state, checked_at.to_rfc3339()],
        )?;
        Ok(())
    }

    /// Remove expired entries older than `max_age`.
    pub fn evict_expired(&self, max_age: Duration) -> Result<usize, StoreError> {
        let cutoff = Utc::now() - max_age;
        let deleted = self.conn.execute(
            "DELETE FROM live_state_cache WHERE checked_at <= ?1",
            rusqlite::params![cutoff.to_rfc3339()],
        )?;
        Ok(deleted)
    }

    /// Clear all cached state (for testing or full refresh).
    pub fn clear_all(&self) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM live_state_cache", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: open an in-memory `SQLite` connection and run cache migrations.
    fn setup_cache() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory should succeed");
        let cache = LiveStateCache::new(&conn);
        cache.run_migrations().expect("migrations should succeed");
        conn
    }

    // ================================================================
    // Schema / migrations
    // ================================================================

    #[test]
    fn migrations_create_table() {
        let conn = setup_cache();
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='live_state_cache'",
                [],
                |row| row.get(0),
            )
            .expect("query should succeed");
        assert!(table_exists, "live_state_cache table should exist after migration");
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory should succeed");
        let cache = LiveStateCache::new(&conn);
        cache.run_migrations().expect("first migration should succeed");
        let result = cache.run_migrations();
        assert!(result.is_ok(), "second migration should also succeed");
    }

    // ================================================================
    // put + get round-trip
    // ================================================================

    #[test]
    fn put_then_get_round_trips() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache.put(CheckType::Branch, "/repo::main", "exists", now).expect("put should succeed");

        let result = cache
            .get(CheckType::Branch, "/repo::main", Duration::seconds(60))
            .expect("get should succeed");

        assert!(result.is_some(), "should find the cached entry");
        let cached = result.unwrap();
        assert_eq!(cached.check_type, CheckType::Branch);
        assert_eq!(cached.check_key, "/repo::main");
        assert_eq!(cached.state, "exists");
    }

    #[test]
    fn put_upserts_existing_entry() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache
            .put(CheckType::Branch, "/repo::feat", "exists", now)
            .expect("first put should succeed");
        cache
            .put(CheckType::Branch, "/repo::feat", "deleted", now)
            .expect("second put should succeed");

        let result = cache
            .get(CheckType::Branch, "/repo::feat", Duration::seconds(60))
            .expect("get should succeed");
        let cached = result.expect("should find entry");
        assert_eq!(cached.state, "deleted", "upsert should overwrite with new value");
    }

    #[test]
    fn get_on_empty_table_returns_none() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);

        let result = cache
            .get(CheckType::Branch, "/repo::main", Duration::seconds(60))
            .expect("get should succeed");
        assert!(result.is_none(), "empty table should return None");
    }

    #[test]
    fn get_nonexistent_key_returns_none() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache.put(CheckType::Branch, "/repo::main", "exists", now).expect("put should succeed");

        let result = cache
            .get(CheckType::Branch, "/repo::other", Duration::seconds(60))
            .expect("get should succeed");
        assert!(result.is_none(), "different key should return None");
    }

    #[test]
    fn get_different_check_type_returns_none() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache.put(CheckType::Branch, "/repo::main", "exists", now).expect("put should succeed");

        let result = cache
            .get(CheckType::Pr, "/repo::main", Duration::seconds(60))
            .expect("get should succeed");
        assert!(result.is_none(), "different check_type should return None");
    }

    // ================================================================
    // TTL expiration
    // ================================================================

    #[test]
    fn get_expired_entry_returns_none() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let old_time = Utc::now() - Duration::seconds(120);

        cache
            .put(CheckType::Branch, "/repo::main", "exists", old_time)
            .expect("put should succeed");

        // max_age of 60s means the 120s-old entry is expired.
        let result = cache
            .get(CheckType::Branch, "/repo::main", Duration::seconds(60))
            .expect("get should succeed");
        assert!(result.is_none(), "expired entry should return None");
    }

    #[test]
    fn get_fresh_entry_returns_some() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let recent = Utc::now() - Duration::seconds(10);

        cache.put(CheckType::Branch, "/repo::main", "exists", recent).expect("put should succeed");

        let result = cache
            .get(CheckType::Branch, "/repo::main", Duration::seconds(60))
            .expect("get should succeed");
        assert!(result.is_some(), "fresh entry should be returned");
    }

    #[test]
    fn get_entry_at_exact_max_age_boundary() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let max_age = Duration::seconds(60);
        let boundary_time = Utc::now() - max_age;

        cache
            .put(CheckType::Dir, "/some/path", "exists", boundary_time)
            .expect("put should succeed");

        // At exactly the boundary, the entry should be considered expired
        // (strictly less than max_age to be fresh).
        let result = cache.get(CheckType::Dir, "/some/path", max_age).expect("get should succeed");
        assert!(result.is_none(), "entry at exact max_age boundary should be expired");
    }

    #[test]
    fn get_entry_one_second_before_expiry() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let max_age = Duration::seconds(60);
        // Entry was checked 59s ago — 1 second before expiry.
        let almost_expired = Utc::now() - Duration::seconds(59);

        cache
            .put(CheckType::Dir, "/some/path", "exists", almost_expired)
            .expect("put should succeed");

        let result = cache.get(CheckType::Dir, "/some/path", max_age).expect("get should succeed");
        assert!(result.is_some(), "entry 1s before max_age should still be fresh");
    }

    // ================================================================
    // evict_expired
    // ================================================================

    #[test]
    fn evict_expired_removes_old_entries() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();
        let old = now - Duration::seconds(120);

        cache.put(CheckType::Branch, "/repo::old", "exists", old).expect("put should succeed");
        cache.put(CheckType::Branch, "/repo::fresh", "exists", now).expect("put should succeed");

        let evicted = cache.evict_expired(Duration::seconds(60)).expect("evict should succeed");
        assert_eq!(evicted, 1, "should evict 1 old entry");

        // Fresh entry should still be there.
        let fresh = cache
            .get(CheckType::Branch, "/repo::fresh", Duration::seconds(60))
            .expect("get should succeed");
        assert!(fresh.is_some(), "fresh entry should survive eviction");

        // Old entry should be gone.
        let old_result = cache
            .get(CheckType::Branch, "/repo::old", Duration::seconds(3600))
            .expect("get should succeed");
        assert!(old_result.is_none(), "old entry should be evicted");
    }

    #[test]
    fn evict_expired_returns_zero_when_nothing_expired() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache.put(CheckType::Branch, "/repo::fresh", "exists", now).expect("put should succeed");

        let evicted = cache.evict_expired(Duration::seconds(60)).expect("evict should succeed");
        assert_eq!(evicted, 0, "nothing should be evicted");
    }

    #[test]
    fn evict_expired_on_empty_table_returns_zero() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);

        let evicted = cache.evict_expired(Duration::seconds(60)).expect("evict should succeed");
        assert_eq!(evicted, 0);
    }

    // ================================================================
    // clear_all
    // ================================================================

    #[test]
    fn clear_all_empties_the_table() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        cache.put(CheckType::Branch, "/repo::a", "exists", now).expect("put should succeed");
        cache.put(CheckType::Pr, "/repo::42", "open", now).expect("put should succeed");
        cache.put(CheckType::Dir, "/some/path", "exists", now).expect("put should succeed");

        cache.clear_all().expect("clear_all should succeed");

        // All entries should be gone.
        let a = cache
            .get(CheckType::Branch, "/repo::a", Duration::seconds(3600))
            .expect("get should succeed");
        let b = cache
            .get(CheckType::Pr, "/repo::42", Duration::seconds(3600))
            .expect("get should succeed");
        let c = cache
            .get(CheckType::Dir, "/some/path", Duration::seconds(3600))
            .expect("get should succeed");
        assert!(a.is_none());
        assert!(b.is_none());
        assert!(c.is_none());
    }

    #[test]
    fn clear_all_on_empty_table_succeeds() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);

        let result = cache.clear_all();
        assert!(result.is_ok(), "clear_all on empty table should not error");
    }

    // ================================================================
    // Multiple check types coexist
    // ================================================================

    #[test]
    fn different_check_types_same_key_stored_independently() {
        let conn = setup_cache();
        let cache = LiveStateCache::new(&conn);
        let now = Utc::now();

        // Same key, different check types.
        cache.put(CheckType::Branch, "/repo::main", "exists", now).expect("put should succeed");
        cache.put(CheckType::Pr, "/repo::main", "merged", now).expect("put should succeed");

        let branch = cache
            .get(CheckType::Branch, "/repo::main", Duration::seconds(60))
            .expect("get should succeed")
            .expect("should find branch entry");
        let pr = cache
            .get(CheckType::Pr, "/repo::main", Duration::seconds(60))
            .expect("get should succeed")
            .expect("should find pr entry");

        assert_eq!(branch.state, "exists");
        assert_eq!(pr.state, "merged");
    }

    // ================================================================
    // CheckType
    // ================================================================

    #[test]
    fn check_type_as_str() {
        assert_eq!(CheckType::Branch.as_str(), "branch");
        assert_eq!(CheckType::Pr.as_str(), "pr");
        assert_eq!(CheckType::Dir.as_str(), "dir");
    }

    #[test]
    fn check_type_equality() {
        assert_eq!(CheckType::Branch, CheckType::Branch);
        assert_eq!(CheckType::Pr, CheckType::Pr);
        assert_eq!(CheckType::Dir, CheckType::Dir);
        assert_ne!(CheckType::Branch, CheckType::Pr);
        assert_ne!(CheckType::Branch, CheckType::Dir);
        assert_ne!(CheckType::Pr, CheckType::Dir);
    }
}
