use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct TelemetryDeviceMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub node_key: u32,
    pub datetime: DateTime<Utc>,
    pub metrics_battery_level: Option<u32>,
    pub metrics_voltage: Option<f32>,
    pub metrics_channel_utilization: Option<f32>,
    pub metrics_air_util_tx: Option<f32>,
    pub metrics_uptime_seconds: Option<u32>,
}
