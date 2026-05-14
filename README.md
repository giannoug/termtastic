# termtastic

<p align="center"><img src="./logo.png" width="466" alt="termtastic"></p>

<p align="center">
  <b>termtastic</b> is a feature-rich handmade <a href="https://meshtastic.org">Meshtastic®</a> console client written in Rust.
</p>


<p align="center">
  <a href="https://meshtastic.org"><img src="./logo-mpowered.png" height="20" alt="M-Powered"/></a>
  <a href="https://github.com/acelot/termtastic/actions"><img src="https://github.com/acelot/termtastic/actions/workflows/build.yml/badge.svg?event=push" height="20" alt="Build"/></a>
  <img src="https://img.shields.io/github/languages/code-size/acelot/termtastic?cacheSeconds=86400" height="20" alt="code size"/>
  <img src="https://img.shields.io/badge/vibecode--free-pink" height="20" alt="vibecode-free"/>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/acelot/termtastic.svg?style=flat&v=2" height="20" alt="GPL-3.0"/></a>
</p>

<table>
  <tr>
    <td><img src="https://github.com/user-attachments/assets/b806aec3-a1d8-4778-ab70-cb3eae633ef9" alt="Channels"/></td>
    <td><img src="https://github.com/user-attachments/assets/a664ee0f-d252-44ec-bfb9-e2d221ec7ef7" alt="Chat"/></td>
  </tr>
  <tr>
    <td><img src="https://github.com/user-attachments/assets/b672f1da-54d8-4b22-8342-3fa13d117037" alt="Emoji Selector"/></td>
    <td><img src="https://github.com/user-attachments/assets/71e34cf6-f2b2-46a8-9ab8-b9fc73c86294" alt="Nodes"/></td>
  </tr>
  <tr>
    <td><img src="https://github.com/user-attachments/assets/03da9c1e-d0b8-4b6a-ae5e-53e87e5bc064" alt="NodeInfo"/></td>
    <td><img src="https://github.com/user-attachments/assets/3f3b464f-1dd6-432e-ae58-01a0d5f0ca10" alt="Connection"/></td>
  </tr>
  <tr>
    <td><img src="https://github.com/user-attachments/assets/1c0ac2f2-6559-48bd-947b-599e40e08072" alt="Settings"/></td>
    <td><img src="https://github.com/user-attachments/assets/8b1d1111-5fe2-4a1f-95e4-2a86e3dfaec5" alt="Logs"/></td>
  </tr>
</table>

| :warning: WARNING                                                                       |
|:----------------------------------------------------------------------------------------|
| Project is under active development, things could be changed completely without notice. |

## Tech highlights

- Vibecode-free fully handwritten code
- Dependency-free single-binary application
- Fully asynchronous (Tokio channels-based)
- Optimized for small screens (down to 80x24 characters)
- Respects the user terminal colors (no fancy opinionated TrueColor schemes)
- Keyboard-centric UI navigation
- Minimum use of emoji/non-ascii characters in UI
- Small memory footprint (~20KB)
- Supports multiple connection protocols (TCP, BLE, Serial)
- Informative logs tab

## Features

> [!NOTE]  
> Unchecked items are not implemented yet.

### Chat tab

#### Channels

- [x] Scrollable channels list (Primary, Secondary)
- [x] Direct conversations
- [x] Display the last message for each channel
- [x] Purge channel chat feature

#### Messenger

- [x] Scrollable chat screen
- [x] Display the short and long names of node
- [x] Display the SNR/RSSI for direct nodes
- [x] Display the number of hops for retranslated messages
- [x] Display the time of messages
- [x] Display the reactions (emojis)
- [x] Ability to see detailed info about reactions
- [x] Ability to see node info of the selected message author
- [x] Ability to send broadcast messages to the channels
- [x] Ability to send direct messages to the nodes
- [x] Ability to reply to the messages
- [x] Ability to send the reactions (emojis)
- [x] Limiting the message length to 200 chars (with counter)

### Nodes tab

#### Nodes list

- [x] Scrollable nodes list
- [x] Ability to start direct conversation with the selected node
- [x] Display the short and long names of node
- [x] Display the SNR/RSSI for direct nodes
- [x] Display the number of hops for the routed nodes
- [x] Display the ID of the nodes
- [x] Display the humanized last heard time of the nodes
- [x] Sort nodes by different fields: last heard, hop count, distance, etc.
- [x] Nodes fuzzy search
- [x] Nodes filtering by special tokens
- [x] Ability to see node info of the selected node

#### Single node expanded view

- [x] Display node detailed info
- [x] Copy public key to clipboard
- [x] Delete node feature
- [ ] Traceroute feature
- [ ] Ignore feature
- [ ] Add to Favorite feature
- [ ] Position info
- [ ] Telemetry info
- [ ] TBD

### Settings tab

#### General

- [x] Loading device configuration (generic feature)
- [x] Saving device configuration (generic feature)
- [ ] Import configuration from the link

#### App

- [x] UI

#### Radio

- [x] LoRa
- [x] Channels
- [x] Security

#### Device

- [x] User
- [x] Device
- [x] Position
- [x] Power
- [x] Display
- [x] Bluetooth

#### Module

- [x] MQTT
- [x] Serial
- [x] External Notification
- [x] Store & Forward
- [x] Range Test
- [x] Telemetry
- [x] Canned Message
- [x] Neighbor Info
- [x] Ambient Lighting
- [x] Detection Sensor

### Connection tab

- [x] Scrollable devices list (TCP, BLE, Serial)
- [x] Connection via TCP
- [x] Connection via BLE
- [x] Connection via Serial
- [x] Device configuration loading during the connection process and storing it into state
- [x] Storing TCP connections into a config file
- [x] Discovering of BLE and Serial devices feature
- [x] Reconnection feature with exponential backoff timeouts

### Logs tab

- [x] Writing logs into files using a daily rolling strategy
- [x] Mirroring logs into the log list with scroll
- [x] Ability to expand the single log record (useful for long logs)
- [x] Ability to copy log record into clipboard

### General features

- [x] RX indicator
- [x] Online/Total nodes counter
- [ ] Storing nodes in local DB
- [ ] Storing messages in local DB

## Stack

| Feature                     | Library                                                         |
|:----------------------------|:----------------------------------------------------------------|
| TUI: Framework              | [Ratatui](https://ratatui.rs)                                   |
| TUI: Backend                | [crossterm](https://github.com/crossterm-rs/crossterm)          |
| TUI: Inputs                 | [ratatui-textarea](https://github.com/ratatui/ratatui-textarea) |
| TUI: Lists                  | [tui-widget-list](https://github.com/preiter93/tui-widget-list) |
| Interaction with Meshtastic | [meshtastic](https://github.com/meshtastic/rust)                |
| Clipboard functionality     | [arboard](https://github.com/1Password/arboard)                 |
| Bluetooth devices discovery | [bluest](https://github.com/alexmoon/bluest/)                   |
| Logging                     | [tracing](https://github.com/tokio-rs/tracing)                  |
| Async/Channels              | [tokio](https://github.com/tokio-rs/tokio)                      |
| Configuration               | [confy](https://github.com/rust-cli/confy)                      |
| Errors                      | [anyhow](https://github.com/dtolnay/anyhow)                     |
| Datetime                    | [chrono](https://github.com/chronotope/chrono)                  |
| Emoji selector              | [emoji](https://github.com/Shizcow/emoji-rs)                    |

## Compatibility

✅ - tested, 🔬 - untested, ❌ - not working

| Feature                  | 🐧 Linux | 🍏 macOS | 🪟 Windows |
|:-------------------------|:--------:|:--------:|:----------:|
| BLE devices discovery    |    ✅     |    ✅     |     🔬     |
| Serial devices discovery |    ✅     |    ✅     |     ✅      |
| Copy to clipboard        |    ✅     |    ✅     |     ✅      |

## Download

| Source             | Link                                                      |
|:-------------------|:----------------------------------------------------------|
| Manual download    | [Releases](https://github.com/acelot/termtastic/releases) |
| Debian PPA         | 🏗️ TBA                                                   |
| Arch Linux AUR     | 🏗️ TBA                                                   |
| macOS Brew         | 🏗️ TBA                                                   |
| Windows Chocolatey | 🏗️ TBA                                                   |

## FAQ

### How to launch a manually downloaded app on macOS?

To run an unsigned application on macOS, you need to dequarantine it using the command below:

```sh
xattr -d com.apple.quarantine ./path/to/termtastic
```

### Why emojis are glitching/tearing on my terminal?

If you are using `foot` terminal try to add these lines into your `foot.ini` config:

```ini
[tweak]
grapheme-width-method=wcswidth
```