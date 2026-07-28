use btleplug::api::{BDAddr, Central, Manager as _, Peripheral as _};
use btleplug::platform::Manager;
use hostaddr::HostAddr;
use meshtastic::api::{ConnectedStreamApi, StreamApi};
use meshtastic::protobufs::FromRadio;
use meshtastic::utils;
use std::time::Duration;
use tokio::sync::mpsc;

const MESHTASTIC_DEFAULT_TCP_PORT: u16 = 4403;

pub async fn connect_via_tcp(
    address: HostAddr<String>,
) -> anyhow::Result<(mpsc::UnboundedReceiver<FromRadio>, ConnectedStreamApi)> {
    let stream_handle = utils::stream::build_tcp_stream(if !address.has_port() {
        address.with_port(MESHTASTIC_DEFAULT_TCP_PORT).to_string()
    } else {
        address.to_string()
    })
    .await?;

    let stream_api = StreamApi::new();
    let (from_radio_receiver, connected_stream_api) = stream_api.connect(stream_handle).await;

    let connected_stream_api = connected_stream_api.configure(utils::generate_rand_id()).await?;

    Ok((from_radio_receiver, connected_stream_api))
}

pub async fn connect_via_ble(
    address: BDAddr,
    name: Option<String>,
) -> anyhow::Result<(mpsc::UnboundedReceiver<FromRadio>, ConnectedStreamApi)> {
    let ble_id = match name {
        Some(n) => utils::stream::BleId::Name(n),
        None => utils::stream::BleId::MacAddress(address),
    };

    let stream_handle = utils::stream::build_ble_stream(ble_id, Duration::from_secs(5)).await?;

    let stream_api = StreamApi::new();
    let (from_radio_receiver, connected_stream_api) = stream_api.connect(stream_handle).await;

    let connected_stream_api = connected_stream_api.configure(utils::generate_rand_id()).await?;

    Ok((from_radio_receiver, connected_stream_api))
}

/// Explicitly tears down the BLE link at the OS/BlueZ level.
///
/// The `meshtastic` crate's `StreamApi::disconnect` only stops its worker tasks
/// and drops the `btleplug` peripheral handle. Dropping the handle does not
/// disconnect the GATT connection, so the device stays connected after exit.
/// Here we look the peripheral up by its address and disconnect it directly.
pub async fn disconnect_ble(address: BDAddr) -> anyhow::Result<()> {
    let manager = Manager::new().await?;

    for adapter in manager.adapters().await? {
        for peripheral in adapter.peripherals().await? {
            if peripheral.address() == address {
                if peripheral.is_connected().await.unwrap_or(false) {
                    peripheral.disconnect().await?;
                }

                return Ok(());
            }
        }
    }

    Ok(())
}

pub async fn connect_via_serial(
    address: String,
) -> anyhow::Result<(mpsc::UnboundedReceiver<FromRadio>, ConnectedStreamApi)> {
    let stream_handle = utils::stream::build_serial_stream(address, None, None, None)?;

    let stream_api = StreamApi::new();
    let (from_radio_receiver, connected_stream_api) = stream_api.connect(stream_handle).await;

    let connected_stream_api = connected_stream_api.configure(utils::generate_rand_id()).await?;

    Ok((from_radio_receiver, connected_stream_api))
}
