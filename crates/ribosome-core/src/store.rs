use crate::{
    contracts::*,
    error::{Error, Result},
    validation::{counter, now_ms, validate},
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    time::Instant,
};

pub struct Store {
    pub(crate) db: Connection,
    pub(crate) write_wait_us: Cell<u64>,
    pub(crate) export_directory: Option<PathBuf>,
}

impl Store {
    /// Another connection in an already owned host lifecycle. Never migrates
    /// or recovers runs; the original connection has completed startup.
    pub(crate) fn service_connection(&self) -> Result<Self> {
        let path = self
            .db
            .path()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| Error::invalid("supervision requires a persistent SQLite database"))?;
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            db,
            write_wait_us: Cell::new(0),
            export_directory: self.export_directory.clone(),
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let existing_version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if existing_version > 21 {
            return Err(Error::invalid("unsupported database schema version"));
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        if existing_version == 0 {
            db.execute_batch(include_str!("store.sql"))?;
        }
        if existing_version < 21 {
            if existing_version > 0 && path != Path::new(":memory:") {
                let backup = path.with_file_name(format!(
                    "{}.before-schema-{}-{}.db",
                    path.file_name()
                        .and_then(|p| p.to_str())
                        .ok_or_else(|| Error::invalid("database backup path must be UTF-8"))?,
                    existing_version,
                    crate::validation::id()
                ));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup
                        .to_str()
                        .ok_or_else(|| Error::invalid("database backup path must be UTF-8"))?],
                )?;
            }
            let tx = rusqlite::Transaction::new_unchecked(
                &db,
                rusqlite::TransactionBehavior::Immediate,
            )?;
            if existing_version < 2 {
                tx.execute_batch(include_str!("migration-2.sql"))?;
            }
            if existing_version < 3 {
                tx.execute_batch(include_str!("migration-3.sql"))?;
            }
            if existing_version < 4 {
                tx.execute_batch(include_str!("migration-4.sql"))?;
            }
            if existing_version < 5 {
                tx.execute_batch(include_str!("migration-5.sql"))?;
            }
            if existing_version < 6 {
                tx.execute_batch(include_str!("migration-6.sql"))?;
            }
            if existing_version < 7 {
                tx.execute_batch(include_str!("migration-7.sql"))?;
            }
            if existing_version < 8 {
                tx.execute_batch(include_str!("migration-8.sql"))?;
            }
            if existing_version < 9 {
                tx.execute_batch(include_str!("migration-9.sql"))?;
            }
            if existing_version < 10 {
                tx.execute_batch(include_str!("migration-10.sql"))?;
            }
            if existing_version < 3 {
                crate::sources::migrate_sources(&tx)?;
            }
            if existing_version < 11 {
                tx.execute_batch(include_str!("migration-11.sql"))?;
                crate::sources::upgrade_source_lineage(&tx)?;
            }
            if existing_version < 12 {
                tx.execute_batch(include_str!("migration-12.sql"))?;
            }
            if existing_version < 13 {
                tx.execute_batch(include_str!("migration-13.sql"))?;
            }
            if existing_version < 14 {
                tx.execute_batch(include_str!("migration-14.sql"))?;
            }
            if existing_version < 15 {
                tx.execute_batch(include_str!("migration-15.sql"))?;
            }
            if existing_version < 16 {
                tx.execute_batch(include_str!("migration-16.sql"))?;
            }
            if existing_version < 17 {
                tx.execute_batch(include_str!("migration-17.sql"))?;
            }
            if existing_version < 18 {
                tx.execute_batch(include_str!("migration-18.sql"))?;
            }
            if existing_version < 19 {
                tx.execute_batch(include_str!("migration-19.sql"))?;
            }
            if existing_version < 20 {
                tx.execute_batch(include_str!("migration-20.sql"))?;
            }
            tx.execute_batch(include_str!("migration-21.sql"))?;
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
        if version != 21 {
            return Err(Error::invalid("unsupported database schema version"));
        }
        Ok(Self {
            db,
            write_wait_us: Cell::new(0),
            export_directory: None,
        })
    }

    pub(crate) fn write_transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        // Reserve the writer before reading state that will be updated. With
        // two WAL connections, a deferred read-to-write upgrade can otherwise
        // fail immediately with SQLITE_BUSY_SNAPSHOT despite busy_timeout.
        let started = Instant::now();
        let result = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        );
        self.write_wait_us.set(
            self.write_wait_us
                .get()
                .saturating_add(started.elapsed().as_micros().min(u64::MAX as u128) as u64),
        );
        Ok(result?)
    }

    pub fn register_grant(&self, grant: &Grant) -> Result<()> {
        validate("Grant", &serde_json::to_value(grant)?)?;
        if grant.prepared_run.is_some() {
            return Err(Error::invalid(
                "prepared_run is derived by Rust, not a registered grant setting",
            ));
        }
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
