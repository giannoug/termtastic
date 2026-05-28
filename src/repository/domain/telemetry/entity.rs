use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Telemetry {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub node_key: u32,
    pub datetime: DateTime<Utc>,
    pub kind: String,
    pub data: Vec<u8>,
}
