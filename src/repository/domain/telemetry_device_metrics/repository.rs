use super::TelemetryDeviceMetrics;
use crate::repository::{Repository, RepositoryError};
use rusqlite::{Error, params};
use serde_rusqlite::{from_row, from_rows, to_params_named};
use std::collections::HashMap;

pub const TABLE_NAME: &str = "telemetry_device_metrics";

impl Repository {
    #[allow(dead_code)]
    pub fn telemetry_device_metrics_find_all(&self) -> Result<Vec<TelemetryDeviceMetrics>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!("SELECT * FROM {}", TABLE_NAME))?;

        let result: Result<Vec<TelemetryDeviceMetrics>, _> = from_rows::<TelemetryDeviceMetrics>(statement.query([])?)
            .into_iter()
            .collect();

        result.map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_device_metrics_find_by_node_key(
        &self,
        node_key: u32,
    ) -> Result<Vec<TelemetryDeviceMetrics>, RepositoryError> {
        let mut statement = self
            .conn
            .prepare(&format!("SELECT * FROM {} WHERE node_key = :node_key", TABLE_NAME))?;

        let result: Result<Vec<TelemetryDeviceMetrics>, _> =
            from_rows::<TelemetryDeviceMetrics>(statement.query(params![node_key])?)
                .into_iter()
                .collect();

        result.map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_device_metrics_find_last_by_node_key(
        &self,
        node_key: u32,
    ) -> Result<Option<TelemetryDeviceMetrics>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT * FROM {} WHERE node_key = :node_key ORDER BY datetime DESC LIMIT 1",
            TABLE_NAME
        ))?;

        match statement.query_row(params![node_key], |row| Ok(from_row::<TelemetryDeviceMetrics>(row))) {
            Ok(Ok(item)) => Ok(Some(item)),
            Ok(Err(e)) => Err(e.into()),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn telemetry_device_metrics_find_last_for_each_node(
        &self,
    ) -> Result<HashMap<u32, TelemetryDeviceMetrics>, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            SELECT * FROM (
                SELECT *, ROW_NUMBER() OVER (PARTITION BY node_key ORDER BY datetime DESC) AS rn FROM {}
            ) WHERE rn = 1;
            "#,
            TABLE_NAME
        ))?;

        let result: Result<HashMap<u32, TelemetryDeviceMetrics>, _> =
            from_rows::<TelemetryDeviceMetrics>(statement.query([])?)
                .into_iter()
                .map(|item| item.map(|metrics| (metrics.node_key, metrics)))
                .collect();

        result.map_err(Into::into)
    }

    pub fn telemetry_device_metrics_insert(
        &self,
        telemetry_device_metrics: TelemetryDeviceMetrics,
    ) -> Result<i64, RepositoryError> {
        let mut statement = self.conn.prepare(&format!(
            r#"
            INSERT INTO {} (
                node_key,
                datetime,
                metrics_battery_level,
                metrics_voltage,
                metrics_channel_utilization,
                metrics_air_util_tx,
                metrics_uptime_seconds
            )
            VALUES (
                :node_key,
                :datetime,
                :metrics_battery_level,
                :metrics_voltage,
                :metrics_channel_utilization,
                :metrics_air_util_tx,
                :metrics_uptime_seconds
            )
            "#,
            TABLE_NAME
        ))?;

        statement
            .insert(to_params_named(&telemetry_device_metrics)?.to_slice().as_slice())
            .map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn telemetry_device_metrics_remove_by_node_key(&self, node_key: u32) -> Result<(), RepositoryError> {
        let mut statement = self
            .conn
            .prepare(&format!("DELETE FROM {} WHERE node_key = :node_key", TABLE_NAME))?;

        match statement.execute(params![node_key]) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
