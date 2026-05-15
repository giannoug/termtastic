use native_db::{native_db, ToKey};
use native_model::{native_model, Model};
use serde::{Deserialize, Serialize};

pub type Node = v1::Node;

pub mod v1 {
    use super::*;
    use chrono::{DateTime, Utc};

    #[derive(Serialize, Deserialize, Debug)]
    #[native_model(id = 1, version = 1)]
    #[native_db]
    pub struct Node {
        pub id: String,
        #[primary_key]
        pub key: u32,
        pub user: Option<NodeUser>,
        pub hops: Option<u32>,
        pub last_heard: Option<DateTime<Utc>>,
        pub snr: f32,
        pub rssi: Option<i32>,
        pub public_key: Vec<u8>,
        pub is_favorite: bool,
        pub is_ignored: bool,
        pub is_muted: bool,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct NodeUser {
        pub short_name: String,
        pub long_name: String,
        pub role: i32,
        pub hw_model: i32,
        pub is_licensed: bool,
        pub is_unmessagable: Option<bool>,
    }
}
