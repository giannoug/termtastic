use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::path::PathBuf;

const MIGRATIONS_LIST: &[M<'_>] = &[
    M::up(include_str!("migrations/0001.sql")),
    M::up(include_str!("migrations/0002.sql")),
];

const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_LIST);

pub fn create_sqlite_repository<'a>(file_path: PathBuf) -> Result<Repository, anyhow::Error> {
    let mut conn = Connection::open(file_path)?;

    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    MIGRATIONS.to_latest(&mut conn)?;

    Ok(Repository::new(conn))
}

#[derive(thiserror::Error, Debug)]
pub enum RepositoryError {
    #[error("db error: {0}")]
    DbError(#[from] rusqlite::Error),
    #[error("serialization error")]
    SerializationError(#[from] serde_rusqlite::Error),
}

pub struct Repository {
    pub conn: Connection,
}

impl<'a> Repository {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn get_file_path(&self) -> &str {
        self.conn.path().unwrap()
    }

    pub fn vacuum(&self) -> Result<(), RepositoryError> {
        match self.conn.execute("VACUUM", []) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
