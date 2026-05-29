use crate::repository::domain::telemetry::Telemetry;
use crate::repository::{Repository, RepositoryError};
use rusqlite::params;
use serde_rusqlite::{from_rows, to_params_named};
use std::collections::HashMap;

pub const TABLE_NAME: &str = "telemetry";

impl Repository {
    #[allow(dead_code)]
    pub fn telemetry_find_all(&self) -> Result<Vec<Telemetry>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!("SELECT * FROM {}", TABLE_NAME))?;

        let result: Result<Vec<Telemetry>, _> = from_rows::<Telemetry>(statement.query([])?).into_iter().collect();

        result.map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_find_by_node_key(&self, node_key: u32) -> Result<Vec<Telemetry>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT * FROM {} AS t WHERE t.node_key = :node_key",
            TABLE_NAME
        ))?;

        let result: Result<Vec<Telemetry>, _> = from_rows::<Telemetry>(statement.query(params![node_key])?)
            .into_iter()
            .collect();

        result.map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_find_last_by_node_key(&self, node_key: u32) -> Result<Vec<Telemetry>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT *
            FROM (
                SELECT t.*, ROW_NUMBER() OVER (
                    PARTITION BY node_key, kind
                    ORDER BY datetime DESC
                ) AS rn
                FROM {} t WHERE t.node_key = :node_key
            )
            WHERE rn = 1;
            "#,
            TABLE_NAME
        ))?;

        let result: Result<Vec<Telemetry>, _> = from_rows::<Telemetry>(statement.query(params![node_key])?)
            .into_iter()
            .collect();

        result.map_err(Into::into)
    }

    pub fn telemetry_find_last_for_each_node(&self) -> Result<HashMap<u32, Vec<Telemetry>>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT *
            FROM (
                SELECT t.*, ROW_NUMBER() OVER (
                    PARTITION BY node_key, kind
                    ORDER BY datetime DESC
                ) AS rn
                FROM {} t
            )
            WHERE rn = 1;
            "#,
            TABLE_NAME
        ))?;

        let result: Result<HashMap<u32, Vec<Telemetry>>, _> = from_rows::<Telemetry>(statement.query([])?)
            .into_iter()
            .collect::<Result<Vec<Telemetry>, _>>()
            .map(|valid_items| {
                valid_items
                    .into_iter()
                    .fold(HashMap::new(), |mut map: HashMap<u32, Vec<Telemetry>>, item| {
                        map.entry(item.node_key).or_default().push(item);
                        map
                    })
            });

        result.map_err(Into::into)
    }

    pub fn telemetry_insert(&self, telemetry: Telemetry) -> Result<i64, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            INSERT INTO {} (
                node_key,
                datetime,
                kind,
                data
            )
            VALUES (
                :node_key,
                :datetime,
                :kind,
                :data
            )
            "#,
            TABLE_NAME
        ))?;

        statement
            .insert(to_params_named(&telemetry)?.to_slice().as_slice())
            .map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_remove_by_node_key(&self, node_key: u32) -> Result<(), RepositoryError> {
        let mut statement = self
            .conn
            .prepare(&format!("DELETE FROM {} WHERE node_key = :node_key", TABLE_NAME))?;

        match statement.execute(params![node_key]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
