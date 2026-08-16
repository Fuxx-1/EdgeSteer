use std::{fs, path::PathBuf, time::Duration};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|argument| argument == "--engine") {
        run_engine_mode(&arguments);
        return;
    }

    let app_executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("edgesteer-ui"));
    let options = edgesteer::agent::AgentOptions {
        config_path: edgesteer::default_config_path(),
        app_bundle: current_app_bundle(),
        app_executable: app_executable.clone(),
    };

    if arguments.iter().any(|argument| argument == "--ui") {
        let client = edgesteer::agent::AgentClient::current_user();
        if let Err(error) = client.status() {
            eprintln!("EdgeSteer Agent is unavailable: {error}");
            std::process::exit(1);
        }
        if let Err(error) = edgesteer::ui::run(edgesteer::ui::UiOptions {
            config_path: options.config_path,
            app_bundle: options.app_bundle,
            agent: client,
        }) {
            eprintln!("EdgeSteer UI stopped: {error}");
            std::process::exit(1);
        }
        return;
    }

    let open_ui = !arguments.iter().any(|argument| argument == "--agent");
    if let Err(error) = edgesteer::agent::start_or_open(options, open_ui) {
        eprintln!("EdgeSteer Agent stopped: {error}");
        std::process::exit(1);
    }
}

fn run_engine_mode(arguments: &[String]) {
    let stop_file = argument_value(arguments, "--stop-file");
    let parent_pid = argument_value(arguments, "--parent-pid").and_then(|value| value.parse().ok());
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("EdgeSteer could not create its engine runtime: {error}");
            std::process::exit(1);
        }
    };
    let result = runtime.block_on(async move {
        let result = edgesteer::run_with_shutdown(
            edgesteer::default_config_path(),
            wait_for_engine_shutdown(stop_file, parent_pid),
        )
        .await;

        // If the Agent disappears without its normal close handshake, the
        // privileged helper is the final process that can restore the system
        // DNS it owns before the loopback resolver exits.
        if parent_pid.is_some_and(|pid| !parent_process_is_alive(pid)) {
            if let Err(error) = edgesteer::integration::restore_managed_dns_for_engine() {
                eprintln!("EdgeSteer could not restore managed system DNS: {error:#}");
            }
        }
        result
    });
    if let Err(error) = result {
        eprintln!("EdgeSteer engine stopped: {error:#}");
        std::process::exit(1);
    }
}

fn argument_value(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

async fn wait_for_engine_shutdown(stop_file: Option<String>, parent_pid: Option<u32>) {
    loop {
        let requested = stop_file
            .as_ref()
            .is_some_and(|path| fs::metadata(path).is_ok());
        let parent_gone = parent_pid.is_some_and(|pid| !parent_process_is_alive(pid));
        if requested || parent_gone {
            return;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

fn parent_process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
    }
}

fn current_app_bundle() -> Option<std::path::PathBuf> {
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
