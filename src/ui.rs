use std::{
    borrow::Cow,
    cell::Cell,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use iced::{
    Application, Background, Border, Color, Command, Element, Font, Length, Settings, Shadow,
    Theme, executor, theme,
    widget::{
        button, checkbox, column, container, horizontal_space, image, pick_list, row, scrollable,
        text as iced_text, text_input,
    },
    window,
};
use iced_aw::{FloatingElement, Modal, floating_element::Anchor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    agent::{
        AgentClient, AgentCommand, AgentResponse, AgentStatus, configure_menu_bar_activation_policy,
    },
    config::{self, FileConfig},
    integration::{self, IntegrationStatus},
};

const DEFAULT_CONFIG: &str = include_str!("../config.example.json");
const APP_LOGO: &[u8] = include_bytes!("../assets/edgesteer-logo.png");

#[derive(Debug, Clone)]
pub struct UiOptions {
    pub config_path: PathBuf,
    pub app_bundle: Option<PathBuf>,
    pub agent: AgentClient,
}

pub fn run(options: UiOptions) -> iced::Result {
    let mut settings = Settings::with_flags(options);
    settings.default_font = platform_ui_font();
    settings.window = window::Settings {
        size: iced::Size::new(1180.0, 780.0),
        min_size: Some(iced::Size::new(900.0, 620.0)),
        // A close request is handled by the configuration UI. It either exits
        // this disposable Iced process or asks the Agent to restore DNS first.
        exit_on_close_request: false,
        ..window::Settings::default()
    };
    EdgeSteerUi::run(settings)
}

fn platform_ui_font() -> Font {
    #[cfg(target_os = "macos")]
    {
        Font::with_name("PingFang SC")
    }

    #[cfg(target_os = "windows")]
    {
        Font::with_name("Microsoft YaHei UI")
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Font::with_name("Noto Sans CJK SC")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Language {
    Chinese,
    English,
}

impl Language {
    const ALL: [Self; 2] = [Self::Chinese, Self::English];

    const fn label(self) -> &'static str {
        match self {
            Self::Chinese => "简体中文",
            Self::English => "English",
        }
    }

    fn text(self, value: &'static str) -> &'static str {
        if self == Self::English {
            value
        } else {
            chinese_text(value).unwrap_or(value)
        }
    }

    fn translate(self, value: &str) -> Cow<'_, str> {
        if self == Self::English {
            Cow::Borrowed(value)
        } else if let Some(localized) = chinese_text(value) {
            Cow::Borrowed(localized)
        } else {
            Cow::Borrowed(value)
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

fn chinese_text(value: &str) -> Option<&'static str> {
    Some(match value {
        "Overview" => "概览",
        "Configuration" => "配置",
        "Settings" => "设置",
        "General" => "通用",
        "Layers" => "层级",
        "Rule sets" => "规则集",
        "Rule set" => "规则集",
        "JSON preview" => "JSON 预览",
        "Adaptive DNS steering" => "自适应 DNS 调度",
        "Navigation" => "导航",
        "Saved" => "已保存",
        "Valid draft" => "有效草稿",
        "Valid draft, not saved" => "有效草稿，尚未保存",
        "Valid and saved" => "有效且已保存",
        "Needs attention" => "需要处理",
        "Invalid" => "无效",
        "Configuration loaded" => "配置已加载",
        "Configuration saved" => "配置已保存",
        "DNS listener" => "DNS 监听器",
        "Runtime state" => "运行状态",
        "Accepting TCP DNS" => "正在接收 TCP DNS",
        "Not accepting TCP DNS" => "未接收 TCP DNS",
        "Listener unavailable" => "监听器不可用",
        "Checking listener" => "正在检查监听器",
        "Checking service" => "正在检查服务",
        "Not registered" => "未注册",
        "Automatic (DHCP)" => "自动（DHCP）",
        "not configured" => "未配置",
        "Entry layer" => "入口层",
        "Startup service" => "启动项",
        "Open at login" => "登录时打开",
        "Open at login enabled" => "已启用登录时打开",
        "Open at login is disabled" => "未启用登录时打开",
        "Legacy command-line service detected" => "检测到旧版命令行服务",
        "Remove legacy command-line service" => "移除旧版命令行服务",
        "No legacy EdgeSteer command-line service is registered" => {
            "未注册旧版 EdgeSteer 命令行服务"
        }
        "Legacy EdgeSteer command-line service removed" => "已移除旧版 EdgeSteer 命令行服务",
        "Configuration file" => "配置文件",
        "Schema status" => "结构校验",
        "The service and this control plane use the same fixed strict JSON document." => {
            "服务与控制界面使用同一份固定的严格 JSON 配置。"
        }
        "Load" => "加载",
        "Save configuration" => "保存配置",
        "Resolver graph" => "解析图",
        "Manage fallback layers, domain rules, and Cloudflare IP rewriting." => {
            "管理回退层、域名规则和 Cloudflare IP 改写。"
        }
        "Refresh runtime" => "刷新运行状态",
        "Select a layer" => "选择层级",
        "Resolver health, configuration state, and host integration at a glance." => {
            "集中查看解析器健康状态、配置状态和主机集成。"
        }
        "Online" => "在线",
        "Offline" => "离线",
        "Edit the resolver graph as a draft. Saving uses the DNS service schema without a translation layer." => {
            "以草稿方式编辑解析图。保存时直接使用 DNS 服务的原始配置结构。"
        }
        "Listener" => "监听器",
        "The local address that accepts UDP and TCP DNS requests." => {
            "接收 UDP 和 TCP DNS 请求的本地地址。"
        }
        "Allow remote resolver clients" => "允许远程解析客户端",
        "Routing" => "路由",
        "The entry layer starts the configured fallback graph." => "入口层启动已配置的回退图。",
        "Default entry layer" => "默认入口层",
        "Whole request timeout (ms)" => "整次请求超时（毫秒）",
        "Cloudflare range data" => "Cloudflare 网段数据",
        "Official Cloudflare network ranges used to validate rewriting candidates." => {
            "用于校验改写候选的 Cloudflare 官方网络范围。"
        }
        "Listener address and remote access changes require a service restart after saving." => {
            "修改监听地址或远程访问后，保存配置还需要重启服务。"
        }
        "Resolver layers" => "解析层",
        "Layer" => "层",
        "Order determines matching priority." => "顺序决定匹配优先级。",
        "Add layer" => "添加层",
        "Layer type" => "层类型",
        "Select a resolver layer to edit it." => "选择一个解析层进行编辑。",
        "The selected resolver layer is unavailable." => "所选解析层不可用。",
        "Resolver behavior, fallback, and domain targeting for this layer." => {
            "配置该层的解析行为、回退关系和域名匹配。"
        }
        "Fallback layer tag" => "回退层标识",
        "Bootstrap address" => "引导地址",
        "Plugin tag" => "插件标识",
        "TLS server name" => "TLS 服务器名称",
        "System DNS refresh (seconds)" => "系统 DNS 刷新间隔（秒）",
        "Move up" => "上移",
        "Move down" => "下移",
        "Domain match" => "域名匹配",
        "Use keywords or loaded SRS rule sets to select this layer before the default entry." => {
            "在默认入口之前，使用关键字或已加载的 SRS 规则集选择该层。"
        }
        "Match mode" => "匹配方式",
        "Remove layer" => "删除层",
        "Domain classifications loaded from SRS sources." => "从 SRS 来源加载的域名分类。",
        "Add rule set" => "添加规则集",
        "Source type" => "来源类型",
        "Select a rule set to edit it." => "选择一个规则集进行编辑。",
        "The selected rule set is unavailable." => "所选规则集不可用。",
        "A remote or local SRS source used by resolver-layer domain matching." => {
            "供解析层域名匹配使用的远程或本地 SRS 来源。"
        }
        "Download timeout (ms)" => "下载超时（毫秒）",
        "Local SRS path" => "本地 SRS 路径",
        "Remove rule set" => "删除规则集",
        "Cloudflare plugins" => "Cloudflare 插件",
        "CF plugins" => "CF 插件",
        "Plugin" => "插件",
        "Validated response rewriting and scheduled edge probing." => {
            "已验证的响应改写和定时边缘探测。"
        }
        "Cloudflare preferred" => "Cloudflare 优选",
        "Add Cloudflare preferred plugin" => "添加 Cloudflare 优选插件",
        "Select a Cloudflare preferred plugin to edit it." => {
            "选择一个 Cloudflare 优选插件进行编辑。"
        }
        "The selected plugin is unavailable." => "所选插件不可用。",
        "Rewrites confirmed Cloudflare responses with a compatible preferred edge address." => {
            "将已确认的 Cloudflare 响应改写为兼容的优选边缘地址。"
        }
        "Response rewrite" => "响应改写",
        "Edge optimizer" => "边缘优选器",
        "Probe Cloudflare candidates on a schedule and retain stable compatible results." => {
            "定时探测 Cloudflare 候选地址，并保留稳定的兼容结果。"
        }
        "Enable scheduled probe" => "启用定时探测",
        "Remove plugin" => "删除插件",
        "Generated from the current form draft. Saving validates before replacing the active file." => {
            "由当前表单草稿生成。保存前会先校验，再替换活动文件。"
        }
        "Schema valid" => "结构有效",
        "Schema invalid" => "结构无效",
        "Refresh" => "刷新",
        "Cancel" => "取消",
        "Continue" => "继续",
        "Confirm" => "确认",
        "Configured listener" => "已配置监听器",
        "Listener status" => "监听器状态",
        "The login item opens this App. It never installs a separate command-line DNS service." => {
            "登录项会打开此 App，不会安装独立的命令行 DNS 服务。"
        }
        "System DNS" => "系统 DNS",
        "This app contains the DNS engine. It can open at login and restores its managed DNS before closing." => {
            "DNS 引擎内置于此 App。它可在登录时打开，并会在关闭前恢复自己接管的 DNS。"
        }
        "DNS engine" => "DNS 引擎",
        "Restart only after saving listener changes. The resolver stays managed by EdgeSteer.app." => {
            "修改监听器后请先保存再重启。解析器始终由 EdgeSteer.app 管理。"
        }
        "Restart only after saving listener changes. The resolver is managed by the background EdgeSteer Agent." => {
            "修改监听器后请先保存再重启。解析器由后台 EdgeSteer Agent 管理。"
        }
        "Applying the requested system change. macOS may be waiting for administrator authorization." => {
            "正在应用系统变更。macOS 可能正在等待管理员授权。"
        }
        "Physical network services" => "物理网络服务",
        "No macOS physical DNS services are available from this platform integration." => {
            "此平台集成未发现可用的 macOS 物理 DNS 服务。"
        }
        "The UI only changes enabled services that are already using automatic DNS or EdgeSteer loopback DNS." => {
            "界面只会修改已启用且当前使用自动 DNS 或 EdgeSteer 回环 DNS 的服务。"
        }
        "Reading service and system DNS status..." => "正在读取服务和系统 DNS 状态...",
        "Error" => "错误",
        "Updated" => "已更新",
        "Tag" => "标识",
        "Address" => "地址",
        "Timeout (ms)" => "超时（毫秒）",
        "Refresh interval (seconds)" => "刷新间隔（秒）",
        "Keywords (comma separated)" => "关键字（逗号分隔）",
        "Rule set tags (comma separated)" => "规则集标识（逗号分隔）",
        "Rewrite TTL (seconds)" => "改写 TTL（秒）",
        "Preferred IPv4 (optional)" => "优选 IPv4（可选）",
        "Preferred IPv6 (optional)" => "优选 IPv6（可选）",
        "Probe interval (seconds)" => "探测间隔（秒）",
        "Probe host" => "探测主机",
        "Probe path" => "探测路径",
        "Probe port" => "探测端口",
        "Probe timeout (ms)" => "探测超时（毫秒）",
        "Concurrency" => "并发数",
        "Samples per CIDR" => "每个 CIDR 的样本数",
        "Probes per candidate" => "每个候选的探测次数",
        "Maximum candidates" => "最大候选数",
        "Candidate IPs/CIDRs (comma separated)" => "候选 IP/CIDR（逗号分隔）",
        "Compatibility hosts (comma separated)" => "兼容性验证主机（逗号分隔）",
        "Excluded candidate IPs/CIDRs (comma separated)" => "排除的候选 IP/CIDR（逗号分隔）",
        "Dynamic local DNS" => "动态本地 DNS",
        "Interceptor" => "拦截器",
        "Full DNS label" => "完整 DNS 标签",
        "Literal substring" => "字面子串",
        "Remote SRS" => "远程 SRS",
        "Local SRS" => "本地 SRS",
        "Enable open at login" => "启用登录时打开",
        "Disable open at login" => "关闭登录时打开",
        "Start DNS engine" => "启动 DNS 引擎",
        "Restart DNS engine" => "重启 DNS 引擎",
        "Enable EdgeSteer DNS" => "启用 EdgeSteer DNS",
        "Restore automatic DNS" => "恢复自动 DNS",
        "Stop DNS engine" => "停止 DNS 引擎",
        "DNS engine stopped" => "DNS 引擎已停止",
        "This restores EdgeSteer-managed system DNS first, then stops the DNS engine." => {
            "此操作会先恢复 EdgeSteer 接管的系统 DNS，再停止 DNS 引擎。"
        }
        "Open EdgeSteer" => "打开 EdgeSteer",
        "Quit EdgeSteer" => "退出 EdgeSteer",
        "Running" => "运行中",
        "Stopped" => "已停止",
        "DNS" => "DNS",
        "Language" => "语言",
        "Appearance" => "外观",
        "Application" => "应用",
        "Close window to menu bar" => "关闭窗口时保留在菜单栏",
        "Closing this window keeps the DNS engine running. Use Quit EdgeSteer from the menu bar when you need to stop it and restore managed system DNS." => {
            "关闭窗口后 DNS 引擎仍会继续运行。需要停止并恢复已接管的系统 DNS 时，请在菜单栏中选择“退出 EdgeSteer”。"
        }
        "Closing this window releases the GUI while EdgeSteer keeps running in the menu bar. Use Quit EdgeSteer from the menu bar when you need to stop it and restore managed system DNS." => {
            "关闭窗口会释放图形界面；EdgeSteer 会继续在菜单栏运行。需要停止并恢复已接管的系统 DNS 时，请在菜单栏中选择“退出 EdgeSteer”。"
        }
        "The menu bar is the primary control surface. This window is for configuration and detailed status." => {
            "菜单栏是主要控制入口；此窗口用于配置与查看详细状态。"
        }
        "The DNS engine remains active after this window closes unless you explicitly quit EdgeSteer." => {
            "除非明确退出 EdgeSteer，关闭此窗口后 DNS 引擎仍会继续运行。"
        }
        "System DNS is managed by EdgeSteer. It will be restored when you explicitly quit the app." => {
            "系统 DNS 当前由 EdgeSteer 接管；明确退出 App 时会自动恢复。"
        }
        "The listener is eligible for system DNS." => "监听器符合系统 DNS 接管要求。",
        "Exit EdgeSteer?" => "要退出 EdgeSteer 吗？",
        "EdgeSteer restores the system DNS it manages, then stops the DNS engine. Existing DNS requests may be interrupted." => {
            "EdgeSteer 会先恢复其接管的系统 DNS，再停止 DNS 引擎。正在进行的 DNS 请求可能会中断。"
        }
        "This opens the installed EdgeSteer app when you log in. It does not install a command-line DNS service." => {
            "此操作会在登录时打开已安装的 EdgeSteer App，不会安装命令行 DNS 服务。"
        }
        "This starts the DNS engine managed by the App. macOS may ask for administrator authorization for port 53." => {
            "此操作会启动由 App 管理的 DNS 引擎。监听 53 端口时，macOS 可能会请求管理员授权。"
        }
        "This removes EdgeSteer from your login startup items." => {
            "此操作会从登录启动项中移除 EdgeSteer。"
        }
        "This removes the pre-App root DNS daemon. macOS will ask for administrator authorization." => {
            "此操作会移除 App 之前使用的 root DNS 守护进程。macOS 会请求管理员授权。"
        }
        "This restarts the DNS engine inside the current EdgeSteer app." => {
            "此操作会重启当前 EdgeSteer App 内的 DNS 引擎。"
        }
        "disabled" => "已禁用",
        "EdgeSteer loopback DNS" => "EdgeSteer 回环 DNS",
        "Fix the configuration validation errors before changing the DNS engine or system DNS." => {
            "请先修复配置校验错误，再修改 DNS 引擎或系统 DNS。"
        }
        "System registration controls are currently available on macOS only. The DNS service and configuration UI remain portable." => {
            "系统注册控件目前仅在 macOS 可用；DNS 服务和配置界面仍可跨平台使用。"
        }
        "Fix the configuration validation errors before enabling system DNS." => {
            "请先修复配置校验错误，再启用系统 DNS。"
        }
        "System DNS requires EdgeSteer to listen on a loopback address at port 53." => {
            "系统 DNS 要求 EdgeSteer 在回环地址的 53 端口监听。"
        }
        "Keep EdgeSteer open until the configured loopback listener accepts TCP DNS." => {
            "请保持 EdgeSteer 运行，直到已配置的回环监听器开始接收 TCP DNS。"
        }
        "The listener is eligible for system DNS. EdgeSteer restores its own DNS changes before it closes." => {
            "监听器符合系统 DNS 要求。EdgeSteer 会在关闭前恢复自己修改的 DNS。"
        }
        "This saves the current configuration, installs the startup service, and starts EdgeSteer." => {
            "这会保存当前配置、安装启动服务并启动 EdgeSteer。"
        }
        "This restarts the registered EdgeSteer startup service." => {
            "这会重启已注册的 EdgeSteer 启动服务。"
        }
        "This stops and removes the EdgeSteer startup service. System DNS settings are left unchanged." => {
            "这会停止并移除 EdgeSteer 启动服务，系统 DNS 设置保持不变。"
        }
        "This changes enabled physical network services to use the configured loopback DNS listener." => {
            "这会让已启用的物理网络服务使用已配置的回环 DNS 监听器。"
        }
        "This removes EdgeSteer loopback DNS from enabled physical network services and returns them to automatic DNS." => {
            "这会从已启用的物理网络服务中移除 EdgeSteer 回环 DNS，并恢复自动 DNS。"
        }
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AppearanceMode {
    Dark,
    Light,
}

impl AppearanceMode {
    const ALL: [Self; 2] = [Self::Dark, Self::Light];

    fn label(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Dark, Language::Chinese) => "黑",
            (Self::Light, Language::Chinese) => "白",
            (Self::Dark, Language::English) => "Dark",
            (Self::Light, Language::English) => "Light",
        }
    }

    fn colors(self) -> UiColors {
        match self {
            Self::Dark => UiColors {
                background: Color::from_rgb8(30, 30, 29),
                surface: Color::from_rgb8(39, 39, 37),
                recessed: Color::from_rgb8(34, 34, 32),
                border: Color::from_rgb8(59, 59, 56),
                border_heavy: Color::from_rgb8(84, 82, 78),
                text: Color::from_rgb8(241, 241, 238),
                muted: Color::from_rgb8(185, 184, 180),
                quiet: Color::from_rgb8(135, 134, 129),
                // The product mark keeps Cloudflare orange. UI accents use a
                // softer companion tone so the controls do not compete with it.
                primary: Color::from_rgb8(190, 116, 78),
                success: Color::from_rgb8(181, 132, 104),
                warning: Color::from_rgb8(191, 151, 101),
                danger: Color::from_rgb8(194, 107, 96),
            },
            Self::Light => UiColors {
                background: Color::from_rgb8(251, 251, 249),
                surface: Color::from_rgb8(255, 255, 254),
                recessed: Color::from_rgb8(247, 247, 244),
                border: Color::from_rgb8(231, 231, 226),
                border_heavy: Color::from_rgb8(211, 210, 204),
                text: Color::from_rgb8(42, 42, 40),
                muted: Color::from_rgb8(105, 105, 100),
                quiet: Color::from_rgb8(153, 153, 147),
                primary: Color::from_rgb8(227, 177, 150),
                success: Color::from_rgb8(166, 121, 97),
                warning: Color::from_rgb8(179, 143, 96),
                danger: Color::from_rgb8(181, 101, 90),
            },
        }
    }
}

impl std::fmt::Display for AppearanceMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label(active_language()))
    }
}

#[derive(Debug, Clone, Copy)]
struct UiColors {
    background: Color,
    surface: Color,
    recessed: Color,
    border: Color,
    border_heavy: Color,
    text: Color,
    muted: Color,
    quiet: Color,
    primary: Color,
    success: Color,
    warning: Color,
    danger: Color,
}

fn colors_from_theme(theme: &Theme) -> UiColors {
    if theme.palette().background.r > 0.5 {
        AppearanceMode::Light.colors()
    } else {
        AppearanceMode::Dark.colors()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct UiPreferences {
    language: Language,
    appearance: AppearanceMode,
    close_to_menu_bar: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            language: Language::Chinese,
            appearance: AppearanceMode::Dark,
            close_to_menu_bar: true,
        }
    }
}

impl UiPreferences {
    fn load() -> Result<Self, String> {
        let path = ui_preferences_path();
        match fs::read(&path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map_err(|error| format!("read UI preferences {}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("read UI preferences {}: {error}", path.display())),
        }
    }

    fn save(&self) -> Result<(), String> {
        let path = ui_preferences_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create UI preferences directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("encode UI preferences: {error}"))?;
        fs::write(&path, contents)
            .map_err(|error| format!("save UI preferences {}: {error}", path.display()))
    }
}

fn ui_preferences_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from);

    #[cfg(target_os = "macos")]
    {
        home.map(|path| path.join("Library/Application Support/EdgeSteer/ui.json"))
            .unwrap_or_else(|| PathBuf::from("edgesteer-ui.json"))
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("EdgeSteer/ui.json"))
            .or_else(|| home.map(|path| path.join("AppData/Roaming/EdgeSteer/ui.json")))
            .unwrap_or_else(|| PathBuf::from("edgesteer-ui.json"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("edgesteer/ui.json"))
            .or_else(|| home.map(|path| path.join(".config/edgesteer/ui.json")))
            .unwrap_or_else(|| PathBuf::from("edgesteer-ui.json"))
    }
}

thread_local! {
    static ACTIVE_LANGUAGE: Cell<Language> = const { Cell::new(Language::Chinese) };
    static ACTIVE_APPEARANCE: Cell<AppearanceMode> = const { Cell::new(AppearanceMode::Dark) };
}

fn set_active_ui_context(language: Language, appearance: AppearanceMode) {
    ACTIVE_LANGUAGE.with(|active| active.set(language));
    ACTIVE_APPEARANCE.with(|active| active.set(appearance));
}

fn active_language() -> Language {
    ACTIVE_LANGUAGE.with(Cell::get)
}

fn active_appearance() -> AppearanceMode {
    ACTIVE_APPEARANCE.with(Cell::get)
}

fn active_colors() -> UiColors {
    active_appearance().colors()
}

// Iced's widget constructor normally borrows its input. Owning the localized
// text keeps every existing form control responsive to the selected language.
fn text(value: impl Into<String>) -> iced::widget::Text<'static> {
    let value = value.into();
    iced_text(active_language().translate(&value).into_owned())
}

fn primary_text() -> Color {
    active_colors().text
}

fn muted_text() -> Color {
    active_colors().muted
}

fn quiet_text() -> Color {
    active_colors().quiet
}

fn accent_color() -> Color {
    active_colors().primary
}

fn success_color() -> Color {
    active_colors().success
}

fn warning_color() -> Color {
    active_colors().warning
}

fn danger_color() -> Color {
    active_colors().danger
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    General,
    Layers,
    RuleSets,
    Cloudflare,
    Preview,
    Settings,
}

impl Page {
    const ALL: [Self; 7] = [
        Self::Overview,
        Self::General,
        Self::Layers,
        Self::RuleSets,
        Self::Cloudflare,
        Self::Preview,
        Self::Settings,
    ];

    fn label(self, language: Language) -> &'static str {
        language.text(match self {
            Self::Overview => "Overview",
            Self::General => "General",
            Self::Layers => "Layers",
            Self::RuleSets => "Rule sets",
            Self::Cloudflare => "Cloudflare",
            Self::Preview => "JSON preview",
            Self::Settings => "Settings",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Udp,
    Tcp,
    Doh,
    Dot,
    Local,
    Interceptor,
}

impl LayerKind {
    const ALL: [Self; 6] = [
        Self::Udp,
        Self::Tcp,
        Self::Doh,
        Self::Dot,
        Self::Local,
        Self::Interceptor,
    ];

    const fn wire_name(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Doh => "doh",
            Self::Dot => "dot",
            Self::Local => "local",
            Self::Interceptor => "interceptor",
        }
    }

    fn label(self, language: Language) -> &'static str {
        language.text(match self {
            Self::Udp => "UDP",
            Self::Tcp => "TCP",
            Self::Doh => "DoH",
            Self::Dot => "DoT",
            Self::Local => "Dynamic local DNS",
            Self::Interceptor => "Interceptor",
        })
    }

    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("udp") => Self::Udp,
            Some("tcp") => Self::Tcp,
            Some("doh") => Self::Doh,
            Some("dot") => Self::Dot,
            Some("interceptor") => Self::Interceptor,
            _ => Self::Local,
        }
    }
}

impl std::fmt::Display for LayerKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label(active_language()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchMode {
    Label,
    Contains,
}

impl MatchMode {
    const ALL: [Self; 2] = [Self::Label, Self::Contains];

    const fn wire_name(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Contains => "contains",
        }
    }

    fn label(self, language: Language) -> &'static str {
        language.text(match self {
            Self::Label => "Full DNS label",
            Self::Contains => "Literal substring",
        })
    }

    fn from_value(value: Option<&str>) -> Self {
        if value == Some("contains") {
            Self::Contains
        } else {
            Self::Label
        }
    }
}

impl std::fmt::Display for MatchMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label(active_language()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleSetKind {
    Remote,
    Local,
}

impl RuleSetKind {
    const ALL: [Self; 2] = [Self::Remote, Self::Local];

    const fn wire_name(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Local => "local",
        }
    }

    fn label(self, language: Language) -> &'static str {
        language.text(match self {
            Self::Remote => "Remote SRS",
            Self::Local => "Local SRS",
        })
    }

    fn from_value(value: Option<&str>) -> Self {
        if value == Some("local") {
            Self::Local
        } else {
            Self::Remote
        }
    }
}

impl std::fmt::Display for RuleSetKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label(active_language()))
    }
}

#[derive(Debug, Clone)]
enum Message {
    SelectPage(Page),
    SelectLanguage(Language),
    SelectAppearance(AppearanceMode),
    CloseToMenuBarChanged(bool),
    LoadConfig,
    SaveConfig,
    ListenerAddressChanged(String),
    AllowRemoteChanged(bool),
    RequestTimeoutChanged(String),
    RangeRefreshChanged(String),
    EntryChanged(String),
    SelectLayer(usize),
    AddLayer(LayerKind),
    RemoveLayer(usize),
    MoveLayerUp(usize),
    MoveLayerDown(usize),
    LayerTagChanged(usize, String),
    LayerTypeChanged(usize, LayerKind),
    LayerFallbackChanged(usize, String),
    LayerPluginChanged(usize, String),
    LayerAddressChanged(usize, String),
    LayerUrlChanged(usize, String),
    LayerServerNameChanged(usize, String),
    LayerTimeoutChanged(usize, String),
    LayerRefreshChanged(usize, String),
    LayerMatchModeChanged(usize, MatchMode),
    LayerKeywordsChanged(usize, String),
    LayerRuleSetsChanged(usize, String),
    SelectRuleSet(usize),
    AddRuleSet(RuleSetKind),
    RemoveRuleSet(usize),
    RuleSetTagChanged(usize, String),
    RuleSetTypeChanged(usize, RuleSetKind),
    RuleSetSourceChanged(usize, String),
    RuleSetIntervalChanged(usize, String),
    RuleSetTimeoutChanged(usize, String),
    SelectPlugin(usize),
    AddPlugin,
    RemovePlugin(usize),
    PluginTagChanged(usize, String),
    PluginTtlChanged(usize, String),
    PluginIpv4Changed(usize, String),
    PluginIpv6Changed(usize, String),
    OptimizerEnabledChanged(usize, bool),
    OptimizerFieldChanged(usize, &'static str, String),
    OptimizerListChanged(usize, &'static str, String),
    RefreshIntegration,
    IntegrationChecked(Result<IntegrationStatus, String>),
    RequestRegistrationAction(RegistrationAction),
    ConfirmPendingAction,
    CancelPendingAction,
    AgentActionFinished(Result<AgentResponse, String>),
    AgentStatusChecked(Result<AgentStatus, String>),
    AgentConfigurationRefreshed(Result<AgentResponse, String>),
    UiTick,
    WindowCloseRequested,
    QuitAgentFinished(Result<AgentResponse, String>),
}

struct EdgeSteerUi {
    page: Page,
    language: Language,
    appearance: AppearanceMode,
    preferences: UiPreferences,
    document: ConfigDocument,
    app_bundle: Option<PathBuf>,
    agent: AgentClient,
    agent_status: Option<AgentStatus>,
    selected_layer: Option<usize>,
    selected_rule_set: Option<usize>,
    selected_plugin: Option<usize>,
    integration: Option<IntegrationStatus>,
    notice: Option<Notice>,
    pending_action: Option<PendingAction>,
    registration_action_in_progress: bool,
    quit_in_progress: bool,
    macos_activation_policy_configured: bool,
}

#[derive(Debug, Clone)]
struct Notice {
    text: String,
    expires_at: Instant,
}

impl Notice {
    const DISPLAY_DURATION: Duration = Duration::from_secs(5);

    fn show(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            expires_at: Instant::now() + Self::DISPLAY_DURATION,
        }
    }

    // Every runtime result uses the same unobtrusive top-level presentation.
    // The message itself retains the actionable error detail when needed.
    fn success(text: impl Into<String>) -> Self {
        Self::show(text)
    }

    fn error(text: impl Into<String>) -> Self {
        Self::show(text)
    }

    fn expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistrationAction {
    StartEngine,
    StopEngine,
    EnableAutoStart,
    DisableAutoStart,
    RemoveLegacyService,
    Restart,
    EnableSystemDns,
    DisableSystemDns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingAction {
    Registration(RegistrationAction),
    Quit,
}

impl RegistrationAction {
    fn label(self, language: Language) -> &'static str {
        language.text(match self {
            Self::StartEngine => "Start DNS engine",
            Self::StopEngine => "Stop DNS engine",
            Self::EnableAutoStart => "Enable open at login",
            Self::DisableAutoStart => "Disable open at login",
            Self::RemoveLegacyService => "Remove legacy command-line service",
            Self::Restart => "Restart DNS engine",
            Self::EnableSystemDns => "Enable EdgeSteer DNS",
            Self::DisableSystemDns => "Restore automatic DNS",
        })
    }

    fn confirmation(self, language: Language) -> &'static str {
        language.text(match self {
            Self::StartEngine => {
                "This starts the DNS engine managed by the App. macOS may ask for administrator authorization for port 53."
            }
            Self::StopEngine => {
                "This restores EdgeSteer-managed system DNS first, then stops the DNS engine."
            }
            Self::EnableAutoStart => {
                "This opens the installed EdgeSteer app when you log in. It does not install a command-line DNS service."
            }
            Self::DisableAutoStart => "This removes EdgeSteer from your login startup items.",
            Self::RemoveLegacyService => {
                "This removes the pre-App root DNS daemon. macOS will ask for administrator authorization."
            }
            Self::Restart => "This restarts the DNS engine inside the current EdgeSteer app.",
            Self::EnableSystemDns => {
                "This changes enabled physical network services to use the configured loopback DNS listener."
            }
            Self::DisableSystemDns => {
                "This removes EdgeSteer loopback DNS from enabled physical network services and returns them to automatic DNS."
            }
        })
    }

    const fn button_style(self) -> theme::Button {
        match self {
            Self::StartEngine | Self::EnableAutoStart | Self::Restart => theme::Button::Primary,
            Self::EnableSystemDns => theme::Button::Positive,
            Self::StopEngine
            | Self::DisableAutoStart
            | Self::RemoveLegacyService
            | Self::DisableSystemDns => theme::Button::Destructive,
        }
    }
}

fn topbar_style(theme: &Theme) -> iced::widget::container::Appearance {
    let colors = colors_from_theme(theme);
    iced::widget::container::Appearance {
        background: Some(Background::Color(colors.surface)),
        border: Border {
            color: colors.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        text_color: None,
    }
}

fn panel_style(theme: &Theme) -> iced::widget::container::Appearance {
    let colors = colors_from_theme(theme);
    iced::widget::container::Appearance {
        background: Some(Background::Color(colors.surface)),
        border: Border {
            color: colors.border,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow::default(),
        text_color: None,
    }
}

fn recessed_panel_style(theme: &Theme) -> iced::widget::container::Appearance {
    let colors = colors_from_theme(theme);
    iced::widget::container::Appearance {
        background: Some(Background::Color(colors.recessed)),
        border: Border {
            color: colors.border,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow::default(),
        text_color: None,
    }
}

fn toast_style(theme: &Theme) -> iced::widget::container::Appearance {
    let colors = colors_from_theme(theme);
    iced::widget::container::Appearance {
        background: Some(Background::Color(colors.surface)),
        border: Border {
            color: colors.border_heavy,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow::default(),
        text_color: None,
    }
}

impl Application for EdgeSteerUi {
    type Executor = executor::Default;
    type Flags = UiOptions;
    type Message = Message;
    type Theme = Theme;

    fn new(options: Self::Flags) -> (Self, Command<Self::Message>) {
        let (document, notice) = ConfigDocument::load(options.config_path);
        let (preferences, preferences_notice) = match UiPreferences::load() {
            Ok(preferences) => (preferences, None),
            Err(error) => (UiPreferences::default(), Some(Notice::error(error))),
        };
        let listener = document.listener_address();
        let agent_for_status = options.agent.clone();
        (
            Self {
                page: Page::Overview,
                language: preferences.language,
                appearance: preferences.appearance,
                preferences,
                document,
                app_bundle: options.app_bundle,
                agent: options.agent,
                agent_status: None,
                selected_layer: Some(0),
                selected_rule_set: Some(0),
                selected_plugin: Some(0),
                integration: None,
                notice: notice.or(preferences_notice),
                pending_action: None,
                registration_action_in_progress: false,
                quit_in_progress: false,
                macos_activation_policy_configured: false,
            },
            Command::batch([
                inspect_integration(listener),
                Command::perform(
                    async move { agent_for_status.status() },
                    Message::AgentStatusChecked,
                ),
            ]),
        )
    }

    fn title(&self) -> String {
        "EdgeSteer".to_owned()
    }

    fn theme(&self) -> Self::Theme {
        let colors = self.colors();
        Theme::custom(
            format!("EdgeSteer {}", self.appearance.label(self.language)),
            theme::Palette {
                background: colors.background,
                text: colors.text,
                primary: colors.primary,
                success: colors.success,
                danger: colors.danger,
            },
        )
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::SelectPage(page) => self.page = page,
            Message::SelectLanguage(language) => {
                self.language = language;
                self.preferences.language = language;
                self.persist_preferences();
            }
            Message::SelectAppearance(appearance) => {
                self.appearance = appearance;
                self.preferences.appearance = appearance;
                self.persist_preferences();
            }
            Message::CloseToMenuBarChanged(value) => {
                self.preferences.close_to_menu_bar = value;
                self.persist_preferences();
            }
            Message::LoadConfig => match self.document.load_current_path() {
                Ok(()) => {
                    self.selected_layer = Some(0);
                    self.selected_rule_set = Some(0);
                    self.selected_plugin = Some(0);
                    self.notice = Some(Notice::success("Configuration loaded"));
                    return Command::batch([
                        inspect_integration(self.document.listener_address()),
                        refresh_agent_configuration(self.agent.clone()),
                    ]);
                }
                Err(error) => self.notice = Some(Notice::error(error)),
            },
            Message::SaveConfig => match self.document.save() {
                Ok(()) => {
                    self.notice = Some(Notice::success("Configuration saved"));
                    return Command::batch([
                        inspect_integration(self.document.listener_address()),
                        refresh_agent_configuration(self.agent.clone()),
                    ]);
                }
                Err(error) => self.notice = Some(Notice::error(error)),
            },
            Message::ListenerAddressChanged(value) => self.document.set_listener_address(value),
            Message::AllowRemoteChanged(value) => self.document.set_listener_allow_remote(value),
            Message::RequestTimeoutChanged(value) => {
                self.document.set_top_number("request_timeout_ms", value)
            }
            Message::RangeRefreshChanged(value) => self
                .document
                .set_cloudflare_number("range_refresh_secs", value),
            Message::EntryChanged(value) => self.document.set_top_string("entry", value),
            Message::SelectLayer(index) => self.selected_layer = Some(index),
            Message::AddLayer(kind) => self.selected_layer = Some(self.document.add_layer(kind)),
            Message::RemoveLayer(index) => {
                self.document.remove_layer(index);
                self.selected_layer = self.document.layers().len().checked_sub(1);
            }
            Message::MoveLayerUp(index) => {
                self.document.move_layer(index, true);
                self.selected_layer = Some(index.saturating_sub(1));
            }
            Message::MoveLayerDown(index) => {
                self.document.move_layer(index, false);
                self.selected_layer = Some(index.saturating_add(1));
            }
            Message::LayerTagChanged(index, value) => {
                self.document.set_layer_string(index, "tag", value)
            }
            Message::LayerTypeChanged(index, kind) => self.document.set_layer_type(index, kind),
            Message::LayerFallbackChanged(index, value) => self
                .document
                .set_layer_optional_string(index, "fallback", value),
            Message::LayerPluginChanged(index, value) => self
                .document
                .set_layer_optional_string(index, "plugin", value),
            Message::LayerAddressChanged(index, value) => self
                .document
                .set_layer_optional_string(index, "address", value),
            Message::LayerUrlChanged(index, value) => {
                self.document.set_layer_optional_string(index, "url", value)
            }
            Message::LayerServerNameChanged(index, value) => self
                .document
                .set_layer_optional_string(index, "server_name", value),
            Message::LayerTimeoutChanged(index, value) => {
                self.document.set_layer_number(index, "timeout_ms", value)
            }
            Message::LayerRefreshChanged(index, value) => {
                self.document.set_layer_number(index, "refresh_secs", value)
            }
            Message::LayerMatchModeChanged(index, mode) => {
                self.document.set_layer_match_mode(index, mode)
            }
            Message::LayerKeywordsChanged(index, value) => {
                self.document.set_layer_match_list(index, "keywords", value)
            }
            Message::LayerRuleSetsChanged(index, value) => {
                self.document
                    .set_layer_match_list(index, "rule_sets", value)
            }
            Message::SelectRuleSet(index) => self.selected_rule_set = Some(index),
            Message::AddRuleSet(kind) => {
                self.selected_rule_set = Some(self.document.add_rule_set(kind))
            }
            Message::RemoveRuleSet(index) => {
                self.document.remove_rule_set(index);
                self.selected_rule_set = self.document.rule_sets().len().checked_sub(1);
            }
            Message::RuleSetTagChanged(index, value) => {
                self.document.set_rule_set_string(index, "tag", value)
            }
            Message::RuleSetTypeChanged(index, kind) => {
                self.document.set_rule_set_type(index, kind)
            }
            Message::RuleSetSourceChanged(index, value) => {
                self.document.set_rule_set_source(index, value)
            }
            Message::RuleSetIntervalChanged(index, value) => {
                self.document
                    .set_rule_set_number(index, "update_interval_secs", value)
            }
            Message::RuleSetTimeoutChanged(index, value) => {
                self.document
                    .set_rule_set_number(index, "timeout_ms", value)
            }
            Message::SelectPlugin(index) => self.selected_plugin = Some(index),
            Message::AddPlugin => self.selected_plugin = Some(self.document.add_plugin()),
            Message::RemovePlugin(index) => {
                self.document.remove_plugin(index);
                self.selected_plugin = self.document.plugins().len().checked_sub(1);
            }
            Message::PluginTagChanged(index, value) => {
                self.document.set_plugin_string(index, "tag", value)
            }
            Message::PluginTtlChanged(index, value) => {
                self.document
                    .set_plugin_number(index, "rewrite_ttl_secs", value)
            }
            Message::PluginIpv4Changed(index, value) => {
                self.document.set_plugin_preferred(index, "ipv4", value)
            }
            Message::PluginIpv6Changed(index, value) => {
                self.document.set_plugin_preferred(index, "ipv6", value)
            }
            Message::OptimizerEnabledChanged(index, value) => {
                self.document.set_optimizer_bool(index, "enabled", value)
            }
            Message::OptimizerFieldChanged(index, field, value) => {
                self.document.set_optimizer_field(index, field, value)
            }
            Message::OptimizerListChanged(index, field, value) => {
                self.document.set_optimizer_list(index, field, value)
            }
            Message::RefreshIntegration => {
                return Command::batch([
                    inspect_integration(self.document.listener_address()),
                    inspect_agent_status(self.agent.clone()),
                ]);
            }
            Message::IntegrationChecked(result) => match result {
                Ok(status) => self.integration = Some(status),
                Err(error) => self.notice = Some(Notice::error(error)),
            },
            Message::AgentStatusChecked(result) => match result {
                Ok(status) => self.agent_status = Some(status),
                Err(error) => self.notice = Some(Notice::error(error)),
            },
            Message::AgentConfigurationRefreshed(result) => match result {
                Ok(response) if response.ok => self.agent_status = Some(response.status),
                Ok(response) => {
                    self.agent_status = Some(response.status);
                    self.notice = Some(Notice::error(response.message));
                }
                Err(error) => self.notice = Some(Notice::error(error)),
            },
            Message::RequestRegistrationAction(action) => {
                if !self.registration_action_in_progress {
                    self.pending_action = Some(PendingAction::Registration(action));
                }
            }
            Message::ConfirmPendingAction => {
                if !self.registration_action_in_progress && !self.quit_in_progress {
                    if let Some(action) = self.pending_action.take() {
                        return match action {
                            PendingAction::Registration(action) => {
                                self.start_registration_action(action)
                            }
                            PendingAction::Quit => self.begin_quit(),
                        };
                    }
                }
            }
            Message::CancelPendingAction => self.pending_action = None,
            Message::AgentActionFinished(result) => {
                self.registration_action_in_progress = false;
                match result {
                    Ok(response) if response.ok => {
                        self.agent_status = Some(response.status);
                        self.notice = Some(Notice::success(response.message));
                    }
                    Ok(response) => {
                        self.agent_status = Some(response.status);
                        self.notice = Some(Notice::error(response.message));
                    }
                    Err(error) => self.notice = Some(Notice::error(error)),
                }
                return inspect_integration(self.document.listener_address());
            }
            Message::UiTick => {
                #[cfg(target_os = "macos")]
                if !self.macos_activation_policy_configured {
                    // Iced 0.12 creates winit with its default Regular policy
                    // before Application::new runs. Apply Accessory only after
                    // the event loop has finished launching, otherwise winit
                    // overwrites it and macOS keeps a Dock icon.
                    configure_menu_bar_activation_policy();
                    self.macos_activation_policy_configured = true;
                }
                if self.notice.as_ref().is_some_and(Notice::expired) {
                    self.notice = None;
                }
            }
            Message::WindowCloseRequested => {
                if self.preferences.close_to_menu_bar {
                    return window::close(window::Id::MAIN);
                }
                if !self.quit_in_progress {
                    self.pending_action = Some(PendingAction::Quit);
                }
            }
            Message::QuitAgentFinished(result) => match result {
                Ok(response) if response.ok => {
                    self.agent_status = Some(response.status);
                    return window::close(window::Id::MAIN);
                }
                Ok(response) => {
                    self.quit_in_progress = false;
                    self.agent_status = Some(response.status);
                    self.notice = Some(Notice::error(response.message));
                }
                Err(error) => {
                    self.quit_in_progress = false;
                    self.notice = Some(Notice::error(format!(
                        "EdgeSteer remains open because automatic DNS could not be restored: {error}"
                    )));
                }
            },
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        iced::Subscription::batch([
            iced::event::listen_with(application_lifecycle_event),
            iced::time::every(Duration::from_millis(250)).map(|_| Message::UiTick),
        ])
    }

    fn view(&self) -> Element<Self::Message> {
        set_active_ui_context(self.language, self.appearance);
        let document_state = if !self.document.is_valid() {
            Some(("Needs attention", danger_color()))
        } else if self.document.is_dirty() {
            Some(("Valid draft", accent_color()))
        } else {
            None
        };
        let navigation = Page::ALL
            .into_iter()
            .fold(row![].spacing(4), |navigation, page| {
                let active = self.page == page;
                navigation.push(
                    button(text(page.label(self.language)).size(14))
                        .padding([7, 9])
                        .style(if active {
                            theme::Button::Secondary
                        } else {
                            theme::Button::Text
                        })
                        .on_press(Message::SelectPage(page)),
                )
            });

        let content = match self.page {
            Page::Overview => self.view_overview(),
            Page::General => scrollable(self.view_general_configuration())
                .height(Length::Fill)
                .into(),
            Page::Layers => scrollable(self.view_layers_configuration())
                .height(Length::Fill)
                .into(),
            Page::RuleSets => scrollable(self.view_rule_sets_configuration())
                .height(Length::Fill)
                .into(),
            Page::Cloudflare => scrollable(self.view_cloudflare_configuration())
                .height(Length::Fill)
                .into(),
            Page::Preview => scrollable(self.view_json_preview())
                .height(Length::Fill)
                .into(),
            Page::Settings => self.view_settings(),
        };

        let topbar_actions = match document_state {
            Some((state, color)) => row![
                text(state).size(12).style(theme::Text::Color(color)),
                save_button(self.document.is_valid()),
            ]
            .spacing(10)
            .align_items(iced::Alignment::Center),
            None => {
                row![save_button(self.document.is_valid())].align_items(iced::Alignment::Center)
            }
        };

        let topbar = container(
            row![
                image(image::Handle::from_memory(APP_LOGO))
                    .width(Length::Fixed(26.0))
                    .height(Length::Fixed(26.0)),
                text("EdgeSteer").size(19),
                navigation,
                horizontal_space(),
                topbar_actions,
            ]
            .spacing(11)
            .align_items(iced::Alignment::Center),
        )
        .padding([10, 24])
        .width(Length::Fill)
        .style(topbar_style);

        let base: Element<_> = column![
            topbar,
            container(content)
                .padding([24, 32, 28, 32])
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let toast = self
            .notice
            .as_ref()
            .map(notice_view)
            .unwrap_or_else(|| container(iced_text("")).into());
        let base = FloatingElement::new(base, toast)
            .anchor(Anchor::North)
            .offset([0.0, 18.0])
            .hide(self.notice.is_none());

        Modal::new(base, self.view_pending_action_modal())
            .backdrop(Message::CancelPendingAction)
            .on_esc(Message::CancelPendingAction)
            .into()
    }
}

impl EdgeSteerUi {
    fn colors(&self) -> UiColors {
        self.appearance.colors()
    }

    fn persist_preferences(&mut self) {
        if let Err(error) = self.preferences.save() {
            self.notice = Some(Notice::error(error));
        }
    }

    fn engine_running(&self) -> bool {
        self.agent_status
            .as_ref()
            .is_some_and(|status| status.engine_running)
    }

    fn begin_quit(&mut self) -> Command<Message> {
        self.quit_in_progress = true;
        self.notice = Some(Notice::success(
            "Restoring automatic DNS before closing EdgeSteer...",
        ));
        let agent = self.agent.clone();
        Command::perform(
            async move { agent.request(AgentCommand::Quit) },
            Message::QuitAgentFinished,
        )
    }

    fn view_pending_action_modal(&self) -> Option<Element<Message>> {
        let pending_action = self.pending_action?;
        let (title, detail, confirm_label, style) = match pending_action {
            PendingAction::Registration(action) => (
                format!(
                    "{}: {}",
                    self.language.text("Confirm"),
                    action.label(self.language)
                ),
                action.confirmation(self.language).to_owned(),
                self.language.text("Continue"),
                action.button_style(),
            ),
            PendingAction::Quit => (
                self.language.text("Exit EdgeSteer?").to_owned(),
                self.language
                    .text("EdgeSteer restores the system DNS it manages, then stops the DNS engine. Existing DNS requests may be interrupted.")
                    .to_owned(),
                self.language.text("Quit EdgeSteer"),
                theme::Button::Destructive,
            ),
        };

        Some(
            container(
                column![
                    text(title).size(20),
                    text(detail)
                        .size(14)
                        .style(theme::Text::Color(muted_text())),
                    row![
                        button(text("Cancel"))
                            .padding([9, 14])
                            .style(theme::Button::Secondary)
                            .on_press(Message::CancelPendingAction),
                        button(text(confirm_label))
                            .padding([9, 14])
                            .style(style)
                            .on_press(Message::ConfirmPendingAction),
                    ]
                    .spacing(8),
                ]
                .spacing(16),
            )
            .padding(22)
            .width(Length::Fixed(460.0))
            .style(panel_style)
            .into(),
        )
    }

    fn start_registration_action(&mut self, action: RegistrationAction) -> Command<Message> {
        if matches!(
            action,
            RegistrationAction::StartEngine
                | RegistrationAction::EnableAutoStart
                | RegistrationAction::Restart
                | RegistrationAction::EnableSystemDns
        ) && !self.document.is_valid()
        {
            self.notice = Some(Notice::error(
                "Fix the configuration validation errors before changing the DNS engine or system DNS.",
            ));
            return Command::none();
        }

        self.registration_action_in_progress = true;
        self.notice = Some(Notice::success(match self.language {
            Language::Chinese => format!("{}正在执行...", action.label(self.language)),
            Language::English => format!("{} in progress...", action.label(self.language)),
        }));

        let command = match action {
            RegistrationAction::StartEngine => AgentCommand::StartEngine,
            RegistrationAction::StopEngine => AgentCommand::StopEngine,
            RegistrationAction::EnableAutoStart => AgentCommand::EnableAutoStart,
            RegistrationAction::DisableAutoStart => AgentCommand::DisableAutoStart,
            RegistrationAction::RemoveLegacyService => AgentCommand::RemoveLegacyService,
            RegistrationAction::Restart => AgentCommand::RestartEngine,
            RegistrationAction::EnableSystemDns => AgentCommand::EnableSystemDns,
            RegistrationAction::DisableSystemDns => AgentCommand::DisableSystemDns,
        };
        let agent = self.agent.clone();
        Command::perform(
            async move { agent.request(command) },
            Message::AgentActionFinished,
        )
    }
}

fn application_lifecycle_event(
    event: iced::Event,
    _status: iced::event::Status,
) -> Option<Message> {
    matches!(event, iced::Event::Window(_, window::Event::CloseRequested))
        .then_some(Message::WindowCloseRequested)
}

fn inspect_integration(listener: std::net::SocketAddr) -> Command<Message> {
    Command::perform(
        async move { integration::inspect(listener).map_err(|error| format!("{error:#}")) },
        Message::IntegrationChecked,
    )
}

fn inspect_agent_status(agent: AgentClient) -> Command<Message> {
    Command::perform(async move { agent.status() }, Message::AgentStatusChecked)
}

fn refresh_agent_configuration(agent: AgentClient) -> Command<Message> {
    Command::perform(
        async move { agent.request(AgentCommand::Refresh) },
        Message::AgentConfigurationRefreshed,
    )
}

struct ConfigDocument {
    config_path: String,
    value: Value,
    serialized: String,
    validated: Option<FileConfig>,
    validation_error: Option<String>,
    dirty: bool,
}

impl ConfigDocument {
    fn load(path: PathBuf) -> (Self, Option<Notice>) {
        let path_text = path.display().to_string();
        match fs::read_to_string(&path) {
            Ok(contents) => match Self::from_contents(path_text, &contents) {
                Ok(document) => (document, None),
                Err(error) => (
                    Self::from_default(path.display().to_string()),
                    Some(Notice::error(error)),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                Self::from_default(path.display().to_string()),
                Some(Notice::success(
                    "Configuration file does not exist; loaded the bundled default as a draft",
                )),
            ),
            Err(error) => (
                Self::from_default(path.display().to_string()),
                Some(Notice::error(format!(
                    "Read configuration {}: {error}",
                    path.display()
                ))),
            ),
        }
    }

    fn from_default(config_path: String) -> Self {
        Self::from_contents(config_path, DEFAULT_CONFIG)
            .expect("bundled EdgeSteer configuration must be valid")
    }

    fn from_contents(config_path: String, contents: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(contents)
            .map_err(|error| format!("Parse JSON configuration: {error}"))?;
        let mut document = Self {
            config_path,
            value,
            serialized: String::new(),
            validated: None,
            validation_error: None,
            dirty: false,
        };
        document.revalidate();
        if let Some(error) = &document.validation_error {
            return Err(error.clone());
        }
        Ok(document)
    }

    fn load_current_path(&mut self) -> Result<(), String> {
        let path = PathBuf::from(self.config_path.trim());
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("Read configuration {}: {error}", path.display()))?;
        let loaded = Self::from_contents(path.display().to_string(), &contents)?;
        *self = loaded;
        Ok(())
    }

    fn save(&mut self) -> Result<(), String> {
        self.revalidate();
        if let Some(error) = &self.validation_error {
            return Err(error.clone());
        }
        let path = Path::new(self.config_path.trim());
        config::write_config_atomically(path, &self.serialized)
            .map_err(|error| format!("Save configuration {}: {error:#}", path.display()))?;
        self.dirty = false;
        Ok(())
    }

    fn listener_address(&self) -> std::net::SocketAddr {
        self.validated
            .as_ref()
            .map(|config| config.listener.address)
            .unwrap_or_else(|| "127.0.0.1:53".parse().expect("valid fallback listener"))
    }

    fn validation_summary(&self) -> String {
        match &self.validation_error {
            Some(error) => format!("{}: {error}", active_language().text("Schema invalid")),
            None if self.dirty => "Valid draft, not saved".to_owned(),
            None => "Valid and saved".to_owned(),
        }
    }

    fn is_valid(&self) -> bool {
        self.validation_error.is_none()
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn layers(&self) -> &[Value] {
        self.value
            .get("layers")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn rule_sets(&self) -> &[Value] {
        self.value
            .get("rule_sets")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn plugins(&self) -> &[Value] {
        self.value
            .get("plugins")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn layer(&self, index: usize) -> Option<&Map<String, Value>> {
        self.layers().get(index)?.as_object()
    }

    fn rule_set(&self, index: usize) -> Option<&Map<String, Value>> {
        self.rule_sets().get(index)?.as_object()
    }

    fn plugin(&self, index: usize) -> Option<&Map<String, Value>> {
        self.plugins().get(index)?.as_object()
    }

    fn set_listener_address(&mut self, value: String) {
        self.listener_mut()
            .insert("address".to_owned(), Value::String(value));
        self.changed();
    }

    fn set_listener_allow_remote(&mut self, value: bool) {
        self.listener_mut()
            .insert("allow_remote".to_owned(), Value::Bool(value));
        self.changed();
    }

    fn set_top_string(&mut self, key: &str, value: String) {
        self.root_mut().insert(key.to_owned(), Value::String(value));
        self.changed();
    }

    fn set_top_number(&mut self, key: &str, value: String) {
        if let Some(value) = parse_u64(&value) {
            self.root_mut().insert(key.to_owned(), json!(value));
            self.changed();
        }
    }

    fn set_cloudflare_number(&mut self, key: &str, value: String) {
        if let Some(value) = parse_u64(&value) {
            self.cloudflare_mut().insert(key.to_owned(), json!(value));
            self.changed();
        }
    }

    fn add_layer(&mut self, kind: LayerKind) -> usize {
        let tags = self.layer_tags();
        let tag = unique_tag("new-layer", &tags);
        let layer = default_layer(&tag, kind);
        let layers = self.layers_mut();
        layers.push(layer);
        let index = layers.len() - 1;
        self.changed();
        index
    }

    fn remove_layer(&mut self, index: usize) {
        let layers = self.layers_mut();
        if index < layers.len() {
            layers.remove(index);
            self.changed();
        }
    }

    fn move_layer(&mut self, index: usize, upward: bool) {
        let layers = self.layers_mut();
        let destination = if upward {
            index.checked_sub(1)
        } else {
            index
                .checked_add(1)
                .filter(|destination| *destination < layers.len())
        };
        if let Some(destination) = destination {
            layers.swap(index, destination);
            self.changed();
        }
    }

    fn set_layer_string(&mut self, index: usize, key: &str, value: String) {
        if let Some(layer) = self.layer_mut(index) {
            layer.insert(key.to_owned(), Value::String(value));
            self.changed();
        }
    }

    fn set_layer_optional_string(&mut self, index: usize, key: &str, value: String) {
        if let Some(layer) = self.layer_mut(index) {
            if value.trim().is_empty() {
                layer.remove(key);
            } else {
                layer.insert(key.to_owned(), Value::String(value));
            }
            self.changed();
        }
    }

    fn set_layer_number(&mut self, index: usize, key: &str, value: String) {
        if let Some(value) = parse_u64(&value) {
            if let Some(layer) = self.layer_mut(index) {
                layer.insert(key.to_owned(), json!(value));
                self.changed();
            }
        }
    }

    fn set_layer_type(&mut self, index: usize, kind: LayerKind) {
        if let Some(layer) = self.layer_mut(index) {
            layer.insert(
                "type".to_owned(),
                Value::String(kind.wire_name().to_owned()),
            );
            normalize_layer(layer, kind);
            self.changed();
        }
    }

    fn set_layer_match_mode(&mut self, index: usize, mode: MatchMode) {
        if let Some(layer) = self.layer_mut(index) {
            layer_match_mut(layer).insert(
                "mode".to_owned(),
                Value::String(mode.wire_name().to_owned()),
            );
            self.changed();
        }
    }

    fn set_layer_match_list(&mut self, index: usize, key: &str, value: String) {
        if let Some(layer) = self.layer_mut(index) {
            layer_match_mut(layer).insert(key.to_owned(), string_list_value(&value));
            self.changed();
        }
    }

    fn add_rule_set(&mut self, kind: RuleSetKind) -> usize {
        let tags = self.rule_set_tags();
        let tag = unique_tag("new-rule-set", &tags);
        let rule_set = default_rule_set(&tag, kind);
        let rule_sets = self.rule_sets_mut();
        rule_sets.push(rule_set);
        let index = rule_sets.len() - 1;
        self.changed();
        index
    }

    fn remove_rule_set(&mut self, index: usize) {
        let rule_sets = self.rule_sets_mut();
        if index < rule_sets.len() {
            rule_sets.remove(index);
            self.changed();
        }
    }

    fn set_rule_set_string(&mut self, index: usize, key: &str, value: String) {
        if let Some(rule_set) = self.rule_set_mut(index) {
            rule_set.insert(key.to_owned(), Value::String(value));
            self.changed();
        }
    }

    fn set_rule_set_type(&mut self, index: usize, kind: RuleSetKind) {
        if let Some(rule_set) = self.rule_set_mut(index) {
            rule_set.insert(
                "type".to_owned(),
                Value::String(kind.wire_name().to_owned()),
            );
            normalize_rule_set(rule_set, kind);
            self.changed();
        }
    }

    fn set_rule_set_source(&mut self, index: usize, value: String) {
        if let Some(rule_set) = self.rule_set_mut(index) {
            let kind = RuleSetKind::from_value(object_string(rule_set, "type"));
            let key = match kind {
                RuleSetKind::Remote => "url",
                RuleSetKind::Local => "path",
            };
            rule_set.insert(key.to_owned(), Value::String(value));
            self.changed();
        }
    }

    fn set_rule_set_number(&mut self, index: usize, key: &str, value: String) {
        if let Some(value) = parse_u64(&value) {
            if let Some(rule_set) = self.rule_set_mut(index) {
                rule_set.insert(key.to_owned(), json!(value));
                self.changed();
            }
        }
    }

    fn add_plugin(&mut self) -> usize {
        let tags = self.plugin_tags();
        let tag = unique_tag("cloudflare-preferred", &tags);
        let plugins = self.plugins_mut();
        plugins.push(default_plugin(&tag));
        let index = plugins.len() - 1;
        self.changed();
        index
    }

    fn remove_plugin(&mut self, index: usize) {
        let plugins = self.plugins_mut();
        if index < plugins.len() {
            plugins.remove(index);
            self.changed();
        }
    }

    fn set_plugin_string(&mut self, index: usize, key: &str, value: String) {
        if let Some(plugin) = self.plugin_mut(index) {
            plugin.insert(key.to_owned(), Value::String(value));
            self.changed();
        }
    }

    fn set_plugin_number(&mut self, index: usize, key: &str, value: String) {
        if let Some(value) = parse_u64(&value) {
            if let Some(plugin) = self.plugin_mut(index) {
                plugin.insert(key.to_owned(), json!(value));
                self.changed();
            }
        }
    }

    fn set_plugin_preferred(&mut self, index: usize, key: &str, value: String) {
        if let Some(plugin) = self.plugin_mut(index) {
            let preferred = plugin_object_mut(plugin, "preferred");
            if value.trim().is_empty() {
                preferred.remove(key);
            } else {
                preferred.insert(key.to_owned(), Value::String(value));
            }
            self.changed();
        }
    }

    fn set_optimizer_bool(&mut self, index: usize, key: &str, value: bool) {
        if let Some(plugin) = self.plugin_mut(index) {
            plugin_object_mut(plugin, "optimizer").insert(key.to_owned(), Value::Bool(value));
            self.changed();
        }
    }

    fn set_optimizer_field(&mut self, index: usize, key: &str, value: String) {
        if let Some(plugin) = self.plugin_mut(index) {
            let optimizer = plugin_object_mut(plugin, "optimizer");
            if matches!(key, "test_host" | "test_path") {
                optimizer.insert(key.to_owned(), Value::String(value));
            } else if let Some(value) = parse_u64(&value) {
                optimizer.insert(key.to_owned(), json!(value));
            } else {
                return;
            }
            self.changed();
        }
    }

    fn set_optimizer_list(&mut self, index: usize, key: &str, value: String) {
        if let Some(plugin) = self.plugin_mut(index) {
            plugin_object_mut(plugin, "optimizer")
                .insert(key.to_owned(), string_list_value(&value));
            self.changed();
        }
    }

    fn root_mut(&mut self) -> &mut Map<String, Value> {
        self.value
            .as_object_mut()
            .expect("EdgeSteer configuration root is always an object")
    }

    fn listener_mut(&mut self) -> &mut Map<String, Value> {
        object_mut(self.root_mut(), "listener")
    }

    fn cloudflare_mut(&mut self) -> &mut Map<String, Value> {
        object_mut(self.root_mut(), "cloudflare")
    }

    fn layers_mut(&mut self) -> &mut Vec<Value> {
        array_mut(self.root_mut(), "layers")
    }

    fn rule_sets_mut(&mut self) -> &mut Vec<Value> {
        array_mut(self.root_mut(), "rule_sets")
    }

    fn plugins_mut(&mut self) -> &mut Vec<Value> {
        array_mut(self.root_mut(), "plugins")
    }

    fn layer_mut(&mut self, index: usize) -> Option<&mut Map<String, Value>> {
        self.layers_mut().get_mut(index)?.as_object_mut()
    }

    fn rule_set_mut(&mut self, index: usize) -> Option<&mut Map<String, Value>> {
        self.rule_sets_mut().get_mut(index)?.as_object_mut()
    }

    fn plugin_mut(&mut self, index: usize) -> Option<&mut Map<String, Value>> {
        self.plugins_mut().get_mut(index)?.as_object_mut()
    }

    fn layer_tags(&self) -> Vec<String> {
        self.layers()
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|layer| object_string(layer, "tag"))
            .map(ToOwned::to_owned)
            .collect()
    }

    fn rule_set_tags(&self) -> Vec<String> {
        self.rule_sets()
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|rule_set| object_string(rule_set, "tag"))
            .map(ToOwned::to_owned)
            .collect()
    }

    fn plugin_tags(&self) -> Vec<String> {
        self.plugins()
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|plugin| object_string(plugin, "tag"))
            .map(ToOwned::to_owned)
            .collect()
    }

    fn changed(&mut self) {
        self.serialized = serde_json::to_string_pretty(&self.value)
            .expect("JSON configuration values are serializable");
        self.dirty = true;
        self.revalidate();
    }

    fn revalidate(&mut self) {
        self.serialized = serde_json::to_string_pretty(&self.value)
            .expect("JSON configuration values are serializable");
        match config::parse_config_text(&self.serialized) {
            Ok(config) => {
                self.validated = Some(config);
                self.validation_error = None;
            }
            Err(error) => {
                self.validated = None;
                self.validation_error = Some(format!("{error:#}"));
            }
        }
    }
}

fn object_mut<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    parent
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("UI object field remains an object")
}

fn array_mut<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    parent
        .entry(key.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("UI array field remains an array")
}

fn object_string<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn object_number(object: &Map<String, Value>, key: &str, default: u64) -> u64 {
    object.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn object_bool(object: &Map<String, Value>, key: &str, default: bool) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn object_list(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn parse_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn string_list_value(value: &str) -> Value {
    let mut seen = BTreeSet::new();
    Value::Array(
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .filter(|value| seen.insert(value.to_ascii_lowercase()))
            .map(|value| Value::String(value.to_owned()))
            .collect(),
    )
}

fn unique_tag(prefix: &str, existing: &[String]) -> String {
    let existing: BTreeSet<_> = existing.iter().map(String::as_str).collect();
    (1_u32..)
        .map(|index| format!("{prefix}-{index}"))
        .find(|candidate| !existing.contains(candidate.as_str()))
        .expect("unbounded unique tag search")
}

fn default_layer(tag: &str, kind: LayerKind) -> Value {
    let mut layer = Map::from_iter([
        ("tag".to_owned(), Value::String(tag.to_owned())),
        (
            "type".to_owned(),
            Value::String(kind.wire_name().to_owned()),
        ),
    ]);
    normalize_layer(&mut layer, kind);
    Value::Object(layer)
}

fn normalize_layer(layer: &mut Map<String, Value>, kind: LayerKind) {
    match kind {
        LayerKind::Udp | LayerKind::Tcp => {
            layer.remove("refresh_secs");
            layer.remove("url");
            layer.remove("server_name");
            layer.remove("plugin");
            layer
                .entry("address".to_owned())
                .or_insert_with(|| Value::String("1.1.1.1:53".to_owned()));
            layer
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(3000));
        }
        LayerKind::Doh => {
            layer.remove("refresh_secs");
            layer.remove("server_name");
            layer.remove("plugin");
            layer
                .entry("address".to_owned())
                .or_insert_with(|| Value::String("1.1.1.1:443".to_owned()));
            layer.entry("url".to_owned()).or_insert_with(|| {
                Value::String("https://cloudflare-dns.com/dns-query".to_owned())
            });
            layer
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(3000));
        }
        LayerKind::Dot => {
            layer.remove("refresh_secs");
            layer.remove("url");
            layer.remove("plugin");
            layer
                .entry("address".to_owned())
                .or_insert_with(|| Value::String("1.1.1.1:853".to_owned()));
            layer
                .entry("server_name".to_owned())
                .or_insert_with(|| Value::String("cloudflare-dns.com".to_owned()));
            layer
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(3000));
        }
        LayerKind::Local => {
            layer.remove("address");
            layer.remove("url");
            layer.remove("server_name");
            layer.remove("plugin");
            layer
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(1800));
            layer
                .entry("refresh_secs".to_owned())
                .or_insert_with(|| json!(30));
        }
        LayerKind::Interceptor => {
            layer.remove("address");
            layer.remove("timeout_ms");
            layer.remove("refresh_secs");
            layer.remove("url");
            layer.remove("server_name");
            layer
                .entry("plugin".to_owned())
                .or_insert_with(|| Value::String(String::new()));
            layer
                .entry("fallback".to_owned())
                .or_insert_with(|| Value::String(String::new()));
        }
    }
}

fn layer_match_mut(layer: &mut Map<String, Value>) -> &mut Map<String, Value> {
    object_mut(layer, "match")
}

fn default_rule_set(tag: &str, kind: RuleSetKind) -> Value {
    let mut rule_set = Map::from_iter([
        ("tag".to_owned(), Value::String(tag.to_owned())),
        (
            "type".to_owned(),
            Value::String(kind.wire_name().to_owned()),
        ),
    ]);
    normalize_rule_set(&mut rule_set, kind);
    Value::Object(rule_set)
}

fn normalize_rule_set(rule_set: &mut Map<String, Value>, kind: RuleSetKind) {
    match kind {
        RuleSetKind::Remote => {
            rule_set.remove("path");
            rule_set
                .entry("url".to_owned())
                .or_insert_with(|| Value::String("https://example.com/rules.srs".to_owned()));
            rule_set
                .entry("update_interval_secs".to_owned())
                .or_insert_with(|| json!(86400));
            rule_set
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(10000));
        }
        RuleSetKind::Local => {
            rule_set.remove("url");
            rule_set.remove("timeout_ms");
            rule_set
                .entry("path".to_owned())
                .or_insert_with(|| Value::String("/path/to/rules.srs".to_owned()));
            rule_set
                .entry("update_interval_secs".to_owned())
                .or_insert_with(|| json!(60));
        }
    }
}

fn default_plugin(tag: &str) -> Value {
    json!({
        "tag": tag,
        "type": "cloudflare_preferred",
        "rewrite_ttl_secs": 60,
        "preferred": {},
        "optimizer": {
            "enabled": false,
            "interval_secs": 21600,
            "test_host": "www.cloudflare.com",
            "test_path": "/cdn-cgi/trace",
            "test_port": 443,
            "timeout_ms": 3000,
            "concurrency": 32,
            "samples_per_cidr": 40,
            "probes_per_candidate": 3,
            "compatibility_hosts": [],
            "excluded_candidates": [],
            "max_candidates": 640,
            "candidates": []
        }
    })
}

fn plugin_object_mut<'a>(
    plugin: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    object_mut(plugin, key)
}

impl EdgeSteerUi {
    fn view_overview(&self) -> Element<Message> {
        let listener_ready = self
            .integration
            .as_ref()
            .is_some_and(|status| status.listener_ready);
        let (listener_state, listener_color) = match self.integration.as_ref() {
            Some(status) if status.listener_ready => ("Accepting TCP DNS", success_color()),
            Some(_) => ("Listener unavailable", danger_color()),
            None => ("Checking listener", warning_color()),
        };
        let startup = self
            .integration
            .as_ref()
            .map(|status| status.startup_service.description())
            .unwrap_or_else(|| "Checking service".to_owned());
        let entry = self
            .document
            .value
            .as_object()
            .and_then(|root| object_string(root, "entry"))
            .unwrap_or("not configured");

        let runtime = container(
            column![
                row![
                    column![
                        text("DNS listener")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                        text(self.document.listener_address().to_string()).size(24),
                    ]
                    .spacing(5)
                    .width(Length::Fill),
                    column![
                        text("Runtime state")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                        text(listener_state)
                            .size(15)
                            .style(theme::Text::Color(listener_color)),
                    ]
                    .spacing(5),
                ]
                .align_items(iced::Alignment::End),
                row![
                    overview_metric("Entry layer", entry),
                    overview_metric("Startup service", startup),
                    overview_metric(
                        "Configuration",
                        if self.document.is_valid() {
                            if self.document.is_dirty() {
                                "Valid draft"
                            } else {
                                "Saved"
                            }
                        } else {
                            "Invalid"
                        },
                    ),
                ]
                .spacing(24),
            ]
            .spacing(18),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel_style);

        let source = container(
            column![
                text("Configuration file").size(18),
                text("The service and this control plane use the same fixed strict JSON document.")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
                row![
                    text(&self.document.config_path).width(Length::Fill),
                    button(text("Load"))
                        .padding([9, 12])
                        .style(theme::Button::Secondary)
                        .on_press(Message::LoadConfig),
                ]
                .spacing(8),
                detail("Schema status", self.document.validation_summary()),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel_style);

        let routing = container(
            column![
                row![
                    column![
                        text("Resolver graph").size(18),
                        text("Manage fallback layers, domain rules, and Cloudflare IP rewriting.")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(5)
                    .width(Length::Fill),
                    button(text("Refresh runtime"))
                        .padding([9, 12])
                        .style(theme::Button::Secondary)
                        .on_press(Message::RefreshIntegration),
                ]
                .align_items(iced::Alignment::Center),
                row![
                    overview_metric("Layers", self.document.layers().len()),
                    overview_metric("Rule sets", self.document.rule_sets().len()),
                    overview_metric("CF plugins", self.document.plugins().len()),
                ]
                .spacing(24),
            ]
            .spacing(14),
        )
        .padding(18)
        .width(Length::Fill)
        .style(recessed_panel_style);

        let content = column![
            row![
                column![
                    text("Overview").size(27),
                    text("Resolver health, configuration state, and host integration at a glance.")
                        .size(14)
                        .style(theme::Text::Color(muted_text())),
                ]
                .spacing(5)
                .width(Length::Fill),
                text(if listener_ready { "Online" } else { "Offline" })
                    .size(13)
                    .style(theme::Text::Color(if listener_ready {
                        success_color()
                    } else {
                        warning_color()
                    })),
            ]
            .align_items(iced::Alignment::Center),
            runtime,
            row![source, routing].spacing(16),
        ]
        .spacing(16)
        .max_width(1020);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_general_configuration(&self) -> Element<Message> {
        let root = self.document.value.as_object();
        let listener = root
            .and_then(|root| root.get("listener"))
            .and_then(Value::as_object);
        let cloudflare = root
            .and_then(|root| root.get("cloudflare"))
            .and_then(Value::as_object);
        let listener_address = listener
            .and_then(|listener| object_string(listener, "address"))
            .unwrap_or("127.0.0.1:53");
        let allow_remote =
            listener.is_some_and(|listener| object_bool(listener, "allow_remote", false));
        let request_timeout = root
            .map(|root| object_number(root, "request_timeout_ms", 8000))
            .unwrap_or(8000)
            .to_string();
        let range_refresh = cloudflare
            .map(|cloudflare| object_number(cloudflare, "range_refresh_secs", 86400))
            .unwrap_or(86400)
            .to_string();
        let entry = root
            .and_then(|root| object_string(root, "entry"))
            .unwrap_or_default();
        let entry_options: Vec<String> = self
            .document
            .layers()
            .iter()
            .filter_map(|layer| layer.as_object())
            .filter_map(|layer| object_string(layer, "tag"))
            .map(ToOwned::to_owned)
            .collect();
        let selected_entry = entry_options
            .iter()
            .find(|tag| tag.as_str() == entry)
            .cloned();

        container(column![
            text("Listener").size(18),
            text("The local address that accepts UDP and TCP DNS requests.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            labeled_input("Address", listener_address, Message::ListenerAddressChanged),
            checkbox(self.language.text("Allow remote resolver clients"), allow_remote)
                .on_toggle(Message::AllowRemoteChanged),
            text("Routing").size(18),
            text("The entry layer starts the configured fallback graph.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            column![
                text("Default entry layer")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
                pick_list(entry_options, selected_entry, Message::EntryChanged)
                    .placeholder(self.language.text("Select a layer"))
                    .width(Length::Fill),
            ]
            .spacing(6),
            labeled_input(
                "Whole request timeout (ms)",
                &request_timeout,
                Message::RequestTimeoutChanged
            ),
            text("Cloudflare range data").size(18),
            text("Official Cloudflare network ranges used to validate rewriting candidates.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            labeled_input(
                "Refresh interval (seconds)",
                &range_refresh,
                Message::RangeRefreshChanged
            ),
            text("Listener address and remote access changes require a service restart after saving.")
                .size(13)
                .style(theme::Text::Color(warning_color())),
        ]
        .spacing(12))
        .padding(18)
        .max_width(760)
        .style(panel_style)
        .into()
    }

    fn view_layers_configuration(&self) -> Element<Message> {
        let list = self.document.layers().iter().enumerate().fold(
            column![
                row![
                    column![
                        text("Resolver layers").size(18),
                        text("Order determines matching priority.")
                            .size(12)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    text(self.document.layers().len().to_string())
                        .size(14)
                        .style(theme::Text::Color(accent_color())),
                ]
                .align_items(iced::Alignment::Center),
            ]
            .spacing(6),
            |list, (index, layer)| {
                let layer = layer.as_object();
                let tag = layer
                    .and_then(|layer| object_string(layer, "tag"))
                    .unwrap_or("invalid-layer");
                let kind =
                    LayerKind::from_value(layer.and_then(|layer| object_string(layer, "type")));
                let selected = self.selected_layer == Some(index);
                list.push(
                    button(
                        row![
                            text(format!("{:02}", index + 1))
                                .size(12)
                                .style(theme::Text::Color(quiet_text())),
                            column![
                                text(tag).size(14),
                                text(kind.label(self.language))
                                    .size(12)
                                    .style(theme::Text::Color(muted_text())),
                            ]
                            .spacing(2),
                        ]
                        .spacing(10)
                        .align_items(iced::Alignment::Center),
                    )
                    .padding([9, 10])
                    .style(if selected {
                        theme::Button::Primary
                    } else {
                        theme::Button::Text
                    })
                    .width(Length::Fill)
                    .on_press(Message::SelectLayer(index)),
                )
            },
        );
        let add_layer = row![
            text("Add layer")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            pick_list(LayerKind::ALL, None::<LayerKind>, Message::AddLayer)
                .placeholder(self.language.text("Layer type"))
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_items(iced::Alignment::Center);
        let list = container(list.push(add_layer).spacing(8))
            .padding(14)
            .width(Length::Fixed(292.0))
            .style(recessed_panel_style);
        let editor = self.view_layer_editor();
        row![
            list,
            container(editor)
                .padding(18)
                .width(Length::Fill)
                .style(panel_style)
        ]
        .spacing(16)
        .into()
    }

    fn view_layer_editor(&self) -> Element<Message> {
        let Some(index) = self
            .selected_layer
            .filter(|index| *index < self.document.layers().len())
        else {
            return text("Select a resolver layer to edit it.").into();
        };
        let Some(layer) = self.document.layer(index) else {
            return text("The selected resolver layer is unavailable.").into();
        };
        let kind = LayerKind::from_value(object_string(layer, "type"));
        let tag = object_string(layer, "tag").unwrap_or_default();
        let fallback = object_string(layer, "fallback").unwrap_or_default();
        let plugin = object_string(layer, "plugin").unwrap_or_default();
        let address = object_string(layer, "address").unwrap_or_default();
        let url = object_string(layer, "url").unwrap_or_default();
        let server_name = object_string(layer, "server_name").unwrap_or_default();
        let timeout = object_number(layer, "timeout_ms", 3000).to_string();
        let refresh = object_number(layer, "refresh_secs", 30).to_string();
        let matcher = layer.get("match").and_then(Value::as_object);
        let match_mode =
            MatchMode::from_value(matcher.and_then(|matcher| object_string(matcher, "mode")));
        let keywords = matcher
            .map(|matcher| object_list(matcher, "keywords"))
            .unwrap_or_default();
        let rule_sets = matcher
            .map(|matcher| object_list(matcher, "rule_sets"))
            .unwrap_or_default();

        let mut controls = column![
            text(format!("{} {:02}", self.language.text("Layer"), index + 1))
                .size(12)
                .style(theme::Text::Color(quiet_text())),
            text(tag).size(20),
            text("Resolver behavior, fallback, and domain targeting for this layer.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            labeled_input("Tag", tag, move |value| Message::LayerTagChanged(
                index, value
            )),
            column![
                text("Layer type")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
                pick_list(LayerKind::ALL, Some(kind), move |value| {
                    Message::LayerTypeChanged(index, value)
                })
                .width(Length::Fill),
            ]
            .spacing(6),
            labeled_input("Fallback layer tag", fallback, move |value| {
                Message::LayerFallbackChanged(index, value)
            }),
        ]
        .spacing(10);

        match kind {
            LayerKind::Udp | LayerKind::Tcp => {
                controls = controls
                    .push(labeled_input("Address", address, move |value| {
                        Message::LayerAddressChanged(index, value)
                    }))
                    .push(labeled_input("Timeout (ms)", &timeout, move |value| {
                        Message::LayerTimeoutChanged(index, value)
                    }));
            }
            LayerKind::Doh => {
                controls = controls
                    .push(labeled_input("Bootstrap address", address, move |value| {
                        Message::LayerAddressChanged(index, value)
                    }))
                    .push(labeled_input("HTTPS URL", url, move |value| {
                        Message::LayerUrlChanged(index, value)
                    }))
                    .push(labeled_input("Timeout (ms)", &timeout, move |value| {
                        Message::LayerTimeoutChanged(index, value)
                    }));
            }
            LayerKind::Dot => {
                controls = controls
                    .push(labeled_input("Address", address, move |value| {
                        Message::LayerAddressChanged(index, value)
                    }))
                    .push(labeled_input(
                        "TLS server name",
                        server_name,
                        move |value| Message::LayerServerNameChanged(index, value),
                    ))
                    .push(labeled_input("Timeout (ms)", &timeout, move |value| {
                        Message::LayerTimeoutChanged(index, value)
                    }));
            }
            LayerKind::Local => {
                controls = controls
                    .push(labeled_input("Timeout (ms)", &timeout, move |value| {
                        Message::LayerTimeoutChanged(index, value)
                    }))
                    .push(labeled_input(
                        "System DNS refresh (seconds)",
                        &refresh,
                        move |value| Message::LayerRefreshChanged(index, value),
                    ));
            }
            LayerKind::Interceptor => {
                controls = controls.push(labeled_input("Plugin tag", plugin, move |value| {
                    Message::LayerPluginChanged(index, value)
                }));
            }
        }

        let up = if index > 0 {
            button(text("Move up"))
                .style(theme::Button::Secondary)
                .on_press(Message::MoveLayerUp(index))
        } else {
            button(text("Move up")).style(theme::Button::Secondary)
        };
        let down = if index + 1 < self.document.layers().len() {
            button(text("Move down"))
                .style(theme::Button::Secondary)
                .on_press(Message::MoveLayerDown(index))
        } else {
            button(text("Move down")).style(theme::Button::Secondary)
        };
        controls = controls
            .push(text("Domain match").size(18))
            .push(
                text("Use keywords or loaded SRS rule sets to select this layer before the default entry.")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
            )
            .push(
                column![
                    text("Match mode")
                        .size(13)
                        .style(theme::Text::Color(muted_text())),
                    pick_list(MatchMode::ALL, Some(match_mode), move |value| {
                        Message::LayerMatchModeChanged(index, value)
                    })
                    .width(Length::Fill),
                ]
                .spacing(6),
            )
            .push(labeled_input(
                "Keywords (comma separated)",
                &keywords,
                move |value| Message::LayerKeywordsChanged(index, value),
            ))
            .push(labeled_input(
                "Rule set tags (comma separated)",
                &rule_sets,
                move |value| Message::LayerRuleSetsChanged(index, value),
            ))
            .push(
                row![
                    up,
                    down,
                    button(text("Remove layer"))
                        .style(theme::Button::Destructive)
                        .on_press(Message::RemoveLayer(index))
                ]
                .spacing(8),
            );

        controls.spacing(12).max_width(740).into()
    }

    fn view_rule_sets_configuration(&self) -> Element<Message> {
        let list = self.document.rule_sets().iter().enumerate().fold(
            column![
                row![
                    column![
                        text("Rule sets").size(18),
                        text("Domain classifications loaded from SRS sources.")
                            .size(12)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    text(self.document.rule_sets().len().to_string())
                        .size(14)
                        .style(theme::Text::Color(accent_color())),
                ]
                .align_items(iced::Alignment::Center),
            ]
            .spacing(6),
            |list, (index, rule_set)| {
                let rule_set = rule_set.as_object();
                let tag = rule_set
                    .and_then(|rule_set| object_string(rule_set, "tag"))
                    .unwrap_or("invalid-rule-set");
                let kind = RuleSetKind::from_value(
                    rule_set.and_then(|rule_set| object_string(rule_set, "type")),
                );
                let selected = self.selected_rule_set == Some(index);
                list.push(
                    button(
                        row![
                            text(format!("{:02}", index + 1))
                                .size(12)
                                .style(theme::Text::Color(quiet_text())),
                            column![
                                text(tag).size(14),
                                text(kind.label(self.language))
                                    .size(12)
                                    .style(theme::Text::Color(muted_text())),
                            ]
                            .spacing(2),
                        ]
                        .spacing(10)
                        .align_items(iced::Alignment::Center),
                    )
                    .padding([9, 10])
                    .style(if selected {
                        theme::Button::Primary
                    } else {
                        theme::Button::Text
                    })
                    .width(Length::Fill)
                    .on_press(Message::SelectRuleSet(index)),
                )
            },
        );
        let add_rule_set = row![
            text("Add rule set")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            pick_list(RuleSetKind::ALL, None::<RuleSetKind>, Message::AddRuleSet)
                .placeholder(self.language.text("Source type"))
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_items(iced::Alignment::Center);
        let list = container(list.push(add_rule_set).spacing(8))
            .padding(14)
            .width(Length::Fixed(292.0))
            .style(recessed_panel_style);
        row![
            list,
            container(self.view_rule_set_editor())
                .padding(18)
                .width(Length::Fill)
                .style(panel_style)
        ]
        .spacing(16)
        .into()
    }

    fn view_rule_set_editor(&self) -> Element<Message> {
        let Some(index) = self
            .selected_rule_set
            .filter(|index| *index < self.document.rule_sets().len())
        else {
            return text("Select a rule set to edit it.").into();
        };
        let Some(rule_set) = self.document.rule_set(index) else {
            return text("The selected rule set is unavailable.").into();
        };
        let kind = RuleSetKind::from_value(object_string(rule_set, "type"));
        let tag = object_string(rule_set, "tag").unwrap_or_default();
        let source = match kind {
            RuleSetKind::Remote => object_string(rule_set, "url").unwrap_or_default(),
            RuleSetKind::Local => object_string(rule_set, "path").unwrap_or_default(),
        };
        let interval = object_number(
            rule_set,
            "update_interval_secs",
            if kind == RuleSetKind::Remote {
                86400
            } else {
                60
            },
        )
        .to_string();
        let timeout = object_number(rule_set, "timeout_ms", 10000).to_string();

        let mut controls = column![
            text(format!(
                "{} {:02}",
                self.language.text("Rule set"),
                index + 1
            ))
            .size(12)
            .style(theme::Text::Color(quiet_text())),
            text(tag).size(20),
            text("A remote or local SRS source used by resolver-layer domain matching.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            labeled_input("Tag", tag, move |value| Message::RuleSetTagChanged(
                index, value
            )),
            column![
                text("Source type")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
                pick_list(RuleSetKind::ALL, Some(kind), move |value| {
                    Message::RuleSetTypeChanged(index, value)
                })
                .width(Length::Fill),
            ]
            .spacing(6),
            labeled_input(
                if kind == RuleSetKind::Remote {
                    "HTTPS URL"
                } else {
                    "Local SRS path"
                },
                source,
                move |value| Message::RuleSetSourceChanged(index, value)
            ),
            labeled_input("Refresh interval (seconds)", &interval, move |value| {
                Message::RuleSetIntervalChanged(index, value)
            }),
        ]
        .spacing(12);
        if kind == RuleSetKind::Remote {
            controls = controls.push(labeled_input(
                "Download timeout (ms)",
                &timeout,
                move |value| Message::RuleSetTimeoutChanged(index, value),
            ));
        }
        controls
            .push(
                button(text("Remove rule set"))
                    .style(theme::Button::Destructive)
                    .on_press(Message::RemoveRuleSet(index)),
            )
            .max_width(740)
            .into()
    }

    fn view_cloudflare_configuration(&self) -> Element<Message> {
        let list = self.document.plugins().iter().enumerate().fold(
            column![
                row![
                    column![
                        text("Cloudflare plugins").size(18),
                        text("Validated response rewriting and scheduled edge probing.")
                            .size(12)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    text(self.document.plugins().len().to_string())
                        .size(14)
                        .style(theme::Text::Color(accent_color())),
                ]
                .align_items(iced::Alignment::Center),
            ]
            .spacing(6),
            |list, (index, plugin)| {
                let plugin = plugin.as_object();
                let tag = plugin
                    .and_then(|plugin| object_string(plugin, "tag"))
                    .unwrap_or("invalid-plugin");
                let selected = self.selected_plugin == Some(index);
                list.push(
                    button(
                        row![
                            text(format!("{:02}", index + 1))
                                .size(12)
                                .style(theme::Text::Color(quiet_text())),
                            column![
                                text(tag).size(14),
                                text("Cloudflare preferred")
                                    .size(12)
                                    .style(theme::Text::Color(muted_text())),
                            ]
                            .spacing(2),
                        ]
                        .spacing(10)
                        .align_items(iced::Alignment::Center),
                    )
                    .padding([9, 10])
                    .style(if selected {
                        theme::Button::Primary
                    } else {
                        theme::Button::Text
                    })
                    .width(Length::Fill)
                    .on_press(Message::SelectPlugin(index)),
                )
            },
        );
        let list = container(
            list.push(
                button(text("Add Cloudflare preferred plugin"))
                    .padding([9, 10])
                    .style(theme::Button::Secondary)
                    .on_press(Message::AddPlugin),
            )
            .spacing(8),
        )
        .padding(14)
        .width(Length::Fixed(292.0))
        .style(recessed_panel_style);
        row![
            list,
            container(self.view_plugin_editor())
                .padding(18)
                .width(Length::Fill)
                .style(panel_style)
        ]
        .spacing(16)
        .into()
    }

    fn view_plugin_editor(&self) -> Element<Message> {
        let Some(index) = self
            .selected_plugin
            .filter(|index| *index < self.document.plugins().len())
        else {
            return text("Select a Cloudflare preferred plugin to edit it.").into();
        };
        let Some(plugin) = self.document.plugin(index) else {
            return text("The selected plugin is unavailable.").into();
        };
        let tag = object_string(plugin, "tag").unwrap_or_default();
        let ttl = object_number(plugin, "rewrite_ttl_secs", 60).to_string();
        let preferred = plugin.get("preferred").and_then(Value::as_object);
        let ipv4 = preferred
            .and_then(|preferred| object_string(preferred, "ipv4"))
            .unwrap_or_default();
        let ipv6 = preferred
            .and_then(|preferred| object_string(preferred, "ipv6"))
            .unwrap_or_default();
        let optimizer = plugin.get("optimizer").and_then(Value::as_object);
        let optimizer_enabled =
            optimizer.is_some_and(|optimizer| object_bool(optimizer, "enabled", false));
        let interval = optimizer
            .map(|optimizer| object_number(optimizer, "interval_secs", 21600))
            .unwrap_or(21600)
            .to_string();
        let test_host = optimizer
            .and_then(|optimizer| object_string(optimizer, "test_host"))
            .unwrap_or("www.cloudflare.com");
        let test_path = optimizer
            .and_then(|optimizer| object_string(optimizer, "test_path"))
            .unwrap_or("/cdn-cgi/trace");
        let test_port = optimizer
            .map(|optimizer| object_number(optimizer, "test_port", 443))
            .unwrap_or(443)
            .to_string();
        let timeout = optimizer
            .map(|optimizer| object_number(optimizer, "timeout_ms", 3000))
            .unwrap_or(3000)
            .to_string();
        let concurrency = optimizer
            .map(|optimizer| object_number(optimizer, "concurrency", 32))
            .unwrap_or(32)
            .to_string();
        let samples = optimizer
            .map(|optimizer| object_number(optimizer, "samples_per_cidr", 1))
            .unwrap_or(1)
            .to_string();
        let probes = optimizer
            .map(|optimizer| object_number(optimizer, "probes_per_candidate", 3))
            .unwrap_or(3)
            .to_string();
        let max_candidates = optimizer
            .map(|optimizer| object_number(optimizer, "max_candidates", 128))
            .unwrap_or(128)
            .to_string();
        let candidates = optimizer
            .map(|optimizer| object_list(optimizer, "candidates"))
            .unwrap_or_default();
        let compatibility_hosts = optimizer
            .map(|optimizer| object_list(optimizer, "compatibility_hosts"))
            .unwrap_or_default();
        let excluded_candidates = optimizer
            .map(|optimizer| object_list(optimizer, "excluded_candidates"))
            .unwrap_or_default();

        column![
            text(format!("{} {:02}", self.language.text("Plugin"), index + 1))
                .size(12)
                .style(theme::Text::Color(quiet_text())),
            text(tag).size(20),
            text(
                "Rewrites confirmed Cloudflare responses with a compatible preferred edge address."
            )
            .size(13)
            .style(theme::Text::Color(muted_text())),
            text("Response rewrite").size(18),
            labeled_input("Tag", tag, move |value| Message::PluginTagChanged(
                index, value
            )),
            labeled_input("Rewrite TTL (seconds)", &ttl, move |value| {
                Message::PluginTtlChanged(index, value)
            }),
            labeled_input("Preferred IPv4 (optional)", ipv4, move |value| {
                Message::PluginIpv4Changed(index, value)
            }),
            labeled_input("Preferred IPv6 (optional)", ipv6, move |value| {
                Message::PluginIpv6Changed(index, value)
            }),
            text("Edge optimizer").size(18),
            text("Probe Cloudflare candidates on a schedule and retain stable compatible results.")
                .size(13)
                .style(theme::Text::Color(muted_text())),
            checkbox(
                self.language.text("Enable scheduled probe"),
                optimizer_enabled
            )
            .on_toggle(move |value| Message::OptimizerEnabledChanged(index, value)),
            labeled_input("Probe interval (seconds)", &interval, move |value| {
                Message::OptimizerFieldChanged(index, "interval_secs", value)
            }),
            labeled_input("Probe host", test_host, move |value| {
                Message::OptimizerFieldChanged(index, "test_host", value)
            }),
            labeled_input("Probe path", test_path, move |value| {
                Message::OptimizerFieldChanged(index, "test_path", value)
            }),
            labeled_input("Probe port", &test_port, move |value| {
                Message::OptimizerFieldChanged(index, "test_port", value)
            }),
            labeled_input("Probe timeout (ms)", &timeout, move |value| {
                Message::OptimizerFieldChanged(index, "timeout_ms", value)
            }),
            labeled_input("Concurrency", &concurrency, move |value| {
                Message::OptimizerFieldChanged(index, "concurrency", value)
            }),
            labeled_input("Samples per CIDR", &samples, move |value| {
                Message::OptimizerFieldChanged(index, "samples_per_cidr", value)
            }),
            labeled_input("Probes per candidate", &probes, move |value| {
                Message::OptimizerFieldChanged(index, "probes_per_candidate", value)
            }),
            labeled_input("Maximum candidates", &max_candidates, move |value| {
                Message::OptimizerFieldChanged(index, "max_candidates", value)
            }),
            labeled_input(
                "Candidate IPs/CIDRs (comma separated)",
                &candidates,
                move |value| Message::OptimizerListChanged(index, "candidates", value)
            ),
            labeled_input(
                "Compatibility hosts (comma separated)",
                &compatibility_hosts,
                move |value| Message::OptimizerListChanged(index, "compatibility_hosts", value)
            ),
            labeled_input(
                "Excluded candidate IPs/CIDRs (comma separated)",
                &excluded_candidates,
                move |value| Message::OptimizerListChanged(index, "excluded_candidates", value)
            ),
            button(text("Remove plugin"))
                .style(theme::Button::Destructive)
                .on_press(Message::RemovePlugin(index)),
        ]
        .spacing(12)
        .max_width(740)
        .into()
    }

    fn view_json_preview(&self) -> Element<Message> {
        container(
            column![
                row![
                    column![
                        text("JSON preview").size(18),
                        text("Generated from the current form draft. Saving validates before replacing the active file.")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    text(if self.document.is_valid() {
                        "Schema valid"
                    } else {
                        "Schema invalid"
                    })
                    .size(13)
                    .style(theme::Text::Color(if self.document.is_valid() {
                        success_color()
                    } else {
                        danger_color()
                    })),
                ]
                .align_items(iced::Alignment::Center),
                container(
                    scrollable(
                        text(&self.document.serialized)
                            .size(13)
                            .style(theme::Text::Color(primary_text())),
                    )
                    .height(Length::Fixed(510.0)),
                )
                .padding(12)
                .width(Length::Fill)
                .style(recessed_panel_style),
            ]
            .spacing(14),
        )
        .padding(18)
        .max_width(1020)
        .style(panel_style)
        .into()
    }

    fn view_settings(&self) -> Element<Message> {
        let listener = self.document.listener_address();
        let platform_supported = cfg!(target_os = "macos");
        let listener_uses_system_dns_port = listener.port() == 53 && listener.ip().is_loopback();
        let listener_ready = self
            .integration
            .as_ref()
            .is_some_and(|status| status.listener_ready);
        let engine_running = self.engine_running();
        let system_dns_active = self.integration.as_ref().is_some_and(|status| {
            status
                .dns_services
                .iter()
                .any(|service| service.uses_loopback_dns())
        }) || integration::system_dns_is_managed();
        let system_dns_ready = self.document.is_valid()
            && listener_uses_system_dns_port
            && listener_ready
            && engine_running
            && !self.registration_action_in_progress
            && platform_supported;
        let autostart_ready = self.document.is_valid()
            && self.app_bundle.is_some()
            && !self.registration_action_in_progress
            && platform_supported;
        let restart_ready =
            self.document.is_valid() && engine_running && !self.registration_action_in_progress;
        let start_ready =
            self.document.is_valid() && !engine_running && !self.registration_action_in_progress;
        let action_ready = !self.registration_action_in_progress && platform_supported;
        let legacy_service_present = self.integration.as_ref().is_some_and(|status| {
            matches!(
                &status.startup_service,
                integration::StartupService::LegacyDaemon { .. }
            )
        });
        let startup_state = self
            .integration
            .as_ref()
            .map(|status| status.startup_service.description())
            .unwrap_or_else(|| "Checking service".to_owned());
        let listener_state = if listener_ready {
            "Accepting TCP DNS"
        } else if self.integration.is_some() {
            "Not accepting TCP DNS"
        } else {
            "Checking listener"
        };
        let listener_color = if listener_ready {
            success_color()
        } else if self.integration.is_some() {
            danger_color()
        } else {
            warning_color()
        };

        let startup_action = if matches!(
            self.integration
                .as_ref()
                .map(|status| &status.startup_service),
            Some(integration::StartupService::Registered { .. })
        ) {
            registration_button(RegistrationAction::DisableAutoStart, action_ready)
        } else {
            registration_button(RegistrationAction::EnableAutoStart, autostart_ready)
        };
        let mut startup_actions = row![startup_action].spacing(8);
        if legacy_service_present {
            startup_actions = startup_actions.push(registration_button(
                RegistrationAction::RemoveLegacyService,
                action_ready,
            ));
        }
        let engine_action = if engine_running {
            registration_button(
                RegistrationAction::StopEngine,
                !self.registration_action_in_progress,
            )
        } else {
            registration_button(RegistrationAction::StartEngine, start_ready)
        };
        let runtime_actions = row![
            engine_action,
            registration_button(RegistrationAction::Restart, restart_ready),
            button(text("Refresh"))
                .padding([9, 12])
                .style(theme::Button::Secondary)
                .on_press(Message::RefreshIntegration),
        ]
        .spacing(8);
        let dns_action = if system_dns_active {
            registration_button(RegistrationAction::DisableSystemDns, action_ready)
        } else {
            registration_button(RegistrationAction::EnableSystemDns, system_dns_ready)
        };
        let dns_actions = row![
            dns_action,
            button(text("Refresh"))
                .padding([9, 12])
                .style(theme::Button::Secondary)
                .on_press(Message::RefreshIntegration),
        ]
        .spacing(8);

        let system_dns_requirement = if !platform_supported {
            "System registration controls are currently available on macOS only. The DNS service and configuration UI remain portable."
        } else if !self.document.is_valid() {
            "Fix the configuration validation errors before enabling system DNS."
        } else if !listener_uses_system_dns_port {
            "System DNS requires EdgeSteer to listen on a loopback address at port 53."
        } else if !listener_ready {
            "Keep EdgeSteer open until the configured loopback listener accepts TCP DNS."
        } else if system_dns_active {
            "System DNS is managed by EdgeSteer. It will be restored when you explicitly quit the app."
        } else {
            "The listener is eligible for system DNS."
        };
        let system_dns_requirement_color = if system_dns_ready {
            success_color()
        } else if !platform_supported {
            muted_text()
        } else if !self.document.is_valid() || !listener_uses_system_dns_port {
            danger_color()
        } else {
            warning_color()
        };

        let runtime = container(
            row![
                column![
                    text("Configured listener")
                        .size(13)
                        .style(theme::Text::Color(muted_text())),
                    text(listener.to_string()).size(22),
                ]
                .spacing(5)
                .width(Length::Fill),
                column![
                    text("Listener status")
                        .size(13)
                        .style(theme::Text::Color(muted_text())),
                    text(listener_state)
                        .size(14)
                        .style(theme::Text::Color(listener_color)),
                ]
                .spacing(5),
            ]
            .align_items(iced::Alignment::End),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel_style);

        let startup_service = container(
            column![
                row![
                    column![
                        text("Open at login").size(18),
                        text("The login item opens this App. It never installs a separate command-line DNS service.")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    text(&startup_state)
                        .size(13)
                        .style(theme::Text::Color(if platform_supported {
                            accent_color()
                        } else {
                            muted_text()
                        })),
                ]
                .align_items(iced::Alignment::Center),
                startup_actions,
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel_style);

        let system_dns = container(
            column![
                text("System DNS").size(18),
                text(system_dns_requirement)
                    .size(13)
                    .style(theme::Text::Color(system_dns_requirement_color)),
                dns_actions,
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .style(recessed_panel_style);

        let application = container(
            column![
                text("Application").size(18),
                text("The menu bar is the primary control surface. This window is for configuration and detailed status.")
                    .size(13)
                    .style(theme::Text::Color(muted_text())),
                row![
                    column![
                        text("Language")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                        pick_list(Language::ALL, Some(self.language), Message::SelectLanguage)
                            .width(Length::Fill),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    column![
                        text("Appearance")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                        pick_list(
                            AppearanceMode::ALL,
                            Some(self.appearance),
                            Message::SelectAppearance
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                ]
                .spacing(12),
                checkbox(
                    self.language.text("Close window to menu bar"),
                    self.preferences.close_to_menu_bar
                )
                .on_toggle(Message::CloseToMenuBarChanged),
                text("Closing this window releases the GUI while EdgeSteer keeps running in the menu bar. Use Quit EdgeSteer from the menu bar when you need to stop it and restore managed system DNS.")
                    .size(12)
                    .style(theme::Text::Color(muted_text())),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel_style);

        let mut content = column![
            text("Settings").size(27),
            application,
            runtime,
            container(
                column![
                    text("DNS engine").size(18),
                    text("Restart only after saving listener changes. The resolver is managed by the background EdgeSteer Agent.")
                        .size(13)
                        .style(theme::Text::Color(muted_text())),
                    runtime_actions,
                ]
                .spacing(12),
            )
            .padding(18)
            .width(Length::Fill)
            .style(recessed_panel_style),
            startup_service,
            system_dns,
        ]
        .spacing(16)
        .max_width(1020);

        if let Some(status) = &self.integration {
            if status.dns_services.is_empty() {
                content = content.push(
                    container(
                        column![
                            text("Physical network services").size(18),
                            text("No macOS physical DNS services are available from this platform integration.")
                                .size(13)
                                .style(theme::Text::Color(muted_text())),
                        ]
                        .spacing(5),
                    )
                    .padding(18)
                    .width(Length::Fill)
                    .style(recessed_panel_style),
                );
            } else {
                let services = status.dns_services.iter().fold(
                    column![
                        text("Physical network services").size(18),
                        text("The UI only changes enabled services that are already using automatic DNS or EdgeSteer loopback DNS.")
                            .size(13)
                            .style(theme::Text::Color(muted_text())),
                    ]
                    .spacing(8),
                    |services, service| {
                        let state = if !service.enabled {
                            "disabled".to_owned()
                        } else if service.uses_loopback_dns() {
                            "EdgeSteer loopback DNS".to_owned()
                        } else {
                            service.dns_description()
                        };
                        services.push(container(detail(
                            format!("{} ({})", service.name, service.device),
                            state,
                        ))
                        .padding([8, 0]))
                    },
                );
                content = content.push(
                    container(services)
                        .padding(18)
                        .width(Length::Fill)
                        .style(recessed_panel_style),
                );
            }
        } else {
            content = content.push(
                container(
                    text("Reading service and system DNS status...")
                        .size(13)
                        .style(theme::Text::Color(muted_text())),
                )
                .padding(18)
                .width(Length::Fill)
                .style(recessed_panel_style),
            );
        }
        scrollable(content).height(Length::Fill).into()
    }
}

fn save_button(valid: bool) -> iced::widget::Button<'static, Message> {
    let control = button(text("Save configuration"))
        .padding([9, 12])
        .style(theme::Button::Primary);
    if valid {
        control.on_press(Message::SaveConfig)
    } else {
        control
    }
}

fn registration_button(
    action: RegistrationAction,
    enabled: bool,
) -> iced::widget::Button<'static, Message> {
    let control = button(text(action.label(active_language())))
        .padding([9, 12])
        .style(action.button_style());
    if enabled {
        control.on_press(Message::RequestRegistrationAction(action))
    } else {
        control
    }
}

fn labeled_input(
    label: impl Into<String>,
    value: impl Into<String>,
    on_input: impl Fn(String) -> Message + 'static,
) -> Element<'static, Message> {
    let label = label.into();
    let label = active_language().translate(&label).into_owned();
    let value = value.into();
    column![
        text(label.clone())
            .size(13)
            .style(theme::Text::Color(muted_text())),
        text_input(&label, &value)
            .on_input(on_input)
            .padding([9, 10])
            .width(Length::Fill),
    ]
    .spacing(6)
    .into()
}

fn detail(label: impl Into<String>, value: impl Into<String>) -> Element<'static, Message> {
    row![
        text(label.into())
            .size(13)
            .width(Length::Fixed(220.0))
            .style(theme::Text::Color(muted_text())),
        text(value.into())
            .size(13)
            .style(theme::Text::Color(primary_text())),
    ]
    .spacing(12)
    .align_items(iced::Alignment::Center)
    .into()
}

fn overview_metric(label: impl Into<String>, value: impl ToString) -> Element<'static, Message> {
    column![
        text(label.into())
            .size(12)
            .style(theme::Text::Color(muted_text())),
        text(value.to_string()).size(14),
    ]
    .spacing(4)
    .width(Length::FillPortion(1))
    .into()
}

fn notice_view(notice: &Notice) -> Element<'_, Message> {
    container(
        text(&notice.text)
            .size(13)
            .style(theme::Text::Color(primary_text())),
    )
    .padding([10, 14])
    .width(Length::Fixed(520.0))
    .style(toast_style)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_document() -> ConfigDocument {
        ConfigDocument::from_default("edgesteer.json".to_owned())
    }

    #[test]
    fn bundled_configuration_is_a_valid_form_document() {
        let document = default_document();
        assert!(document.is_valid());
        assert!(!document.is_dirty());
    }

    #[test]
    fn interface_copy_switches_between_chinese_and_english() {
        assert_eq!(Language::Chinese.text("Overview"), "概览");
        assert_eq!(Language::Chinese.text("Dynamic local DNS"), "动态本地 DNS");
        assert_eq!(
            Language::Chinese.text(
                "Closing this window releases the GUI while EdgeSteer keeps running in the menu bar. Use Quit EdgeSteer from the menu bar when you need to stop it and restore managed system DNS."
            ),
            "关闭窗口会释放图形界面；EdgeSteer 会继续在菜单栏运行。需要停止并恢复已接管的系统 DNS 时，请在菜单栏中选择“退出 EdgeSteer”。"
        );
        assert_eq!(Language::English.text("Overview"), "Overview");
    }

    #[test]
    fn appearance_modes_use_separate_monochrome_surfaces() {
        let dark = AppearanceMode::Dark.colors();
        let light = AppearanceMode::Light.colors();

        assert!(dark.background.r < light.background.r);
        assert!(dark.text.r > light.text.r);
        assert!(dark.primary.r > dark.background.r);
        assert!(light.primary.r < light.background.r);
    }

    #[test]
    fn ui_preferences_default_to_menu_bar_continuity() {
        let preferences = UiPreferences::default();

        assert_eq!(preferences.language, Language::Chinese);
        assert_eq!(preferences.appearance, AppearanceMode::Dark);
        assert!(preferences.close_to_menu_bar);
    }

    #[test]
    fn ui_preferences_round_trip_without_touching_dns_configuration() {
        let preferences = UiPreferences {
            language: Language::English,
            appearance: AppearanceMode::Light,
            close_to_menu_bar: false,
        };

        let encoded = serde_json::to_vec(&preferences).expect("serialize preferences");
        let decoded: UiPreferences =
            serde_json::from_slice(&encoded).expect("deserialize preferences");

        assert_eq!(decoded, preferences);
    }

    #[test]
    fn notifications_expire_automatically() {
        let mut notice = Notice::success("Configuration saved");
        assert!(!notice.expired());

        notice.expires_at = Instant::now() - Duration::from_millis(1);
        assert!(notice.expired());
    }

    #[test]
    fn changing_a_network_layer_to_local_removes_network_fields() {
        let mut document = default_document();
        let index = document
            .layers()
            .iter()
            .position(|layer| {
                layer
                    .as_object()
                    .and_then(|layer| object_string(layer, "tag"))
                    == Some("tencent-doh")
            })
            .expect("bundled Tencent resolver layer");

        document.set_layer_type(index, LayerKind::Local);

        let layer = document.layer(index).expect("edited layer");
        assert_eq!(object_string(layer, "type"), Some("local"));
        assert!(!layer.contains_key("address"));
        assert!(!layer.contains_key("url"));
        assert!(document.is_valid());
    }

    #[test]
    fn form_save_writes_the_validated_document_atomically() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("edgesteer.json");
        let mut document = default_document();
        document.config_path = path.display().to_string();

        document.save().expect("save valid configuration");

        assert!(!document.is_dirty());
        assert!(config::load_config(&path).is_ok());
    }
}
