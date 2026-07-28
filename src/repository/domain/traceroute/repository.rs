use crate::repository::domain::traceroute::Traceroute;
use crate::repository::{Repository, RepositoryError};
use rusqlite::params;
use serde_rusqlite::{from_rows, to_params_named};
use std::collections::HashMap;

pub const TABLE_NAME: &str = "traceroute";

impl Repository {
    pub async fn traceroute_find_last_for_each_node(&self) -> Result<HashMap<u32, Traceroute>, RepositoryError> {
        let result: HashMap<u32, Traceroute> = self
            .conn
            .call(move |conn| -> Result<_, RepositoryError> {
                let mut statement = conn.prepare(&format!(
                    r#"
                    SELECT *
                    FROM (
                        SELECT t.*, ROW_NUMBER() OVER (
                            PARTITION BY node_key
                            ORDER BY datetime DESC
                        ) AS rn
                        FROM {} t
                    )
                    WHERE rn = 1;
                    "#,
                    TABLE_NAME
                ))?;

                from_rows::<Traceroute>(statement.query([])?)
                    .into_iter()
                    .collect::<Result<Vec<Traceroute>, _>>()
                    .map(|valid_items| {
                        valid_items
                            .into_iter()
                            .map(|item| (item.node_key, item))
                            .collect::<HashMap<u32, Traceroute>>()
                    })
                    .map_err(Into::into)
            })
            .await?;

        Ok(result)
    }

    pub async fn traceroute_insert(&self, traceroute: Traceroute) -> Result<i64, RepositoryError> {
        let result: i64 = self
            .conn
            .call(move |conn| -> Result<_, RepositoryError> {
                let mut statement = conn.prepare(&format!(
                    r#"
                    INSERT INTO {} (
                        node_key,
                        datetime,
                        data
                    )
                    VALUES (
                        :node_key,
                        :datetime,
                        :data
                    )
                    "#,
                    TABLE_NAME
                ))?;

                statement
                    .insert(to_params_named(&traceroute)?.to_slice().as_slice())
                    .map_err(Into::into)
            })
            .await?;

        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn traceroute_remove_by_node_key(&self, node_key: u32) -> Result<(), RepositoryError> {
        self.conn
            .call(move |conn| -> Result<_, RepositoryError> {
                let mut statement = conn.prepare(&format!("DELETE FROM {} WHERE node_key = :node_key", TABLE_NAME))?;
                statement.execute(params![node_key]).map_err(Into::into)
            })
            .await?;

        Ok(())
    }
}
