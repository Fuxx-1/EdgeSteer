//! EdgeSteer menu-bar agent.
//!
//! This process owns the DNS engine, the system DNS registration, and the
//! configuration file. It keeps running after the Flutter interface window is
//! closed, so closing the window never stops DNS service.
//!
//! Running it a second time focuses the existing instance instead of starting
//! a second agent.

use std::path::{Path, PathBuf};

use edgesteer::agent::{self, AgentOptions};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let open_ui = !arguments.iter().any(|argument| argument == "--no-ui");
    let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("edgesteer-agent"));

    if let Err(error) = check_engine_helper(&executable) {
        eprintln!("EdgeSteer Agent: {error}");
        std::process::exit(1);
    }

    let options = AgentOptions {
        config_path: edgesteer::default_config_path(),
        app_bundle: current_app_bundle(),
        ui_executable: interface_executable(&executable),
        engine_executable: engine_executable(&executable),
    };

    if let Err(error) = agent::start_or_open(options, open_ui) {
        eprintln!("EdgeSteer Agent stopped: {error}");
        std::process::exit(1);
    }
}

/// The agent is useless without its DNS helper, so refuse to start without it
/// instead of failing later on a menu-bar click.
fn check_engine_helper(executable: &Path) -> Result<(), String> {
    let engine = engine_executable(executable);
    if engine.is_file() {
        Ok(())
    } else {
        Err(format!(
            "the DNS engine helper is missing: {}",
            engine.display()
        ))
    }
}

fn executable_directory(current: &Path) -> PathBuf {
    current
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The Flutter interface ships as a nested `EdgeSteer.app` inside the outer
/// application bundle, so its executable lives under its own `Contents/MacOS`.
/// Outside a bundle it is a plain sibling.
fn interface_executable(current: &Path) -> PathBuf {
    let directory = executable_directory(current);
    let nested = directory.join("EdgeSteer.app/Contents/MacOS/EdgeSteer");
    if nested.is_file() {
        return nested;
    }
    directory.join("EdgeSteer")
}

/// The privileged helper always sits next to the agent in `MacOS/`.
fn engine_executable(current: &Path) -> PathBuf {
    executable_directory(current).join("edgesteer-engine")
}

fn current_app_bundle() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|path| {
        path.ancestors().find_map(|ancestor| {
            (ancestor
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("app"))
            .then(|| ancestor.to_path_buf())
        })
    })
}
