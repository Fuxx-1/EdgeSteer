use std::{
    net::{IpAddr, SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

#[cfg(target_os = "macos")]
use std::{fs, path::PathBuf, process::Command};

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};

/// Per-user launchd label used to open the packaged application at login.
pub const AUTOSTART_LABEL: &str = "io.edgesteer.app";

// These labels belonged to the pre-App implementation. They are removed only
// when a user explicitly enables login start from the packaged application.
#[cfg(target_os = "macos")]
const LEGACY_DAEMON_LABELS: [&str; 2] = ["io.edgesteer.dns", "com.fuxuxiang.edgesteer"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationStatus {
    pub listener: SocketAddr,
    pub listener_ready: bool,
    pub startup_service: StartupService,
    pub dns_services: Vec<SystemDnsService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupService {
    Registered { label: String },
    LegacyDaemon { label: String },
    NotRegistered,
    Unsupported { reason: String },
}

impl StartupService {
    pub fn description(&self) -> String {
        match self {
            Self::Registered { .. } => "Open at login enabled".to_owned(),
            Self::LegacyDaemon { .. } => "Legacy command-line service detected".to_owned(),
            Self::NotRegistered => "Open at login is disabled".to_owned(),
            Self::Unsupported { reason } => reason.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDnsService {
    pub name: String,
    pub device: String,
    pub enabled: bool,
    pub servers: Vec<String>,
}

impl SystemDnsService {
    pub fn uses_loopback_dns(&self) -> bool {
        !self.servers.is_empty() && self.servers.iter().all(|server| is_loopback_dns(server))
    }

    pub fn uses_automatic_dns(&self) -> bool {
        self.servers.is_empty()
    }

    pub fn dns_description(&self) -> String {
        if self.uses_automatic_dns() {
            "Automatic (DHCP)".to_owned()
        } else {
            self.servers.join(", ")
        }
    }

    fn uses_dns_server(&self, server: &str) -> bool {
        self.servers.len() == 1 && self.servers[0] == server
    }
}

pub fn inspect(listener: SocketAddr) -> Result<IntegrationStatus> {
    let listener_ready = listener_is_ready(listener);

    #[cfg(target_os = "macos")]
    {
        Ok(IntegrationStatus {
            listener,
            listener_ready,
            startup_service: macos_startup_service(),
            dns_services: macos_dns_services()?,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(IntegrationStatus {
            listener,
            listener_ready,
            startup_service: StartupService::Unsupported {
                reason: "System DNS registration is currently managed by the operating system on this platform.".to_owned(),
            },
            dns_services: Vec::new(),
        })
    }
}

/// Enables user-session autostart for the packaged app. This intentionally
/// launches the UI application itself; no DNS command-line daemon is installed.
pub fn enable_auto_start(app_bundle: &Path) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let app_bundle = absolute_existing_path(app_bundle, "EdgeSteer application")?;
        validate_app_bundle(&app_bundle)?;
        remove_legacy_daemons_if_present()?;

        let target = launch_agent_path()?;
        let parent = target
            .parent()
            .context("login launch agent path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create login launch agent directory {}", parent.display()))?;
        write_launch_agent(&target, &launch_agent_plist(&app_bundle))?;

        let domain = gui_domain()?;
        let label = format!("{domain}/{AUTOSTART_LABEL}");
        let _ = Command::new("/bin/launchctl")
            .args(["bootout", &label])
            .output();
        run_macos_command(
            "/bin/launchctl",
            ["bootstrap", &domain, target.to_string_lossy().as_ref()],
        )?;
        Ok("EdgeSteer will open automatically when you log in".to_owned())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_bundle;
        bail!("Login start from the UI is currently implemented for macOS only")
    }
}

pub fn disable_auto_start() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let target = launch_agent_path()?;
        let domain = gui_domain()?;
        let label = format!("{domain}/{AUTOSTART_LABEL}");
        let _ = Command::new("/bin/launchctl")
            .args(["bootout", &label])
            .output();
        remove_launch_agent(&target)?;
        Ok("EdgeSteer will no longer open automatically when you log in".to_owned())
    }

    #[cfg(not(target_os = "macos"))]
    {
        bail!("Login start from the UI is currently implemented for macOS only")
    }
}

/// Removes the root LaunchDaemon used by pre-App EdgeSteer releases. The
/// packaged App invokes this explicitly from its Registration page, so a DMG
/// install never needs an installation-time script.
pub fn remove_legacy_service() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        if legacy_daemon_label().is_none() {
            return Ok("No legacy EdgeSteer command-line service is registered".to_owned());
        }
        remove_legacy_daemons_if_present()?;
        Ok("Legacy EdgeSteer command-line service removed".to_owned())
    }

    #[cfg(not(target_os = "macos"))]
    {
        bail!("Legacy macOS service cleanup is only available on macOS")
    }
}

pub fn enable_system_dns(listener: SocketAddr) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let server = loopback_listener_server(listener)?;
        if !listener_is_ready(listener) {
            bail!(
                "no TCP DNS listener is accepting {listener}; keep EdgeSteer open before enabling system DNS"
            );
        }

        let services = macos_dns_services()?;
        let eligible: Vec<_> = services.iter().filter(|service| service.enabled).collect();
        if eligible.is_empty() {
            bail!("no enabled physical macOS network service was found");
        }
        let manual: Vec<_> = eligible
            .iter()
            .filter(|service| !service.uses_automatic_dns() && !service.uses_loopback_dns())
            .map(|service| format!("{} ({})", service.name, service.dns_description()))
            .collect();
        if !manual.is_empty() {
            bail!(
                "refusing to overwrite explicit DNS without a snapshot: {}",
                manual.join(", ")
            );
        }

        let targets: Vec<_> = eligible
            .iter()
            .filter(|service| service.uses_automatic_dns() || service.uses_dns_server(server))
            .collect();
        if targets.is_empty() {
            bail!("no enabled physical network service can use the EdgeSteer loopback listener");
        }

        let commands: Vec<String> = targets
            .iter()
            .filter(|service| !service.uses_dns_server(server))
            .map(|service| {
                format!(
                    "/usr/sbin/networksetup -setdnsservers {} {}",
                    shell_quote_text(&service.name),
                    shell_quote_text(server)
                )
            })
            .collect();

        // Write the ownership record before the privileged mutation. If the
        // process disappears between these operations, the next App startup
        // still knows that it must keep the resolver alive and restore DHCP
        // DNS on exit. If an authorization prompt is cancelled, retaining the
        // record is safer than losing recovery information after a partial
        // shell command; the next close/restore pass removes it when the
        // services are not actually pointing at EdgeSteer.
        let guard = DnsGuard {
            listener: server.to_owned(),
            services: targets.iter().map(|service| service.name.clone()).collect(),
        };
        write_dns_guard(&guard)?;

        let result = if commands.is_empty() {
            "System DNS already points at the EdgeSteer listener".to_owned()
        } else {
            let mut command = commands.join(" && ");
            command.push_str(" && /usr/bin/dscacheutil -flushcache >/dev/null 2>&1 || true");
            command.push_str("; /usr/bin/killall -HUP mDNSResponder >/dev/null 2>&1 || true");
            match run_macos_privileged(&command) {
                Ok(result) => result,
                Err(error) => return Err(error).context("enable EdgeSteer as system DNS"),
            }
        };
        Ok(result)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = listener;
        bail!("System DNS registration from the UI is currently implemented for macOS only")
    }
}

/// Returns macOS network services taken over by EdgeSteer to automatic DNS.
/// It never writes a historical DNS snapshot and never changes a service that
/// is not listed in EdgeSteer's ownership record.
pub fn disable_system_dns() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let Some(guard) = read_dns_guard()? else {
            return Ok("No EdgeSteer-managed system DNS is active".to_owned());
        };
        let services = macos_dns_services()?;
        let targets: Vec<_> = services
            .iter()
            .filter(|service| {
                // A service can be disabled while the app is closing. Restore
                // it anyway; otherwise re-enabling that interface later would
                // resurrect 127.0.0.1 after the engine has stopped.
                guard.services.iter().any(|name| name == &service.name)
                    && service.uses_dns_server(&guard.listener)
            })
            .collect();
        if targets.is_empty() {
            remove_dns_guard()?;
            return Ok("EdgeSteer system DNS was already restored".to_owned());
        }

        let commands: Vec<String> = targets
            .iter()
            .map(|service| {
                format!(
                    "/usr/sbin/networksetup -setdnsservers {} Empty",
                    shell_quote_text(&service.name)
                )
            })
            .collect();
        let mut command = commands.join(" && ");
        command.push_str(" && /usr/bin/dscacheutil -flushcache >/dev/null 2>&1 || true");
        command.push_str("; /usr/bin/killall -HUP mDNSResponder >/dev/null 2>&1 || true");
        let result = run_macos_privileged(&command).context("restore automatic system DNS")?;
        remove_dns_guard()?;
        Ok(result)
    }

    #[cfg(not(target_os = "macos"))]
    {
        bail!("System DNS registration from the UI is currently implemented for macOS only")
    }
}

/// Restores EdgeSteer-owned system DNS without opening another authorization
/// prompt. This is used only by the already-privileged hidden engine when its
/// UI parent disappears (for example during an OS shutdown or a forced app
/// termination). The normal UI close path still uses `disable_system_dns`,
/// which gives the user an actionable error instead of silently proceeding.
pub fn restore_managed_dns_for_engine() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let Some(guard) = read_dns_guard()? else {
            return Ok("No EdgeSteer-managed system DNS is active".to_owned());
        };
        let services = macos_dns_services()?;
        let targets: Vec<_> = services
            .iter()
            .filter(|service| {
                guard.services.iter().any(|name| name == &service.name)
                    && service.uses_dns_server(&guard.listener)
            })
            .collect();
        if targets.is_empty() {
            remove_dns_guard()?;
            return Ok("EdgeSteer system DNS was already restored".to_owned());
        }

        for service in targets {
            let status = Command::new("/usr/sbin/networksetup")
                .args(["-setdnsservers", service.name.as_str(), "Empty"])
                .status()
                .with_context(|| format!("restore DNS for {}", service.name))?;
            if !status.success() {
                bail!("networksetup could not restore DNS for {}", service.name);
            }
        }
        let _ = Command::new("/usr/bin/dscacheutil")
            .args(["-flushcache"])
            .status();
        let _ = Command::new("/usr/bin/killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
        remove_dns_guard()?;
        Ok("EdgeSteer system DNS restored to automatic DNS".to_owned())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("System DNS cleanup is not managed by EdgeSteer on this platform".to_owned())
    }
}

/// Reports whether EdgeSteer has an active ownership record for system DNS.
/// This read-only check lets the App restore a previously enabled resolver at
/// login without treating every loopback DNS setting as its own.
pub fn system_dns_is_managed() -> bool {
    #[cfg(target_os = "macos")]
    {
        read_dns_guard().ok().flatten().is_some()
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn listener_is_ready(listener: SocketAddr) -> bool {
    TcpStream::connect_timeout(&listener, Duration::from_millis(600)).is_ok()
}

fn is_loopback_dns(server: &str) -> bool {
    matches!(server.trim(), "127.0.0.1" | "::1")
}

#[cfg(any(target_os = "macos", test))]
fn loopback_listener_server(listener: SocketAddr) -> Result<&'static str> {
    if listener.port() != 53 {
        bail!("system DNS has no port setting; EdgeSteer must listen on 127.0.0.1:53 or [::1]:53");
    }
    match listener.ip() {
        IpAddr::V4(address) if address.is_loopback() => Ok("127.0.0.1"),
        IpAddr::V6(address) if address.is_loopback() => Ok("::1"),
        _ => bail!("system DNS registration requires a loopback listener"),
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Serialize, Deserialize)]
struct DnsGuard {
    listener: String,
    services: Vec<String>,
}

#[cfg(target_os = "macos")]
fn macos_startup_service() -> StartupService {
    if launch_agent_path().is_ok_and(|path| path.exists()) {
        return StartupService::Registered {
            label: AUTOSTART_LABEL.to_owned(),
        };
    }
    if let Some(label) = legacy_daemon_label() {
        return StartupService::LegacyDaemon { label };
    }
    StartupService::NotRegistered
}

#[cfg(target_os = "macos")]
fn macos_dns_services() -> Result<Vec<SystemDnsService>> {
    let order = command_output("/usr/sbin/networksetup", ["-listnetworkserviceorder"])?;
    parse_macos_network_service_order(&order)
        .into_iter()
        .map(|(name, device)| {
            let enabled = command_output(
                "/usr/sbin/networksetup",
                ["-getnetworkserviceenabled", &name],
            )?
            .trim()
                == "Enabled";
            let servers = parse_macos_dns_servers(&command_output(
                "/usr/sbin/networksetup",
                ["-getdnsservers", &name],
            )?);
            Ok(SystemDnsService {
                name,
                device,
                enabled,
                servers,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn command_output<const N: usize>(program: &str, arguments: [&str; N]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("run {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn run_macos_command<const N: usize>(program: &str, arguments: [&str; N]) -> Result<String> {
    command_output(program, arguments)
}

#[cfg(target_os = "macos")]
fn run_macos_privileged(shell_command: &str) -> Result<String> {
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        escape_applescript_string(shell_command)
    );
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .context("request macOS administrator authorization")?;
    if !output.status.success() {
        bail!(
            "administrator action failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(if result.is_empty() {
        "Completed".to_owned()
    } else {
        result
    })
}

#[cfg(target_os = "macos")]
fn absolute_existing_path(path: &Path, label: &str) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("resolve {label} path {}", path.display()))
}

#[cfg(target_os = "macos")]
fn validate_app_bundle(path: &Path) -> Result<()> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("app") {
        bail!("login start requires the installed EdgeSteer.app bundle");
    }
    let executable = path.join("Contents/MacOS/edgesteer-ui");
    if !executable.is_file() {
        bail!(
            "EdgeSteer.app does not contain its application executable at {}",
            executable.display()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn user_home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .context("HOME is unavailable")
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf> {
    Ok(launch_agent_path_for(&user_home_dir()?))
}

#[cfg(target_os = "macos")]
fn launch_agent_path_for(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{AUTOSTART_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn write_launch_agent(path: &Path, contents: &str) -> Result<()> {
    match fs::write(path, contents) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            // Some older installers leave ~/Library/LaunchAgents owned by
            // root. Keep the fallback scoped to EdgeSteer's one plist rather
            // than changing ownership of the user's LaunchAgents directory.
            let uid = command_output("/usr/bin/id", ["-u"])?
                .trim()
                .parse::<u32>()
                .context("parse current user ID")?;
            let gid = command_output("/usr/bin/id", ["-g"])?
                .trim()
                .parse::<u32>()
                .context("parse current group ID")?;
            let target = shell_quote_text(&path.display().to_string());
            let command = format!(
                "/usr/bin/printf %s {} > {target}; /usr/sbin/chown {uid}:{gid} {target}; /bin/chmod 644 {target}",
                shell_quote_text(contents),
            );
            run_macos_privileged(&command)
                .with_context(|| format!("write login launch agent {}", path.display()))?;
            Ok(())
        }
        Err(error) => {
            Err(error).with_context(|| format!("write login launch agent {}", path.display()))
        }
    }
}

#[cfg(target_os = "macos")]
fn remove_launch_agent(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let command = format!(
                "/bin/rm -f {}",
                shell_quote_text(&path.display().to_string())
            );
            run_macos_privileged(&command)
                .with_context(|| format!("remove login launch agent {}", path.display()))?;
            Ok(())
        }
        Err(error) => {
            Err(error).with_context(|| format!("remove login launch agent {}", path.display()))
        }
    }
}

#[cfg(target_os = "macos")]
fn dns_guard_path() -> Result<PathBuf> {
    Ok(user_home_dir()?.join("Library/Application Support/EdgeSteer/system-dns.json"))
}

#[cfg(target_os = "macos")]
fn write_dns_guard(guard: &DnsGuard) -> Result<()> {
    let path = dns_guard_path()?;
    let parent = path
        .parent()
        .context("DNS guard path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create DNS guard directory {}", parent.display()))?;
    let contents = serde_json::to_vec_pretty(guard).context("serialize DNS guard")?;
    fs::write(&path, contents).with_context(|| format!("write DNS guard {}", path.display()))
}

#[cfg(target_os = "macos")]
fn read_dns_guard() -> Result<Option<DnsGuard>> {
    let path = dns_guard_path()?;
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("read DNS guard {}", path.display()));
        }
    };
    let guard = serde_json::from_slice(&contents)
        .with_context(|| format!("parse DNS guard {}", path.display()))?;
    Ok(Some(guard))
}

#[cfg(target_os = "macos")]
fn remove_dns_guard() -> Result<()> {
    let path = dns_guard_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove DNS guard {}", path.display())),
    }
}

#[cfg(target_os = "macos")]
fn gui_domain() -> Result<String> {
    let output = command_output("/usr/bin/id", ["-u"])?;
    let uid = output
        .trim()
        .parse::<u32>()
        .context("parse current user ID")?;
    Ok(format!("gui/{uid}"))
}

#[cfg(target_os = "macos")]
fn legacy_daemon_label() -> Option<String> {
    LEGACY_DAEMON_LABELS.iter().find_map(|label| {
        let plist = PathBuf::from(format!("/Library/LaunchDaemons/{label}.plist"));
        let loaded = Command::new("/bin/launchctl")
            .args(["print", &format!("system/{label}")])
            .output()
            .is_ok_and(|output| output.status.success());
        (plist.exists() || loaded).then(|| (*label).to_owned())
    })
}

#[cfg(target_os = "macos")]
fn remove_legacy_daemons_if_present() -> Result<()> {
    if legacy_daemon_label().is_none() {
        return Ok(());
    }
    let mut commands = Vec::new();
    for label in LEGACY_DAEMON_LABELS {
        commands.push(format!(
            "/bin/launchctl bootout system/{label} >/dev/null 2>&1 || true"
        ));
        commands.push(format!("/bin/rm -f /Library/LaunchDaemons/{label}.plist"));
    }
    run_macos_privileged(&commands.join("; "))
        .context("remove legacy EdgeSteer command-line service")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn shell_quote_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(target_os = "macos")]
fn launch_agent_plist(app_bundle: &Path) -> String {
    let app_bundle = xml_escape(&app_bundle.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{AUTOSTART_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>-g</string>
    <string>-j</string>
    <string>-a</string>
    <string>{app_bundle}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn parse_macos_network_service_order(output: &str) -> Vec<(String, String)> {
    let mut services = Vec::new();
    let mut name = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('(') {
            if let Some((_, value)) = rest.split_once(") ") {
                name = Some(value.to_owned());
                continue;
            }
        }
        let Some(service) = name.take() else {
            continue;
        };
        let Some(device) = line
            .split("Device: ")
            .nth(1)
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        if device.starts_with("en") {
            services.push((service, device.to_owned()));
        }
    }
    services
}

#[cfg(target_os = "macos")]
fn parse_macos_dns_servers(output: &str) -> Vec<String> {
    if output.starts_with("There aren't any DNS Servers set on") {
        return Vec::new();
    }
    output
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_only_loopback_dns_servers() {
        assert!(is_loopback_dns("127.0.0.1"));
        assert!(is_loopback_dns("::1"));
        assert!(!is_loopback_dns("192.168.1.1"));
    }

    #[test]
    fn system_dns_requires_port_53_loopback_listener() {
        assert!(loopback_listener_server("127.0.0.1:53".parse().unwrap()).is_ok());
        assert!(loopback_listener_server("127.0.0.1:53535".parse().unwrap()).is_err());
        assert!(loopback_listener_server("192.168.1.10:53".parse().unwrap()).is_err());
    }

    #[test]
    fn only_an_exact_edge_steer_listener_counts_as_managed() {
        let service = SystemDnsService {
            name: "Wi-Fi".to_owned(),
            device: "en0".to_owned(),
            enabled: true,
            servers: vec!["127.0.0.1".to_owned()],
        };
        assert!(service.uses_dns_server("127.0.0.1"));
        assert!(!service.uses_dns_server("::1"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_macos_physical_network_services() {
        let services = parse_macos_network_service_order(
            "(1) USB Ethernet\n(Hardware Port: USB, Device: en7)\n(2) VPN\n(Hardware Port: VPN, Device: utun2)\n(3) Wi-Fi\n(Hardware Port: Wi-Fi, Device: en0)\n",
        );
        assert_eq!(
            services,
            vec![
                ("USB Ethernet".to_owned(), "en7".to_owned()),
                ("Wi-Fi".to_owned(), "en0".to_owned()),
            ]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_opens_the_app_bundle_without_a_cli_service() {
        let plist = launch_agent_plist(Path::new("/Applications/Edge & Steer.app"));
        assert!(plist.contains("/usr/bin/open"));
        assert!(plist.contains("Edge &amp; Steer.app"));
        assert!(!plist.contains("KeepAlive"));
        assert!(!plist.contains("edgesteer-ui"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_path_has_no_whitespace_or_extra_component() {
        assert_eq!(
            launch_agent_path_for(Path::new("/Users/example")),
            Path::new("/Users/example/Library/LaunchAgents/io.edgesteer.app.plist")
        );
    }
}
