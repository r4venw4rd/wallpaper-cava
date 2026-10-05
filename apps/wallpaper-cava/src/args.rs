//! CLI argument parsing: `--config path [--check-config]`.

use std::fs;

/// Parsed CLI arguments.
pub struct Args {
    /// Config file path.
    pub config_path: String,
    /// Only validate the config, don't start the visualizer.
    pub check_only: bool,
}

/// CLI usage, written to stdout. Stdout is correct here: this is the
/// program's primary output channel when misused.
#[allow(clippy::print_stdout)]
pub fn print_help() {
    println!("Command line options");
    println!("--config path [--check-config]");
    println!("--check-config validates the config and exits without Wayland");
}

/// Parse `argv` into [`Args`]. Exits `0` after printing help on misuse.
pub fn parse_args(argv: &[String]) -> Args {
    let mut config_path: Option<String> = None;
    let mut check_only = false;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--check-config" => check_only = true,
            "--config" => {
                i += 1;
                if i >= argv.len() {
                    print_help();
                    std::process::exit(0);
                }
                config_path = Some(argv[i].clone());
            }
            _ => {
                print_help();
                std::process::exit(0);
            }
        }
        i += 1;
    }
    let config_path = config_path.unwrap_or_else(|| {
        std::env::var("HOME")
            .map(|home| format!("{home}/.config/wallpaper-cava/config.toml"))
            .ok()
            .filter(|path| fs::metadata(path).is_ok())
            .unwrap_or_else(|| "config.toml".to_string())
    });
    Args {
        config_path,
        check_only,
    }
}
