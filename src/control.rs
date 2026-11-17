//! Authenticated loopback control protocol shared by the menu-bar agent and
//! the Flutter user interface.
//!
//! The agent owns the DNS engine, the system DNS registration, and the
//! configuration file. The user interface never touches those resources
//! directly: it sends one JSON request per connection and receives one JSON
//! response. Configuration documents travel through this channel so the UI and
//! the DNS service always validate against the exact same strict parser.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::config;

/// Protocol revision. It changes whenever a request or response shape changes,
/// so an outdated user interface fails loudly instead of misreading a reply.
pub const CONTROL_PROTOCOL: u8 = 2;

/// The control record the agent publishes for the user interface.
pub const CONTROL_FILE_NAME: &str = "agent.json";
/// The agent's single-instance lock file.
pub const LOCK_FILE_NAME: &str = "agent.lock";

/// How long the user interface waits for a single agent reply. Privileged
/// operations wait for an authorization prompt, so the budget is generous.
pub const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(75);
/// How long the agent waits before it answers an unread request.
pub const SERVER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(70);
/// Connect timeout for a control request.
pub const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Bundled configuration used as a starting draft when the user has no
/// configuration file yet.
pub const DEFAULT_CONFIG: &str = include_str!("../config.example.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommand {
    /// Reply with the current runtime status.
    Status,
    /// Start the DNS engine.
    StartEngine,
    /// Restore managed system DNS, then stop the DNS engine.
    StopEngine,
    /// Restart the DNS engine.
    RestartEngine,
    /// Point eligible physical network services at the loopback listener.
    EnableSystemDns,
    /// Restore the services EdgeSteer manages to automatic DNS.
    DisableSystemDns,
    /// Register the packaged application as a login item.
    EnableAutoStart,
    /// Remove the login item.
    DisableAutoStart,
    /// Remove a pre-App root DNS daemon.
    RemoveLegacyService,
    /// Re-validate the configuration file and refresh status.
    Refresh,
    /// Open (or focus) the user interface window, optionally on a page.
    OpenUi { page: Option<UiPage> },
    /// Quit the agent after restoring managed system DNS.
    Quit,
    /// Read the configuration file. A missing file yields the bundled default
    /// with `exists = false`.
    ReadConfig,
    /// Validate a configuration draft without writing it.
    ValidateConfig { contents: String },
    /// Validate, then atomically replace the configuration file.
    SaveConfig { contents: String },
    /// Read the user interface preferences (language and appearance).
    ReadPreferences,
    /// Persist the user interface preferences.
    SavePreferences { preferences: UiPreferences },
}

impl AgentCommand {
    /// True when the command mutates the DNS engine, system DNS, or login items.
    pub const fn changes_runtime(&self) -> bool {
        matches!(
            self,
            Self::StartEngine
                | Self::StopEngine
                | Self::RestartEngine
                | Self::EnableSystemDns
                | Self::DisableSystemDns
                | Self::EnableAutoStart
                | Self::DisableAutoStart
                | Self::RemoveLegacyService
                | Self::Refresh
                | Self::Quit
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub engine_running: bool,
    pub listener: String,
    pub listener_ready: bool,
    pub system_dns_enabled: bool,
    pub autostart_enabled: bool,
    pub configuration_error: Option<String>,
    /// Whether a legacy root DNS daemon was detected on this machine.
    pub legacy_service_detected: bool,
    /// Whether the packaged application can register a login item.
    pub autostart_supported: bool,
    /// Network services EdgeSteer inspects for system DNS management.
    pub dns_services: Vec<DnsServiceStatus>,
    /// Whether the interface currently has a live window process.
    pub ui_running: bool,
    /// What the interface window shows. `None` when no window is running.
    pub ui: Option<UiSnapshot>,
}

impl AgentStatus {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            engine_running: false,
            listener: "Unavailable".to_owned(),
            listener_ready: false,
            system_dns_enabled: false,
            autostart_enabled: false,
            configuration_error: Some(message.into()),
            legacy_service_detected: false,
            autostart_supported: false,
            dns_services: Vec::new(),
            ui_running: false,
            ui: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsServiceStatus {
    pub name: String,
    pub device: String,
    pub enabled: bool,
    pub servers: Vec<String>,
    pub automatic: bool,
}

/// What the interface window currently shows. This is the agent-facing view of
/// the UI: enough to know whether a window exists and what it is displaying,
/// without screen scraping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSnapshot {
    pub running: bool,
    /// The page the window opened on, when it is known.
    pub page: Option<UiPage>,
    pub language: Language,
    pub appearance: AppearanceMode,
    pub configuration_path: String,
    pub configuration_exists: bool,
    /// `null` when the saved document is valid.
    pub configuration_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub path: String,
    pub contents: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok: bool,
    pub message: String,
    pub status: AgentStatus,
    #[serde(default)]
    pub config: Option<ConfigSnapshot>,
    #[serde(default)]
    pub preferences: Option<UiPreferences>,
}

impl AgentResponse {
    pub fn success(message: impl Into<String>, status: AgentStatus) -> Self {
        Self {
            ok: true,
            message: message.into(),
            status,
            config: None,
            preferences: None,
        }
    }

    pub fn failure(message: impl Into<String>, status: AgentStatus) -> Self {
        Self {
            ok: false,
            message: message.into(),
            status,
            config: None,
            preferences: None,
        }
    }

    pub fn with_config(mut self, config: ConfigSnapshot) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_preferences(mut self, preferences: UiPreferences) -> Self {
        self.preferences = Some(preferences);
        self
    }
}

/// The wire request. `token` is the shared secret from the control record; an
/// unauthenticated connection is rejected before the command is decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub protocol: u8,
    pub token: String,
    pub command: AgentCommand,
}

/// The record published in the state directory so the UI can find the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRecord {
    pub protocol: u8,
    pub pid: u32,
    pub address: SocketAddr,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Chinese,
    English,
}

impl Default for Language {
    fn default() -> Self {
        Self::Chinese
    }
}

/// A page the interface can be opened on. The wire names match the Flutter
/// interface's page identifiers so `EDGESTEER_UI_PAGE` and the control protocol
/// agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiPage {
    Status,
    /// The configuration section, opened on its first view.
    Config,
    Resolvers,
    /// The Flutter interface names this page `ruleSets`, and the same string is
    /// passed through `EDGESTEER_UI_PAGE`. Spelling it explicitly keeps the wire
    /// value, the environment value, and the Dart page identifier identical.
    #[serde(rename = "ruleSets")]
    RuleSets,
    Preferred,
    Json,
    System,
}

impl UiPage {
    /// The value the interface reads from its environment.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Config => "config",
            Self::Resolvers => "resolvers",
            Self::RuleSets => "ruleSets",
            Self::Preferred => "preferred",
            Self::Json => "json",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceMode {
    /// Follow the operating system appearance.
    System,
    Dark,
    Light,
}

impl Default for AppearanceMode {
    fn default() -> Self {
        Self::System
    }
}

/// Preferences shared by the Flutter interface and the menu-bar labels. The
/// agent reads this file so the tray follows the interface language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPreferences {
    pub language: Language,
    pub appearance: AppearanceMode,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            language: Language::Chinese,
            appearance: AppearanceMode::System,
        }
    }
}

impl UiPreferences {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map_err(|error| format!("read UI preferences {}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("read UI preferences {}: {error}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create UI preferences directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("encode UI preferences: {error}"))?;
        std::fs::write(path, contents)
            .map_err(|error| format!("save UI preferences {}: {error}", path.display()))
    }
}

/// Reads the configuration file for the interface. A missing file is not an
/// error: the bundled example becomes the draft, and `exists` stays false so
/// the interface can say the file has not been created yet.
pub fn read_config_snapshot(path: &Path) -> Result<ConfigSnapshot, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(ConfigSnapshot {
            path: path.display().to_string(),
            contents,
            exists: true,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ConfigSnapshot {
            path: path.display().to_string(),
            contents: DEFAULT_CONFIG.to_owned(),
            exists: false,
        }),
        Err(error) => Err(format!(
            "read configuration file {}: {error}",
            path.display()
        )),
    }
}

/// Validates a draft with the same parser the DNS service uses.
pub fn validate_config_text(contents: &str) -> Result<(), String> {
    config::parse_config_text(contents)
        .map(|_| ())
        .map_err(|error| format!("{error:#}"))
}

/// Validates, then atomically replaces the configuration file.
pub fn save_config_text(path: &Path, contents: &str) -> Result<(), String> {
    config::write_config_atomically(path, contents).map_err(|error| format!("{error:#}"))
}

/// Resolves the state directory that holds the control record and preferences.
pub fn state_directory() -> PathBuf {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from);

    #[cfg(target_os = "macos")]
    {
        home.map(|path| path.join("Library/Application Support/EdgeSteer"))
            .unwrap_or_else(|| PathBuf::from("edgesteer"))
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("EdgeSteer"))
            .or_else(|| home.map(|path| path.join("AppData/Roaming/EdgeSteer")))
            .unwrap_or_else(|| PathBuf::from("edgesteer"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("edgesteer"))
            .or_else(|| home.map(|path| path.join(".config/edgesteer")))
            .unwrap_or_else(|| PathBuf::from("edgesteer"))
    }
}

/// The preferences file shared with the agent's tray labels.
pub fn ui_preferences_path() -> PathBuf {
    state_directory().join("ui.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_configuration_is_valid() {
        validate_config_text(DEFAULT_CONFIG).expect("bundled example configuration must validate");
    }

    #[test]
    fn missing_configuration_file_yields_the_bundled_default() {
        let directory = tempfile::tempdir().expect("create temporary directory");
        let snapshot = read_config_snapshot(&directory.path().join("absent.json"))
            .expect("a missing file is not an error");

        assert!(!snapshot.exists);
        assert_eq!(snapshot.contents, DEFAULT_CONFIG);
    }

    #[test]
    fn invalid_drafts_are_rejected_before_writing() {
        let directory = tempfile::tempdir().expect("create temporary directory");
        let path = directory.path().join("edgesteer.json");

        assert!(save_config_text(&path, "{ not json").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn saving_a_valid_draft_round_trips() {
        let directory = tempfile::tempdir().expect("create temporary directory");
        let path = directory.path().join("edgesteer.json");
        save_config_text(&path, DEFAULT_CONFIG).expect("save the bundled configuration");

        let snapshot = read_config_snapshot(&path).expect("read the saved configuration");
        assert!(snapshot.exists);
        assert_eq!(snapshot.contents, DEFAULT_CONFIG);
    }

    #[test]
    fn preferences_default_to_chinese_and_following_the_system() {
        let preferences = UiPreferences::default();
        assert_eq!(preferences.language, Language::Chinese);
        assert_eq!(preferences.appearance, AppearanceMode::System);
    }

    #[test]
    fn an_existing_preferences_file_without_system_mode_still_loads() {
        // Older files only ever wrote `dark` or `light`; the new `system` value
        // must be additive, not a breaking change.
        let directory = tempfile::tempdir().expect("create temporary directory");
        let path = directory.path().join("ui.json");
        std::fs::write(&path, r#"{"language":"english","appearance":"light"}"#)
            .expect("write preferences");

        let loaded = UiPreferences::load(&path).expect("load preferences");
        assert_eq!(loaded.appearance, AppearanceMode::Light);
        assert_eq!(loaded.language, Language::English);
    }

    #[test]
    fn appearance_modes_round_trip() {
        for mode in [
            AppearanceMode::System,
            AppearanceMode::Dark,
            AppearanceMode::Light,
        ] {
            let encoded = serde_json::to_string(&mode).expect("encode mode");
            let decoded: AppearanceMode = serde_json::from_str(&encoded).expect("decode mode");
            assert_eq!(decoded, mode);
        }
        assert_eq!(
            serde_json::to_string(&AppearanceMode::System).unwrap(),
            "\"system\""
        );
    }

    #[test]
    fn preferences_round_trip_through_json() {
        let directory = tempfile::tempdir().expect("create temporary directory");
        let path = directory.path().join("ui.json");
        let preferences = UiPreferences {
            language: Language::English,
            appearance: AppearanceMode::Light,
        };
        preferences.save(&path).expect("save preferences");

        assert_eq!(
            UiPreferences::load(&path).expect("load preferences"),
            preferences
        );
    }

    #[test]
    fn ui_page_wire_names_match_the_interface_page_identifiers() {
        // The wire value, the `EDGESTEER_UI_PAGE` value, and the Flutter page
        // identifier are one string. If they drift, `ui <page>` silently opens
        // the wrong page, so pin them here.
        for (page, wire) in [
            (UiPage::Status, "status"),
            (UiPage::Config, "config"),
            (UiPage::Resolvers, "resolvers"),
            (UiPage::RuleSets, "ruleSets"),
            (UiPage::Preferred, "preferred"),
            (UiPage::Json, "json"),
            (UiPage::System, "system"),
        ] {
            let encoded = serde_json::to_string(&page).expect("encode page");
            assert_eq!(encoded, format!("\"{wire}\""));
            assert_eq!(page.as_str(), wire);
            let decoded: UiPage = serde_json::from_str(&encoded).expect("decode page");
            assert_eq!(decoded, page);
        }
    }

    #[test]
    fn commands_that_change_the_runtime_are_classified() {
        assert!(AgentCommand::StartEngine.changes_runtime());
        assert!(AgentCommand::Quit.changes_runtime());
        assert!(!AgentCommand::Status.changes_runtime());
        assert!(!AgentCommand::ReadConfig.changes_runtime());
    }
}
