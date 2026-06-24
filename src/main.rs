mod app;
mod log2state;
mod meshtastic;
mod repository;
mod serde;
mod service;
mod state;
mod types;
mod ui;

use std::{env, process};

use etcetera::BaseStrategy;
use tracing_unwrap::ResultExt;

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("APP_VERSION");

#[tokio::main]
async fn main() {
    let xdg = etcetera::choose_base_strategy().expect_or_log("xdg config build failed");
    let data_dir = xdg.data_dir().join(APP_NAME);
    let config_dir = xdg.config_dir().join(APP_NAME);

    let args: Vec<String> = env::args().collect();

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!("\x1b[1m{}\x1b[22m {}", APP_NAME, APP_VERSION);
        println!();

        for line in ui::logo::LOGO_ASCII {
            println!("{}", line);
        }

        println!();
        println!("Feature-rich handmade Meshtastic® console client written in Rust.");
        println!();
        println!(
            "\x1b[1mUsage\x1b[22m: {}{} [OPTIONS]",
            APP_NAME,
            if cfg!(target_os = "windows") { ".exe" } else { "" }
        );
        println!();
        println!("\x1b[1mOptions\x1b[22m:");
        println!("  -h, --help     Print help");
        println!("  -V, --version  Print version");
        println!();
        println!("\x1b[1mDirectories\x1b[22m:");
        println!("  data:    {}", data_dir.display());
        println!("  config:  {}", config_dir.display());

        process::exit(0);
    }

    if args.contains(&"--version".to_string()) || args.contains(&"-V".to_string()) {
        println!("{}", APP_VERSION);

        process::exit(0);
    }

    app::run(data_dir, config_dir).await;
}
