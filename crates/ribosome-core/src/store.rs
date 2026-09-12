use crate::{
    contracts::*,
    error::{Error, Result},
    validation::{counter, now_ms, validate},
};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct Store {
    pub(crate) db: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let existing_version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if existing_version > 2 {
            return Err(Error::invalid("unsupported database schema version"));
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        if existing_version == 0 {
            db.execute_batch(include_str!("store.sql"))?;
        }
        if existing_version < 2 {
            let tx = rusqlite::Transaction::new_unchecked(
                &db,
                rusqlite::TransactionBehavior::Immediate,
            )?;
            tx.execute_batch(include_str!("migration-2.sql"))?;
            tx.commit()?;
        }
        let fts: bool = db.query_row(
            "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
            [],
            |row| row.get(0),
        )?;
        if !fts {
            return Err(Error::internal("SQLite requires FTS5"));
        }
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 2 {
            return Err(Error::invalid("unsupported database schema version"));
        }
        Ok(Self { db })
    }

    pub(crate) fn write_transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        // Reserve the writer before reading state that will be updated. With
        // two WAL connections, a deferred read-to-write upgrade can otherwise
        // fail immediately with SQLITE_BUSY_SNAPSHOT despite busy_timeout.
        Ok(rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?)
    }

    pub fn register_grant(&self, grant: &Grant) -> Result<()> {
        validate("Grant", &serde_json::to_value(grant)?)?;
        if counter(&grant.budget.deadline_ms)? <= now_ms() {
            return Err(Error::invalid("grant already expired"));
        }
        let body = serde_json::to_string(grant)?;
        let existing: Option<String> = self
            .db
            .query_row("SELECT body FROM grants WHERE id=?1", [&grant.id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(existing) = existing {
            if existing != body {
                return Err(Error::conflict("grant IDs are immutable"));
            }
        } else {
            self.db.execute(
                "INSERT INTO grants(id,body) VALUES (?1,?2)",
                params![grant.id, body],
            )?;
        }
        Ok(())
    }

    pub fn grant(&self, id: &str) -> Result<Grant> {
        let body: String = self
            .db
            .query_row("SELECT body FROM grants WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::missing("grant not found"))?;
        Ok(serde_json::from_str(&body)?)
    }
}
