use crate::APP_NAME;
use itertools::Itertools;
use native_db::{Builder, Database, Models};
use std::path::PathBuf;
use std::sync::LazyLock;

static MODELS: LazyLock<Models> = LazyLock::new(|| {
    let mut models = Models::new();
    models.define::<super::entities::v1::Node>().unwrap();
    models
});

pub fn create_repository<'a>(dir: &PathBuf, cache_size: usize) -> Result<Repository<'a>, anyhow::Error> {
    Ok(Repository::new(
        Builder::new()
            .set_cache_size(cache_size)
            .create(&MODELS, dir.join(format!("{}.db", APP_NAME)))?,
    ))
}

#[derive(thiserror::Error, Debug)]
pub enum RepositoryError {
    #[error("db error: {0}")]
    DbError(#[from] native_db::db_type::Error),
}

pub struct Repository<'a> {
    db: Database<'a>,
}

impl<'a> Repository<'a> {
    pub fn new(db: Database<'a>) -> Repository<'a> {
        Repository { db }
    }

    pub fn nodes_get_all(&self) -> Result<Vec<super::entities::Node>, RepositoryError> {
        let r = self.db.r_transaction()?;
        let nodes: Vec<super::entities::Node> = r.scan().primary()?.all()?.try_collect()?;

        Ok(nodes)
    }

    pub fn nodes_find_by_key(&self, key: u32) -> Result<Option<super::entities::Node>, RepositoryError> {
        let r = self.db.r_transaction()?;
        let node: Option<super::entities::Node> = r.get().primary(key)?;

        Ok(node)
    }

    pub fn nodes_upsert(&self, node: super::entities::Node) -> Result<bool, RepositoryError> {
        let rw = self.db.rw_transaction()?;
        let is_updated = rw.upsert(node)?.is_some();
        rw.commit()?;

        Ok(is_updated)
    }

    pub fn nodes_remove(&self, key: u32) -> Result<bool, RepositoryError> {
        let rw = self.db.rw_transaction()?;

        if let Some(node) = rw.get().primary::<super::entities::Node>(key)? {
            let is_removed = rw.remove(node).is_ok();
            rw.commit()?;

            return Ok(is_removed);
        }

        Ok(false)
    }
}
