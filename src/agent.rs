use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use serde::{Deserialize, Serialize};
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

use crate::{
    config,
    integration::{self, IntegrationStatus, StartupService},
    tray::{self, TrayController, TrayEvent},
};

const CONTROL_PROTOCOL: u8 = 1;
const CONTROL_FILE_NAME: &str = "agent.json";
const LOCK_FILE_NAME: &str = "agent.lock";
#[cfg(target_os = "macos")]
const ENGINE_STOP_FILE_NAME: &str = "engine.stop";
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct AgentOptions {
    pub config_path: PathBuf,
    pub app_bundle: Option<PathBuf>,
    pub app_executable: PathBuf,
}

/// Configures the process as a menu-bar application before any window or
/// event-loop code can register it as a regular Dock application.
#[cfg(target_os = "macos")]
pub fn configure_menu_bar_activation_policy() {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    let _ = application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
pub fn configure_menu_bar_activation_policy() {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommand {
    Status,
    OpenUi,
    StartEngine,
    StopEngine,
    RestartEngine,
    EnableSystemDns,
    DisableSystemDns,
    EnableAutoStart,
    DisableAutoStart,
    RemoveLegacyService,
    Refresh,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub engine_running: bool,
    pub listener: String,
    pub listener_ready: bool,
    pub system_dns_enabled: bool,
    pub autostart_enabled: bool,
    pub configuration_error: Option<String>,
}

impl AgentStatus {
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            engine_running: false,
            listener: "Unavailable".to_owned(),
            listener_ready: false,
            system_dns_enabled: false,
            autostart_enabled: false,
            configuration_error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok: bool,
    pub message: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone)]
pub struct AgentClient {
    state_dir: PathBuf,
}

impl AgentClient {
    pub fn current_user() -> Self {
        Self {
            state_dir: state_directory(),
        }
    }

    pub fn request(&self, command: AgentCommand) -> Result<AgentResponse, String> {
        let record = read_record(&self.state_dir)?;
        if record.protocol != CONTROL_PROTOCOL {
            return Err("EdgeSteer Agent uses an incompatible control protocol".to_owned());
        }

        let mut stream = TcpStream::connect_timeout(&record.address, Duration::from_secs(3))
            .map_err(|error| format!("connect to EdgeSteer Agent: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(75)))
            .map_err(|error| format!("configure EdgeSteer Agent response timeout: {error}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| format!("configure EdgeSteer Agent request timeout: {error}"))?;

        let request = ControlRequest {
            protocol: CONTROL_PROTOCOL,
            token: record.token,
            command,
        };
        let encoded = serde_json::to_vec(&request)
            .map_err(|error| format!("encode EdgeSteer Agent request: {error}"))?;
        stream
            .write_all(&encoded)
            .and_then(|()| stream.write_all(b"\n"))
            .map_err(|error| format!("send EdgeSteer Agent request: {error}"))?;
        let _ = stream.shutdown(Shutdown::Write);

        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .map_err(|error| format!("read EdgeSteer Agent response: {error}"))?;
        serde_json::from_str(&response)
            .map_err(|error| format!("parse EdgeSteer Agent response: {error}"))
    }

    pub fn status(&self) -> Result<AgentStatus, String> {
        let response = self.request(AgentCommand::Status)?;
        if response.ok {
            Ok(response.status)
        } else {
            Err(response.message)
        }
    }
}

pub fn start_or_open(options: AgentOptions, open_ui: bool) -> Result<(), String> {
    let state_dir = state_directory();
    match AgentLock::acquire(&state_dir)? {
        LockState::Existing => {
            let response = AgentClient {
                state_dir: state_dir.clone(),
            }
            .request(if open_ui {
                AgentCommand::OpenUi
            } else {
                AgentCommand::Status
            })?;
            if response.ok {
                Ok(())
            } else {
                Err(response.message)
            }
        }
        LockState::Acquired(lock) => run_agent(options, state_dir, lock, open_ui),
    }
}

fn run_agent(
    options: AgentOptions,
    state_dir: PathBuf,
    lock: AgentLock,
    open_ui: bool,
) -> Result<(), String> {
    let mut event_loop_builder = EventLoopBuilder::<()>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        event_loop_builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_default_menu(false)
            .with_activate_ignoring_other_apps(false);
    }
    let event_loop = event_loop_builder
        .build()
        .map_err(|error| format!("create EdgeSteer Agent event loop: {error}"))?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("bind EdgeSteer Agent control listener: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("read EdgeSteer Agent control address: {error}"))?;
    let record = AgentRecord {
        protocol: CONTROL_PROTOCOL,
        pid: std::process::id(),
        address,
        token: random_token(),
    };
    write_record(&state_dir, &record)?;

    let (request_sender, request_receiver) = mpsc::channel();
    let control_server = ControlServer::start(listener, record.token.clone(), request_sender)?;
    let mut agent = Agent::new(options, state_dir, lock, control_server)?;
    if open_ui {
        let _ = agent.open_ui();
    }
    agent.refresh_status();
    #[cfg(target_os = "macos")]
    let mut activation_policy_configured = false;

    event_loop
        .run(move |event, target| {
            #[cfg(target_os = "macos")]
            if matches!(event, Event::Resumed) && !activation_policy_configured {
                // winit must own NSApplication initialization. Applying the
                // menu-bar policy after the first lifecycle event also makes
                // this resilient to later AppKit/winit initialization.
                configure_menu_bar_activation_policy();
                activation_policy_configured = true;
            }

            if !matches!(event, Event::AboutToWait) {
                return;
            }

            let mut exit = false;
            while let Ok(request) = request_receiver.try_recv() {
                let response = agent.handle_command(request.command);
                exit |= agent.exit_requested;
                let _ = request.response.send(response);
            }

            while let Some(event) = agent.tray.next_event() {
                let response = agent.handle_tray_event(event);
                if !response.ok {
                    eprintln!("EdgeSteer Agent: {}", response.message);
                }
                exit |= agent.exit_requested;
            }

            if agent.last_status_refresh.elapsed() >= STATUS_REFRESH_INTERVAL {
                agent.refresh_status();
            }
            agent.sync_tray();

            if exit {
                target.exit();
            } else {
                target.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(250),
                ));
            }
        })
        .map_err(|error| format!("run EdgeSteer Agent event loop: {error}"))
}

struct Agent {
    options: AgentOptions,
    state_dir: PathBuf,
    _lock: AgentLock,
    _control_server: ControlServer,
    tray: TrayController,
    engine: Option<EngineRuntime>,
    listener: Option<SocketAddr>,
    configuration_error: Option<String>,
    status: AgentStatus,
    last_status_refresh: Instant,
    ui_child: Option<Child>,
    exit_requested: bool,
}

impl Agent {
    fn new(
        options: AgentOptions,
        state_dir: PathBuf,
        lock: AgentLock,
        control_server: ControlServer,
    ) -> Result<Self, String> {
        let mut agent = Self {
            options,
            state_dir,
            _lock: lock,
            _control_server: control_server,
            tray: TrayController::new(&empty_presentation())?,
            engine: None,
            listener: None,
            configuration_error: None,
            status: AgentStatus::unavailable("Configuration has not been checked"),
            last_status_refresh: Instant::now() - STATUS_REFRESH_INTERVAL,
            ui_child: None,
            exit_requested: false,
        };

        if let Err(error) = agent.refresh_configuration() {
            agent.configuration_error = Some(error);
        } else if agent
            .listener
            .is_some_and(|listener| listener.port() != 53 || integration::system_dns_is_managed())
        {
            if let Err(error) = agent.start_engine() {
                agent.configuration_error = Some(error);
            }
        }
        Ok(agent)
    }

    fn handle_tray_event(&mut self, event: TrayEvent) -> AgentResponse {
        let command = match event {
            TrayEvent::Open => AgentCommand::OpenUi,
            TrayEvent::ToggleEngine => {
                if self.status.engine_running {
                    AgentCommand::StopEngine
                } else {
                    AgentCommand::StartEngine
                }
            }
            TrayEvent::ToggleSystemDns => {
                if self.status.system_dns_enabled {
                    AgentCommand::DisableSystemDns
                } else {
                    AgentCommand::EnableSystemDns
                }
            }
            TrayEvent::ToggleAutostart => {
                if self.status.autostart_enabled {
                    AgentCommand::DisableAutoStart
                } else {
                    AgentCommand::EnableAutoStart
                }
            }
            TrayEvent::Quit => AgentCommand::Quit,
        };
        self.handle_command(command)
    }

    fn handle_command(&mut self, command: AgentCommand) -> AgentResponse {
        let result = match command {
            AgentCommand::Status => Ok("EdgeSteer Agent is running".to_owned()),
            AgentCommand::OpenUi => self.open_ui(),
            AgentCommand::StartEngine => self.start_engine(),
            AgentCommand::StopEngine => self.stop_engine_with_dns_restore(),
            AgentCommand::RestartEngine => self.restart_engine(),
            AgentCommand::EnableSystemDns => self.enable_system_dns(),
            AgentCommand::DisableSystemDns => integration::disable_system_dns()
                .map_err(|error| format!("restore automatic DNS: {error:#}")),
            AgentCommand::EnableAutoStart => self.enable_auto_start(),
            AgentCommand::DisableAutoStart => integration::disable_auto_start()
                .map_err(|error| format!("disable open at login: {error:#}")),
            AgentCommand::RemoveLegacyService => integration::remove_legacy_service()
                .map_err(|error| format!("remove legacy service: {error:#}")),
            AgentCommand::Refresh => self.refresh_configuration(),
            AgentCommand::Quit => self.shutdown(),
        };

        self.refresh_status();
        match result {
            Ok(message) => AgentResponse {
                ok: true,
                message,
                status: self.status.clone(),
            },
            Err(error) => AgentResponse {
                ok: false,
                message: error,
                status: self.status.clone(),
            },
        }
    }

    fn refresh_configuration(&mut self) -> Result<String, String> {
        let config = config::load_config(&self.options.config_path).map_err(|error| {
            format!(
                "validate configuration {}: {error:#}",
                self.options.config_path.display()
            )
        })?;
        self.listener = Some(config.listener.address);
        self.configuration_error = None;
        Ok("Configuration refreshed".to_owned())
    }

    fn start_engine(&mut self) -> Result<String, String> {
        let _ = self.refresh_configuration()?;
        if self.engine.is_none() {
            self.engine = Some(EngineRuntime::start(
                self.options.config_path.clone(),
                &self.options.app_executable,
            )?);
        }
        Ok("DNS engine starting".to_owned())
    }

    fn stop_engine_with_dns_restore(&mut self) -> Result<String, String> {
        let restored = integration::disable_system_dns()
            .map_err(|error| format!("restore automatic DNS before stopping: {error:#}"))?;
        self.stop_engine();
        Ok(format!("{restored}; DNS engine stopped"))
    }

    fn restart_engine(&mut self) -> Result<String, String> {
        let _ = self.refresh_configuration()?;
        self.stop_engine();
        self.start_engine()?;
        Ok("DNS engine restarted".to_owned())
    }

    fn stop_engine(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            engine.stop();
        }
    }

    fn stop_ui(&mut self) {
        let Some(mut child) = self.ui_child.take() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }

    fn enable_system_dns(&mut self) -> Result<String, String> {
        let listener = self
            .listener
            .ok_or_else(|| "configuration has no listener".to_owned())?;
        if self.engine.is_none() {
            return Err("start the DNS engine before enabling system DNS".to_owned());
        }
        integration::enable_system_dns(listener)
            .map_err(|error| format!("enable EdgeSteer system DNS: {error:#}"))
    }

    fn enable_auto_start(&self) -> Result<String, String> {
        let app_bundle = self.options.app_bundle.as_deref().ok_or_else(|| {
            "install and open EdgeSteer.app before enabling open at login".to_owned()
        })?;
        integration::enable_auto_start(app_bundle)
            .map_err(|error| format!("enable open at login: {error:#}"))
    }

    fn open_ui(&mut self) -> Result<String, String> {
        let ui_is_running = self
            .ui_child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if ui_is_running {
            return Ok("EdgeSteer window is already open".to_owned());
        }
        self.stop_ui();
        let child = Command::new(&self.options.app_executable)
            .arg("--ui")
            .spawn()
            .map_err(|error| format!("open EdgeSteer window: {error}"))?;
        self.ui_child = Some(child);
        Ok("Opening EdgeSteer".to_owned())
    }

    fn shutdown(&mut self) -> Result<String, String> {
        let restored = integration::disable_system_dns()
            .map_err(|error| format!("restore automatic DNS before quitting: {error:#}"))?;
        self.stop_engine();
        self.exit_requested = true;
        Ok(format!("{restored}; EdgeSteer stopped"))
    }

    fn refresh_status(&mut self) {
        let integration_status = self
            .listener
            .and_then(|listener| integration::inspect(listener).ok());
        self.status = status_from(
            self.listener,
            self.engine.is_some(),
            self.configuration_error.clone(),
            integration_status.as_ref(),
        );
        self.last_status_refresh = Instant::now();
    }

    fn sync_tray(&mut self) {
        self.tray.sync(&tray_presentation(
            &self.status,
            tray_language(&self.state_dir),
        ));
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        // Normal app shutdown and any event-loop teardown must leave the
        // system on DHCP DNS before the loopback resolver goes away.
        let _ = integration::disable_system_dns();
        self.stop_engine();
        self.stop_ui();
    }
}

fn status_from(
    listener: Option<SocketAddr>,
    engine_running: bool,
    configuration_error: Option<String>,
    integration_status: Option<&IntegrationStatus>,
) -> AgentStatus {
    let listener_ready = integration_status.is_some_and(|status| status.listener_ready);
    let system_dns_enabled = integration_status.is_some_and(|status| {
        status
            .dns_services
            .iter()
            .any(|service| service.uses_loopback_dns())
    }) || integration::system_dns_is_managed();
    let autostart_enabled = integration_status
        .is_some_and(|status| matches!(status.startup_service, StartupService::Registered { .. }));
    AgentStatus {
        engine_running,
        listener: listener
            .map(|listener| listener.to_string())
            .unwrap_or_else(|| "Invalid configuration".to_owned()),
        listener_ready,
        system_dns_enabled,
        autostart_enabled,
        configuration_error,
    }
}

fn empty_presentation() -> tray::Presentation {
    tray_presentation(
        &AgentStatus::unavailable("Starting EdgeSteer Agent"),
        TrayLanguage::Chinese,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayLanguage {
    Chinese,
    English,
}

fn tray_language(state_dir: &Path) -> TrayLanguage {
    let path = state_dir.join("ui.json");
    let value = fs::read(&path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<serde_json::Value>(&contents).ok());
    if value
        .as_ref()
        .and_then(|value| value.get("language"))
        .and_then(serde_json::Value::as_str)
        == Some("english")
    {
        TrayLanguage::English
    } else {
        TrayLanguage::Chinese
    }
}

fn tray_presentation(status: &AgentStatus, language: TrayLanguage) -> tray::Presentation {
    let labels = match language {
        TrayLanguage::Chinese => tray::Labels {
            open: "打开 EdgeSteer".to_owned(),
            dns: "DNS".to_owned(),
            running: "运行中".to_owned(),
            stopped: "已停止".to_owned(),
            start_engine: "启动 DNS 引擎".to_owned(),
            stop_engine: "停止 DNS 引擎".to_owned(),
            system_dns: "系统 DNS".to_owned(),
            open_at_login: "登录时打开".to_owned(),
            quit: "退出 EdgeSteer".to_owned(),
        },
        TrayLanguage::English => tray::Labels {
            open: "Open EdgeSteer".to_owned(),
            dns: "DNS".to_owned(),
            running: "Running".to_owned(),
            stopped: "Stopped".to_owned(),
            start_engine: "Start DNS engine".to_owned(),
            stop_engine: "Stop DNS engine".to_owned(),
            system_dns: "System DNS".to_owned(),
            open_at_login: "Open at login".to_owned(),
            quit: "Quit EdgeSteer".to_owned(),
        },
    };
    let listener_supports_system_dns = status
        .listener
        .parse::<SocketAddr>()
        .is_ok_and(|listener| listener.port() == 53 && listener.ip().is_loopback());
    tray::Presentation {
        labels,
        listener: status.listener.clone(),
        engine_running: status.engine_running,
        engine_action_enabled: status.configuration_error.is_none(),
        system_dns_enabled: status.system_dns_enabled,
        system_dns_action_enabled: status.configuration_error.is_none()
            && status.engine_running
            && status.listener_ready
            && listener_supports_system_dns,
        autostart_enabled: status.autostart_enabled,
        autostart_action_enabled: status.configuration_error.is_none(),
    }
}

struct EngineRuntime {
    shutdown: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
    stop_file: Option<PathBuf>,
}

impl EngineRuntime {
    fn start(config_path: PathBuf, _app_executable: &Path) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        let config = config::load_config(&config_path).map_err(|error| {
            format!(
                "validate configuration {}: {error:#}",
                config_path.display()
            )
        })?;

        #[cfg(target_os = "macos")]
        if config.listener.address.port() == 53 {
            return Self::start_privileged(config_path, _app_executable);
        }

        let (shutdown, shutdown_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("edgesteer-dns".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("EdgeSteer could not create its DNS runtime: {error}");
                        return;
                    }
                };
                if let Err(error) =
                    runtime.block_on(crate::run_with_shutdown(config_path, async move {
                        let _ = tokio::task::spawn_blocking(move || shutdown_receiver.recv()).await;
                    }))
                {
                    eprintln!("EdgeSteer DNS engine stopped: {error:#}");
                }
            })
            .map_err(|error| format!("start DNS engine: {error}"))?;

        Ok(Self {
            shutdown: Some(shutdown),
            worker: Some(worker),
            stop_file: None,
        })
    }

    #[cfg(target_os = "macos")]
    fn start_privileged(config_path: PathBuf, app_executable: &Path) -> Result<Self, String> {
        let stop_file = state_directory().join(ENGINE_STOP_FILE_NAME);
        fs::create_dir_all(state_directory())
            .map_err(|error| format!("create EdgeSteer control directory: {error}"))?;
        let _ = fs::remove_file(&stop_file);

        let (shutdown, shutdown_receiver) = mpsc::channel();
        let child_slot = Arc::new(Mutex::new(None));
        let child_slot_for_worker = Arc::clone(&child_slot);
        let executable = app_executable
            .canonicalize()
            .map_err(|error| format!("resolve packaged App executable: {error}"))?;
        let home = config_path
            .parent()
            .ok_or_else(|| "configuration has no parent directory".to_owned())?
            .to_path_buf();
        let stop_file_for_worker = stop_file.clone();
        let parent_pid = std::process::id();
        let worker = thread::Builder::new()
            .name("edgesteer-privileged-engine".to_owned())
            .spawn(move || {
                let shell_command = format!(
                    "export HOME={}; export RUST_LOG=info; exec {} --engine --stop-file {} --parent-pid {}",
                    shell_quote(&home),
                    shell_quote(&executable),
                    shell_quote(&stop_file_for_worker),
                    parent_pid,
                );
                let script = format!(
                    "do shell script \"{}\" with administrator privileges",
                    escape_applescript_string(&shell_command)
                );
                let child = match Command::new("/usr/bin/osascript").args(["-e", &script]).spawn() {
                    Ok(child) => child,
                    Err(error) => {
                        eprintln!("EdgeSteer could not request engine authorization: {error}");
                        return;
                    }
                };
                *child_slot_for_worker.lock().expect("engine child mutex poisoned") = Some(child);
                loop {
                    match shutdown_receiver.recv_timeout(Duration::from_millis(120)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let exited = child_slot_for_worker
                                .lock()
                                .expect("engine child mutex poisoned")
                                .as_mut()
                                .and_then(|child| child.try_wait().ok())
                                .flatten()
                                .is_some();
                            if exited {
                                break;
                            }
                        }
                    }
                }
                let _ = fs::write(&stop_file_for_worker, b"stop");
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    let exited = child_slot_for_worker
                        .lock()
                        .expect("engine child mutex poisoned")
                        .as_mut()
                        .and_then(|child| child.try_wait().ok())
                        .flatten()
                        .is_some();
                    if exited || Instant::now() >= deadline {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                if let Some(mut child) = child_slot_for_worker
                    .lock()
                    .expect("engine child mutex poisoned")
                    .take()
                {
                    if child.try_wait().ok().flatten().is_none() {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                }
            })
            .map_err(|error| format!("start privileged DNS engine controller: {error}"))?;

        Ok(Self {
            shutdown: Some(shutdown),
            worker: Some(worker),
            stop_file: Some(stop_file),
        })
    }

    fn stop(&mut self) {
        if let Some(stop_file) = &self.stop_file {
            let _ = fs::write(stop_file, b"stop");
        }
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for EngineRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "macos")]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentRecord {
    protocol: u8,
    pid: u32,
    address: SocketAddr,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ControlRequest {
    protocol: u8,
    token: String,
    command: AgentCommand,
}

struct ServerRequest {
    command: AgentCommand,
    response: mpsc::Sender<AgentResponse>,
}

struct ControlServer {
    alive: Arc<AtomicBool>,
    address: SocketAddr,
    worker: Option<thread::JoinHandle<()>>,
}

impl ControlServer {
    fn start(
        listener: TcpListener,
        token: String,
        request_sender: mpsc::Sender<ServerRequest>,
    ) -> Result<Self, String> {
        let address = listener
            .local_addr()
            .map_err(|error| format!("read EdgeSteer Agent control address: {error}"))?;
        let alive = Arc::new(AtomicBool::new(true));
        let alive_for_worker = Arc::clone(&alive);
        let worker = thread::Builder::new()
            .name("edgesteer-agent-control".to_owned())
            .spawn(move || {
                while alive_for_worker.load(Ordering::Acquire) {
                    let Ok((stream, _)) = listener.accept() else {
                        continue;
                    };
                    if !alive_for_worker.load(Ordering::Acquire) {
                        break;
                    }
                    handle_control_connection(stream, &token, &request_sender);
                }
            })
            .map_err(|error| format!("start EdgeSteer Agent control server: {error}"))?;
        Ok(Self {
            alive,
            address,
            worker: Some(worker),
        })
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn handle_control_connection(
    mut stream: TcpStream,
    token: &str,
    request_sender: &mpsc::Sender<ServerRequest>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    let read = stream
        .try_clone()
        .and_then(|stream| BufReader::new(stream).read_line(&mut line));
    let response = match read
        .map_err(|error| format!("read request: {error}"))
        .and_then(|_| {
            serde_json::from_str::<ControlRequest>(&line)
                .map_err(|error| format!("parse request: {error}"))
        }) {
        Ok(request) if request.protocol == CONTROL_PROTOCOL && request.token == token => {
            let (response_sender, response_receiver) = mpsc::channel();
            if request_sender
                .send(ServerRequest {
                    command: request.command,
                    response: response_sender,
                })
                .is_err()
            {
                AgentResponse {
                    ok: false,
                    message: "EdgeSteer Agent is shutting down".to_owned(),
                    status: AgentStatus::unavailable("Agent is shutting down"),
                }
            } else {
                response_receiver
                    .recv_timeout(Duration::from_secs(70))
                    .unwrap_or_else(|_| AgentResponse {
                        ok: false,
                        message: "EdgeSteer Agent did not finish the requested operation"
                            .to_owned(),
                        status: AgentStatus::unavailable("Agent operation timed out"),
                    })
            }
        }
        Ok(_) => AgentResponse {
            ok: false,
            message: "EdgeSteer Agent rejected an unauthenticated request".to_owned(),
            status: AgentStatus::unavailable("Unauthenticated request"),
        },
        Err(error) => AgentResponse {
            ok: false,
            message: format!("EdgeSteer Agent rejected the request: {error}"),
            status: AgentStatus::unavailable("Invalid request"),
        },
    };
    if let Ok(encoded) = serde_json::to_vec(&response) {
        let _ = stream.write_all(&encoded);
        let _ = stream.write_all(b"\n");
    }
}

enum LockState {
    Acquired(AgentLock),
    Existing,
}

struct AgentLock {
    state_dir: PathBuf,
    lock_path: PathBuf,
    record_path: PathBuf,
}

impl AgentLock {
    fn acquire(state_dir: &Path) -> Result<LockState, String> {
        fs::create_dir_all(state_dir).map_err(|error| {
            format!(
                "create EdgeSteer state directory {}: {error}",
                state_dir.display()
            )
        })?;
        let lock_path = state_dir.join(LOCK_FILE_NAME);
        let record_path = state_dir.join(CONTROL_FILE_NAME);

        for _ in 0..2 {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&lock_path) {
                Ok(mut file) => {
                    file.write_all(std::process::id().to_string().as_bytes())
                        .map_err(|error| format!("write EdgeSteer Agent lock: {error}"))?;
                    return Ok(LockState::Acquired(Self {
                        state_dir: state_dir.to_path_buf(),
                        lock_path,
                        record_path,
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if read_record(state_dir).is_ok_and(|record| process_is_alive(record.pid)) {
                        return Ok(LockState::Existing);
                    }
                    let _ = fs::remove_file(&lock_path);
                    let _ = fs::remove_file(&record_path);
                }
                Err(error) => {
                    return Err(format!(
                        "create EdgeSteer Agent lock {}: {error}",
                        lock_path.display()
                    ));
                }
            }
        }
        Err("could not acquire the EdgeSteer Agent lock".to_owned())
    }
}

impl Drop for AgentLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.record_path);
        let _ = fs::remove_file(&self.lock_path);
        let _ = fs::remove_dir(&self.state_dir);
    }
}

fn read_record(state_dir: &Path) -> Result<AgentRecord, String> {
    let path = state_dir.join(CONTROL_FILE_NAME);
    let contents = fs::read(&path)
        .map_err(|error| format!("read EdgeSteer Agent record {}: {error}", path.display()))?;
    serde_json::from_slice(&contents)
        .map_err(|error| format!("parse EdgeSteer Agent record {}: {error}", path.display()))
}

fn write_record(state_dir: &Path, record: &AgentRecord) -> Result<(), String> {
    let path = state_dir.join(CONTROL_FILE_NAME);
    let contents = serde_json::to_vec(record)
        .map_err(|error| format!("encode EdgeSteer Agent record: {error}"))?;
    write_private_file(&path, &contents)
}

fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary).map_err(|error| {
        format!(
            "create temporary EdgeSteer state file {}: {error}",
            temporary.display()
        )
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "write EdgeSteer state file {}: {error}",
                temporary.display()
            )
        })?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("publish EdgeSteer state file {}: {error}", path.display()))
}

fn random_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let bytes = rand::random::<[u8; 32]>();
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    token
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_token_is_hex_and_unpredictable_length() {
        let token = random_token();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn tray_presentation_requires_an_active_loopback_listener_for_system_dns() {
        let status = AgentStatus {
            engine_running: true,
            listener: "127.0.0.1:53".to_owned(),
            listener_ready: true,
            system_dns_enabled: false,
            autostart_enabled: false,
            configuration_error: None,
        };
        assert!(tray_presentation(&status, TrayLanguage::Chinese).system_dns_action_enabled);

        let status = AgentStatus {
            listener: "127.0.0.1:53535".to_owned(),
            ..status
        };
        assert!(!tray_presentation(&status, TrayLanguage::Chinese).system_dns_action_enabled);
    }
}
