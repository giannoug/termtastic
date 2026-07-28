use crate::repository::domain::traceroute::Traceroute;
use crate::types::NodeTraceroute;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct TracerouteData {
    route: Vec<u32>,
    snr_towards: Vec<i32>,
    route_back: Vec<u32>,
    snr_back: Vec<i32>,
}

impl TryFrom<NodeTraceroute> for Traceroute {
    type Error = anyhow::Error;

    fn try_from(value: NodeTraceroute) -> Result<Self, Self::Error> {
        let data = TracerouteData {
            route: value.route,
            snr_towards: value.snr_towards,
            route_back: value.route_back,
            snr_back: value.snr_back,
        };

        Ok(Self {
            id: None,
            node_key: value.node_key,
            datetime: value.datetime,
            data: serde_sqlite_jsonb::to_vec(&data)?.to_vec(),
        })
    }
}

impl TryFrom<&Traceroute> for NodeTraceroute {
    type Error = anyhow::Error;

    fn try_from(value: &Traceroute) -> Result<Self, Self::Error> {
        let data: TracerouteData = serde_sqlite_jsonb::from_slice(value.data.as_slice())?;

        Ok(Self {
            node_key: value.node_key,
            datetime: value.datetime,
            route: data.route,
            snr_towards: data.snr_towards,
            route_back: data.route_back,
            snr_back: data.snr_back,
        })
    }
}
