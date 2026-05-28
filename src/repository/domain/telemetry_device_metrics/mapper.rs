use super::entity::TelemetryDeviceMetrics;
use crate::types::TelemetryPacket;
use chrono::{DateTime, Utc};
use meshtastic::protobufs::{DeviceMetrics, telemetry};

impl TryFrom<TelemetryPacket> for TelemetryDeviceMetrics {
    type Error = anyhow::Error;

    fn try_from(value: TelemetryPacket) -> Result<Self, Self::Error> {
        let telemetry::Variant::DeviceMetrics(metrics) = value.data else {
            return Err(anyhow::anyhow!("DeviceMetrics expected, got: {:?}", value.data));
        };

        Ok(Self {
            id: None,
            node_key: value.node_key,
            datetime: DateTime::from_timestamp(value.time as i64, 0).unwrap_or_else(|| Utc::now()),
            metrics_battery_level: metrics.battery_level,
            metrics_voltage: metrics.voltage,
            metrics_channel_utilization: metrics.channel_utilization,
            metrics_air_util_tx: metrics.air_util_tx,
            metrics_uptime_seconds: metrics.uptime_seconds,
        })
    }
}

impl From<TelemetryDeviceMetrics> for DeviceMetrics {
    fn from(value: TelemetryDeviceMetrics) -> Self {
        Self {
            battery_level: value.metrics_battery_level,
            voltage: value.metrics_voltage,
            channel_utilization: value.metrics_channel_utilization,
            air_util_tx: value.metrics_air_util_tx,
            uptime_seconds: value.metrics_uptime_seconds,
        }
    }
}
