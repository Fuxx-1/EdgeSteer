//! Privileged EdgeSteer DNS engine helper.
//!
//! The agent launches this binary through a single macOS authorization prompt
//! when the configured listener needs port 53. It contains nothing but the DNS
//! service: no window, no menu bar, and no configuration editing.
//!
//! Shutdown is cooperative. The agent writes the stop file, and the helper also
//! exits when the agent process disappears, so a crash cannot leave a loopback
//! resolver running with the system pointing at it.

use std::{fs, time::Duration};

use edgesteer::default_config_path;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let stop_file = argument_value(&arguments, "--stop-file");
    let parent_pid =
        argument_value(&arguments, "--parent-pid").and_then(|value| value.parse().ok());

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
            default_config_path(),
            wait_for_shutdown(stop_file, parent_pid),
        )
        .await;

        // If the agent disappears without its normal close handshake, the
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

async fn wait_for_shutdown(stop_file: Option<String>, parent_pid: Option<u32>) {
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
