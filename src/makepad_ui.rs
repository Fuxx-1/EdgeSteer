use std::{
    collections::BTreeSet,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
};

use makepad_widgets::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    agent::{AgentClient, AgentCommand, AgentResponse, AgentStatus},
    config::{self, FileConfig},
    integration::{self, IntegrationStatus, StartupService},
};

const APP_LOGO: &[u8] = include_bytes!("../assets/edgesteer-logo.png");
const DEFAULT_CONFIG: &str = include_str!("../config.example.json");
const NOTICE_DURATION_SECONDS: f64 = 5.0;

static UI_OPTIONS: OnceLock<Mutex<Option<UiOptions>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct UiOptions {
    pub config_path: PathBuf,
    pub app_bundle: Option<PathBuf>,
    pub agent: AgentClient,
}

pub fn run(options: UiOptions) -> Result<(), String> {
    prepare_font_cache();
    let mut slot = UI_OPTIONS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "EdgeSteer UI bootstrap state is unavailable".to_owned())?;
    *slot = Some(options);
    drop(slot);

    app_main();
    Ok(())
}

/// Makepad resolves font dependencies before the first frame. Prefer fonts
/// already installed by the operating system; only fetch the fallback set
/// when no supported CJK font is present. The cache is deliberately outside
/// the App bundle so updates do not require reinstalling EdgeSteer.
fn prepare_font_cache() {
    if system_cjk_font_available() {
        return;
    }
    let Some(cache) = font_cache_directory() else {
        return;
    };
    let mut required = Vec::new();
    if !system_cjk_font_available() {
        required.extend([
            (
                "LXGWWenKaiRegular.ttf",
                "https://raw.githubusercontent.com/lxgw/LxgwWenKai/main/fonts/TTF/LXGWWenKai-Regular.ttf",
            ),
            (
                "LXGWWenKaiBold.ttf",
                "https://raw.githubusercontent.com/lxgw/LxgwWenKai/main/fonts/TTF/LXGWWenKai-Regular.ttf",
            ),
        ]);
    }
    if required.iter().all(|(name, _)| {
        cache
            .join(name)
            .metadata()
            .is_ok_and(|meta| meta.len() > 1024)
    }) {
        return;
    }
    if let Err(error) = fs::create_dir_all(&cache) {
        eprintln!("EdgeSteer: create font cache failed: {error}");
        return;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("EdgeSteer: create font download runtime failed: {error}");
            return;
        }
    };
    runtime.block_on(async {
        let client = match reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                eprintln!("EdgeSteer: create font client failed: {error}");
                return;
            }
        };
        for (name, url) in required {
            let target = cache.join(name);
            if target.metadata().is_ok_and(|meta| meta.len() > 1024) {
                continue;
            }
            match client.get(url).send().await {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => match response.bytes().await {
                        Ok(bytes) if bytes.len() <= 32 * 1024 * 1024 => {
                            let temporary = target.with_extension("download");
                            if fs::write(&temporary, &bytes)
                                .and_then(|()| fs::rename(&temporary, &target))
                                .is_err()
                            {
                                eprintln!("EdgeSteer: save font {name} failed");
                            }
                        }
                        Ok(_) => eprintln!("EdgeSteer: font {name} exceeds size limit"),
                        Err(error) => eprintln!("EdgeSteer: download font {name} failed: {error}"),
                    },
                    Err(error) => eprintln!("EdgeSteer: download font {name} failed: {error}"),
                },
                Err(error) => eprintln!("EdgeSteer: download font {name} failed: {error}"),
            }
        }
    });
}

fn font_cache_directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EDGESTEER_FONT_CACHE") {
        return Some(PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join("Library/Caches/EdgeSteer/fonts"));
    }
    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Some(PathBuf::from(local_app_data).join("EdgeSteer/fonts"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cache/edgesteer/fonts"))
}

#[allow(clippy::needless_return)]
fn system_cjk_font_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        let home_font = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Fonts/LXGWWenKai-Regular.ttf"));
        return home_font.is_some_and(|path| path.is_file());
    }
    #[cfg(target_os = "windows")]
    return ["C:/Windows/Fonts/msyh.ttf", "C:/Windows/Fonts/simhei.ttf"]
        .iter()
        .any(|path| PathBuf::from(path).is_file());
    #[cfg(target_os = "linux")]
    return [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ]
    .iter()
    .any(|path| PathBuf::from(path).is_file());
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    false
}

fn take_ui_options() -> Option<UiOptions> {
    UI_OPTIONS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
}

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    EdgePrimaryButton = <Button> {
        width: Fit
        height: 34
        padding: {left: 13., right: 13., top: 8., bottom: 8.}
        draw_bg: {
            color: #xF48120
            color_hover: #xE06F13
            color_down: #xC75E0D
            border_color_1: #xF48120
            border_color_1_hover: #xE06F13
            border_color_1_down: #xC75E0D
            border_color_2: #xF48120
            border_color_2_hover: #xE06F13
            border_color_2_down: #xC75E0D
            border_radius: 6.
        }
        draw_text: {
            color: #xffffffff
            color_hover: #xffffffff
            color_down: #xffffffff
            text_style: {font_size: 13.}
        }
    }

    EdgeDangerButton = <Button> {
        width: Fit
        height: 34
        padding: {left: 13., right: 13., top: 8., bottom: 8.}
        draw_bg: {
            color: #xB84A3A
            color_hover: #x9F3D30
            color_down: #x843126
            border_color_1: #xB84A3A
            border_color_1_hover: #x9F3D30
            border_color_1_down: #x843126
            border_color_2: #xB84A3A
            border_color_2_hover: #x9F3D30
            border_color_2_down: #x843126
            border_radius: 6.
        }
        draw_text: {
            color: #xffffffff
            color_hover: #xffffffff
            color_down: #xffffffff
            text_style: {font_size: 13.}
        }
    }

    EdgeQuietButton = <Button> {
        width: Fit
        height: 34
        padding: {left: 11., right: 11., top: 8., bottom: 8.}
        draw_bg: {
            color: #x00000000
            color_hover: (THEME_COLOR_OUTSET_HOVER)
            color_down: (THEME_COLOR_OUTSET_ACTIVE)
            border_color_1: #x00000000
            border_color_1_hover: #x00000000
            border_color_1_down: #x00000000
            border_color_2: #x00000000
            border_color_2_hover: #x00000000
            border_color_2_down: #x00000000
            border_radius: 6.
        }
        draw_text: {
            color: (THEME_COLOR_TEXT)
            color_hover: (THEME_COLOR_TEXT_HOVER)
            color_down: (THEME_COLOR_TEXT_DOWN)
            text_style: {font_size: 13.}
        }
    }

    EdgePanel = <RoundedView> {
        width: Fill
        height: Fit
        flow: Down
        spacing: 8.
        padding: {left: 18., right: 18., top: 18., bottom: 18.}
        draw_bg: {
            color: (THEME_COLOR_OUTSET)
            border_color: (THEME_COLOR_BEVEL)
            border_size: 1.
            border_radius: 8.
        }
    }

    EdgeSectionTitle = <Label> {
        width: Fill
        draw_text: {
            color: (THEME_COLOR_TEXT)
            text_style: {font_size: 19.}
        }
    }

    EdgeBody = <Label> {
        width: Fill
        draw_text: {
            color: (THEME_COLOR_TEXT_META)
            text_style: {font_size: 13.}
            wrap: Word
        }
    }

    EdgeMetricTitle = <Label> {
        width: Fill
        draw_text: {
            color: (THEME_COLOR_TEXT_META)
            text_style: {font_size: 12.}
        }
    }

    EdgeMetricValue = <Label> {
        width: Fill
        draw_text: {
            color: (THEME_COLOR_TEXT)
            text_style: {font_size: 16.}
        }
    }

    EdgeFieldLabel = <Label> {
        width: Fill
        draw_text: {
            color: (THEME_COLOR_TEXT_META)
            text_style: {font_size: 12.}
        }
    }

    EdgeSteerMakepadApp = {{EdgeSteerMakepadApp}} {
        ui: <Root> {
            main_window = <Window> {
                window: {
                    title: "EdgeSteer"
                    inner_size: vec2(1180., 780.)
                    position: vec2(120., 80.)
                }
                pass: {clear_color: (THEME_COLOR_BG_APP)}
                body = <View> {
                    width: Fill
                    height: Fill
                    flow: Overlay
                    draw_bg: {color: (THEME_COLOR_BG_APP)}

                    main_content = <View> {
                        width: Fill
                        height: Fill
                        flow: Down

                        top_bar = <RoundedView> {
                            width: Fill
                            height: 58.
                            flow: Right
                            align: {y: 0.5}
                            padding: {left: 26., right: 26., top: 0., bottom: 0.}
                            spacing: 10.
                            draw_bg: {
                                color: (THEME_COLOR_OUTSET)
                                border_color: (THEME_COLOR_BEVEL)
                                border_size: 1.
                                border_radius: 0.
                            }
                            app_logo = <Image> {
                                width: 28.
                                height: 28.
                                fit: Smallest
                            }
                            app_name = <Label> {
                                width: Fit
                                text: "EdgeSteer"
                                draw_text: {
                                    color: (THEME_COLOR_TEXT)
                                    text_style: {font_size: 18.}
                                }
                            }
                            <View> {width: 22. height: Fill}
                            nav_overview = <EdgeQuietButton> {text: "状态"}
                            nav_resolver = <EdgeQuietButton> {text: "解析层"}
                            nav_rules = <EdgeQuietButton> {text: "规则集"}
                            nav_cloudflare = <EdgeQuietButton> {text: "CF 优选"}
                            nav_json = <EdgeQuietButton> {text: "高级 JSON"}
                            nav_system = <EdgeQuietButton> {text: "系统"}
                            <Filler> {}
                            document_state = <Label> {
                                width: Fit
                                text: "正在加载"
                                draw_text: {
                                    color: (THEME_COLOR_TEXT_META)
                                    text_style: {font_size: 12.}
                                }
                            }
                            save_configuration = <EdgePrimaryButton> {text: "保存配置"}
                        }

                        pages = <View> {
                            width: Fill
                            height: Fill
                            flow: Overlay

                            overview_page = <ScrollYView> {
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: 16.
                                padding: {left: 32., right: 32., top: 28., bottom: 28.}
                                overview_title = <EdgeSectionTitle> {text: "运行状态"}
                                overview_copy = <EdgeBody> {
                                    text: "EdgeSteer 在菜单栏持续运行；此窗口用于查看状态和修改配置。"
                                }
                                metrics = <View> {
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    spacing: 12.
                                    engine_metric = <EdgePanel> {
                                        width: Fill
                                        engine_metric_title = <EdgeMetricTitle> {text: "DNS 引擎"}
                                        engine_metric_value = <EdgeMetricValue> {text: "正在检查"}
                                    }
                                    listener_metric = <EdgePanel> {
                                        width: Fill
                                        listener_metric_title = <EdgeMetricTitle> {text: "DNS 监听器"}
                                        listener_metric_value = <EdgeMetricValue> {text: "正在检查"}
                                    }
                                    dns_metric = <EdgePanel> {
                                        width: Fill
                                        dns_metric_title = <EdgeMetricTitle> {text: "系统 DNS"}
                                        dns_metric_value = <EdgeMetricValue> {text: "正在检查"}
                                    }
                                }
                                runtime_panel = <EdgePanel> {
                                    runtime_heading = <EdgeSectionTitle> {text: "DNS 引擎"}
                                    runtime_detail = <EdgeBody> {text: "运行操作通过常驻菜单栏 Agent 执行。"}
                                    runtime_actions = <View> {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 8.
                                        engine_toggle = <EdgePrimaryButton> {text: "启动 DNS 引擎"}
                                        engine_restart = <EdgeQuietButton> {text: "重启"}
                                        runtime_refresh = <EdgeQuietButton> {text: "刷新"}
                                    }
                                }
                                resolver_panel = <EdgePanel> {
                                    resolver_heading = <EdgeSectionTitle> {text: "解析链"}
                                    resolver_detail = <EdgeBody> {text: "正在读取已验证的解析层。"}
                                }
                            }

                            resolver_page = <ScrollYView> {
                                visible: false
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: 16.
                                padding: {left: 32., right: 32., top: 28., bottom: 28.}
                                resolver_title = <EdgeSectionTitle> {text: "解析层"}
                                resolver_copy = <EdgeBody> {text: "按入口顺序编辑动态本地 DNS、DoH、DoT 和 TCP/UDP 层；每层可配置匹配、下一层与故障回退。"}
                                resolver_general = <EdgePanel> {
                                    general_heading = <EdgeSectionTitle> {text: "入口与监听"}
                                    listener_address_label = <EdgeFieldLabel> {text: "监听地址"}
                                    listener_address = <TextInput> {width: Fill height: 34. empty_text: "127.0.0.1:53"}
                                    entry_name_label = <EdgeFieldLabel> {text: "入口层"}
                                    entry_name = <TextInput> {width: Fill height: 34. empty_text: "entry layer tag"}
                                    request_timeout_label = <EdgeFieldLabel> {text: "请求超时（毫秒）"}
                                    request_timeout = <TextInput> {width: Fill height: 34. empty_text: "8000" is_numeric_only: true}
                                    range_refresh_label = <EdgeFieldLabel> {text: "CF 号段刷新（秒）"}
                                    range_refresh = <TextInput> {width: Fill height: 34. empty_text: "86400" is_numeric_only: true}
                                    allow_remote = <CheckBox> {text: "允许远程访问监听器"}
                                }
                                resolver_editor = <EdgePanel> {
                                    layer_toolbar = <View> {width: Fill height: Fit flow: Right spacing: 8.
                                        layer_select = <DropDownFlat> {width: Fill height: 34. popup_menu_position: BelowInput}
                                        layer_add = <EdgePrimaryButton> {text: "添加层"}
                                        layer_up = <EdgeQuietButton> {text: "上移"}
                                        layer_down = <EdgeQuietButton> {text: "下移"}
                                        layer_remove = <EdgeDangerButton> {text: "删除"}
                                    }
                                    layer_tag_label = <EdgeFieldLabel> {text: "层标签"}
                                    layer_tag = <TextInput> {width: Fill height: 34. empty_text: "layer tag"}
                                    layer_type_label = <EdgeFieldLabel> {text: "层类型"}
                                    layer_type = <DropDownFlat> {width: Fill height: 34. popup_menu_position: BelowInput}
                                    layer_next_label = <EdgeFieldLabel> {text: "下一层（未命中时）"}
                                    layer_next = <TextInput> {width: Fill height: 34. empty_text: "next layer tag"}
                                    layer_fallback_label = <EdgeFieldLabel> {text: "回退层（请求失败时）"}
                                    layer_fallback = <TextInput> {width: Fill height: 34. empty_text: "fallback layer tag"}
                                    layer_plugin_label = <EdgeFieldLabel> {text: "响应插件"}
                                    layer_plugin = <TextInput> {width: Fill height: 34. empty_text: "plugin tag (optional)"}
                                    layer_address_label = <EdgeFieldLabel> {text: "上游地址"}
                                    layer_address = <TextInput> {width: Fill height: 34. empty_text: "1.1.1.1:443"}
                                    layer_url_label = <EdgeFieldLabel> {text: "DoH URL"}
                                    layer_url = <TextInput> {width: Fill height: 34. empty_text: "https://.../dns-query"}
                                    layer_server_name_label = <EdgeFieldLabel> {text: "DoT 服务名"}
                                    layer_server_name = <TextInput> {width: Fill height: 34. empty_text: "DoT server name"}
                                    layer_timeout_label = <EdgeFieldLabel> {text: "层超时（毫秒）"}
                                    layer_timeout = <TextInput> {width: Fill height: 34. empty_text: "3000" is_numeric_only: true}
                                    layer_refresh_label = <EdgeFieldLabel> {text: "本地 DNS 刷新（秒）"}
                                    layer_refresh = <TextInput> {width: Fill height: 34. empty_text: "30" is_numeric_only: true}
                                    layer_match_mode_label = <EdgeFieldLabel> {text: "匹配模式"}
                                    layer_match_mode = <DropDownFlat> {width: Fill height: 34. popup_menu_position: BelowInput}
                                    layer_keywords_label = <EdgeFieldLabel> {text: "关键词（逗号分隔）"}
                                    layer_keywords = <TextInput> {width: Fill height: 34. empty_text: "keywords, comma separated"}
                                    layer_rulesets_label = <EdgeFieldLabel> {text: "规则集标签（逗号分隔）"}
                                    layer_rulesets = <TextInput> {width: Fill height: 34. empty_text: "rule-set tags, comma separated"}
                                }
                            }

                            rules_page = <ScrollYView> {
                                visible: false width: Fill height: Fill flow: Down spacing: 16. padding: {left: 32. right: 32. top: 28. bottom: 28.}
                                rules_title = <EdgeSectionTitle> {text: "规则集"}
                                rules_copy = <EdgeBody> {text: "管理本地或远程 SRS 规则集，并将标签绑定到解析层匹配。"}
                                rules_editor = <EdgePanel> {
                                    rules_toolbar = <View> {width: Fill height: Fit flow: Right spacing: 8.
                                        rules_select = <DropDownFlat> {width: Fill height: 34. popup_menu_position: BelowInput}
                                        rules_type = <DropDownFlat> {width: 180. height: 34. popup_menu_position: BelowInput}
                                        rules_add = <EdgePrimaryButton> {text: "添加规则集"}
                                        rules_remove = <EdgeDangerButton> {text: "删除"}
                                    }
                                    rules_tag_label = <EdgeFieldLabel> {text: "规则集标签"}
                                    rules_tag = <TextInput> {width: Fill height: 34. empty_text: "rule-set tag"}
                                    rules_source_label = <EdgeFieldLabel> {text: "规则集来源"}
                                    rules_source = <TextInput> {width: Fill height: 34. empty_text: "https://...srs or /path/file.srs"}
                                    rules_interval_label = <EdgeFieldLabel> {text: "刷新间隔（秒）"}
                                    rules_interval = <TextInput> {width: Fill height: 34. empty_text: "86400" is_numeric_only: true}
                                    rules_timeout_label = <EdgeFieldLabel> {text: "下载超时（毫秒）"}
                                    rules_timeout = <TextInput> {width: Fill height: 34. empty_text: "10000" is_numeric_only: true}
                                }
                            }

                            cloudflare_page = <ScrollYView> {
                                visible: false width: Fill height: Fill flow: Down spacing: 16. padding: {left: 32. right: 32. top: 28. bottom: 28.}
                                cloudflare_title = <EdgeSectionTitle> {text: "CF 优选"}
                                cloudflare_copy = <EdgeBody> {text: "配置 Cloudflare 响应重写、固定优选地址和稳定优选探测范围。"}
                                cloudflare_editor = <EdgePanel> {
                                    plugin_toolbar = <View> {width: Fill height: Fit flow: Right spacing: 8.
                                        plugin_select = <DropDownFlat> {width: Fill height: 34. popup_menu_position: BelowInput}
                                        plugin_add = <EdgePrimaryButton> {text: "添加插件"}
                                        plugin_remove = <EdgeDangerButton> {text: "删除"}
                                    }
                                    plugin_tag_label = <EdgeFieldLabel> {text: "插件标签"}
                                    plugin_tag = <TextInput> {width: Fill height: 34. empty_text: "plugin tag"}
                                    plugin_ttl_label = <EdgeFieldLabel> {text: "重写 TTL（秒）"}
                                    plugin_ttl = <TextInput> {width: Fill height: 34. empty_text: "60" is_numeric_only: true}
                                    plugin_ipv4_label = <EdgeFieldLabel> {text: "固定优选 IPv4（可选）"}
                                    plugin_ipv4 = <TextInput> {width: Fill height: 34. empty_text: "preferred IPv4 (optional)"}
                                    plugin_ipv6_label = <EdgeFieldLabel> {text: "固定优选 IPv6（可选）"}
                                    plugin_ipv6 = <TextInput> {width: Fill height: 34. empty_text: "preferred IPv6 (optional)"}
                                    optimizer_enabled = <CheckBox> {text: "启用定时优选"}
                                    optimizer_interval_label = <EdgeFieldLabel> {text: "优选间隔（秒）"}
                                    optimizer_interval = <TextInput> {width: Fill height: 34. empty_text: "21600" is_numeric_only: true}
                                    optimizer_host_label = <EdgeFieldLabel> {text: "探测主机"}
                                    optimizer_host = <TextInput> {width: Fill height: 34. empty_text: "www.cloudflare.com"}
                                    optimizer_path_label = <EdgeFieldLabel> {text: "探测路径"}
                                    optimizer_path = <TextInput> {width: Fill height: 34. empty_text: "/cdn-cgi/trace"}
                                    optimizer_port_label = <EdgeFieldLabel> {text: "探测端口"}
                                    optimizer_port = <TextInput> {width: Fill height: 34. empty_text: "443" is_numeric_only: true}
                                    optimizer_timeout_label = <EdgeFieldLabel> {text: "探测超时（毫秒）"}
                                    optimizer_timeout = <TextInput> {width: Fill height: 34. empty_text: "3000" is_numeric_only: true}
                                    optimizer_concurrency_label = <EdgeFieldLabel> {text: "并发数"}
                                    optimizer_concurrency = <TextInput> {width: Fill height: 34. empty_text: "32" is_numeric_only: true}
                                    optimizer_samples_label = <EdgeFieldLabel> {text: "每个 CIDR 采样数"}
                                    optimizer_samples = <TextInput> {width: Fill height: 34. empty_text: "40" is_numeric_only: true}
                                    optimizer_probes_label = <EdgeFieldLabel> {text: "每个候选探测数"}
                                    optimizer_probes = <TextInput> {width: Fill height: 34. empty_text: "3" is_numeric_only: true}
                                    optimizer_max_label = <EdgeFieldLabel> {text: "最大候选数"}
                                    optimizer_max = <TextInput> {width: Fill height: 34. empty_text: "640" is_numeric_only: true}
                                    optimizer_candidates_label = <EdgeFieldLabel> {text: "候选 IP/CIDR（逗号分隔）"}
                                    optimizer_candidates = <TextInput> {width: Fill height: 34. empty_text: "CIDRs, comma separated"}
                                    optimizer_compatibility_label = <EdgeFieldLabel> {text: "兼容性主机（逗号分隔）"}
                                    optimizer_compatibility = <TextInput> {width: Fill height: 34. empty_text: "compatibility hosts, comma separated"}
                                    optimizer_excluded_label = <EdgeFieldLabel> {text: "排除 IP/CIDR（逗号分隔）"}
                                    optimizer_excluded = <TextInput> {width: Fill height: 34. empty_text: "excluded CIDRs, comma separated"}
                                }
                            }

                            json_page = <ScrollYView> {
                                visible: false
                                width: Fill height: Fill flow: Down spacing: 16. padding: {left: 32. right: 32. top: 28. bottom: 28.}
                                json_title = <EdgeSectionTitle> {text: "高级 JSON"}
                                json_copy = <EdgeBody> {text: "直接编辑完整配置。表单页和这里共享同一份严格校验，适合批量修改未暴露字段。"}
                                config_panel = <EdgePanel> {
                                    config_path_label = <EdgeMetricTitle> {text: "配置文件"}
                                    config_path_value = <EdgeMetricValue> {text: "~/edgesteer.json"}
                                    config_validation = <EdgeBody> {text: "正在验证"}
                                    config_actions = <View> {width: Fill height: Fit flow: Right spacing: 8.
                                        reload_configuration = <EdgeQuietButton> {text: "重新加载"}
                                        save_configuration_page = <EdgePrimaryButton> {text: "保存配置"}
                                    }
                                }
                                editor_panel = <EdgePanel> {width: Fill height: 470. editor_heading = <EdgeSectionTitle> {text: "JSON"}
                                    config_editor = <TextInput> {width: Fill height: Fill flow: RightWrap empty_text: "{ }"
                                        draw_bg: {color: (THEME_COLOR_INSET) color_hover: (THEME_COLOR_INSET_HOVER) color_focus: (THEME_COLOR_INSET_FOCUS) color_empty: (THEME_COLOR_INSET_EMPTY) border_color_1: (THEME_COLOR_BEVEL_INSET_1) border_color_1_hover: (THEME_COLOR_BEVEL_INSET_1_HOVER) border_color_1_focus: #xF48120 border_color_2: (THEME_COLOR_BEVEL_INSET_2) border_color_2_hover: (THEME_COLOR_BEVEL_INSET_2_HOVER) border_color_2_focus: #xF48120 border_radius: 6.}
                                        draw_text: {color: (THEME_COLOR_TEXT) color_hover: (THEME_COLOR_TEXT_HOVER) color_focus: (THEME_COLOR_TEXT_FOCUS) color_down: (THEME_COLOR_TEXT_DOWN) color_empty: (THEME_COLOR_TEXT_PLACEHOLDER) text_style: {font_size: 12.}}
                                        draw_cursor: {color: (THEME_COLOR_CURSOR)}
                                    }
                                }
                            }

                            system_page = <ScrollYView> {
                                visible: false
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: 16.
                                padding: {left: 32., right: 32., top: 28., bottom: 28.}
                                system_title = <EdgeSectionTitle> {text: "系统"}
                                system_copy = <EdgeBody> {
                                    text: "菜单栏是主控制面；关闭此窗口只会释放图形界面，不会停止 DNS 服务。"
                                }
                                appearance_panel = <EdgePanel> {
                                    appearance_heading = <EdgeSectionTitle> {text: "应用"}
                                    appearance_copy = <EdgeBody> {
                                        text: "选择界面语言、黑白风格与关闭窗口后的行为。"
                                    }
                                    appearance_rows = <View> {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 16.
                                        language_select = <DropDownFlat> {
                                            width: 150.
                                            height: 34.
                                            popup_menu_position: BelowInput
                                            labels: ["简体中文", "English"]
                                            values: [Chinese, English]
                                        }
                                        appearance_select = <DropDownFlat> {
                                            width: 130.
                                            height: 34.
                                            popup_menu_position: BelowInput
                                            labels: ["深色", "浅色"]
                                            values: [Dark, Light]
                                        }
                                        close_behavior_select = <DropDownFlat> {
                                            width: 230.
                                            height: 34.
                                            popup_menu_position: BelowInput
                                            labels: ["关闭窗口仍保持运行", "关闭窗口时退出 EdgeSteer"]
                                            values: [KeepRunning, Quit]
                                        }
                                    }
                                }
                                startup_panel = <EdgePanel> {
                                    startup_heading = <EdgeSectionTitle> {text: "登录启动"}
                                    startup_detail = <EdgeBody> {text: "正在检查启动项。"}
                                    startup_action = <EdgeQuietButton> {text: "正在检查"}
                                }
                                system_dns_panel = <EdgePanel> {
                                    system_dns_heading = <EdgeSectionTitle> {text: "系统 DNS"}
                                    system_dns_detail = <EdgeBody> {text: "正在检查系统 DNS。"}
                                    system_dns_action = <EdgeQuietButton> {text: "正在检查"}
                                }
                                legacy_panel = <EdgePanel> {
                                    visible: false
                                    legacy_heading = <EdgeSectionTitle> {text: "旧版服务"}
                                    legacy_detail = <EdgeBody> {text: "检测到旧版命令行服务。"}
                                    legacy_action = <EdgeQuietButton> {text: "移除旧版服务"}
                                }
                                services_panel = <EdgePanel> {
                                    services_heading = <EdgeSectionTitle> {text: "网络服务"}
                                    services_detail = <EdgeBody> {text: "正在读取网络服务。"}
                                }
                            }
                        }
                    }

                    notice_popup = <PopupNotification> {
                        align: {x: 0.5, y: 0.0}
                        content: <View> {
                            width: 520.
                            height: Fit
                            margin: {top: 18., right: 0., bottom: 0., left: 0.}
                            notice_panel = <RoundedView> {
                                width: Fill
                                height: Fit
                                padding: {left: 16., right: 16., top: 12., bottom: 12.}
                                draw_bg: {
                                    color: (THEME_COLOR_OUTSET)
                                    border_color: #xF48120
                                    border_size: 1.
                                    border_radius: 7.
                                }
                                notice_text = <EdgeBody> {text: ""}
                            }
                        }
                    }

                    confirmation_modal = <Modal> {
                        content: <View> {
                            width: 450.
                            height: Fit
                            confirmation_panel = <RoundedView> {
                                width: Fill
                                height: Fit
                                flow: Down
                                spacing: 16.
                                padding: {left: 24., right: 24., top: 24., bottom: 24.}
                                draw_bg: {
                                    color: (THEME_COLOR_OUTSET)
                                    border_color: (THEME_COLOR_BEVEL)
                                    border_size: 1.
                                    border_radius: 8.
                                }
                                confirmation_title = <EdgeSectionTitle> {text: "确认操作"}
                                confirmation_detail = <EdgeBody> {text: ""}
                                confirmation_actions = <View> {
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    align: {x: 1.0, y: 0.5}
                                    spacing: 8.
                                    cancel_confirmation = <EdgeQuietButton> {text: "取消"}
                                    confirm_primary = <EdgePrimaryButton> {text: "继续"}
                                    confirm_danger = <EdgeDangerButton> {
                                        visible: false
                                        text: "继续"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

app_main!(EdgeSteerMakepadApp);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Resolver,
    RuleSets,
    Cloudflare,
    Json,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Udp,
    Tcp,
    Doh,
    Dot,
    Local,
}

impl LayerKind {
    const ALL: [Self; 5] = [Self::Udp, Self::Tcp, Self::Doh, Self::Dot, Self::Local];

    const fn wire_name(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Doh => "doh",
            Self::Dot => "dot",
            Self::Local => "local",
        }
    }

    const fn label(self, language: Language) -> &'static str {
        match self {
            Self::Udp => language.text("UDP", "UDP"),
            Self::Tcp => language.text("TCP", "TCP"),
            Self::Doh => language.text("DoH", "DoH"),
            Self::Dot => language.text("DoT", "DoT"),
            Self::Local => language.text("动态本地 DNS", "Dynamic local DNS"),
        }
    }

    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("udp") => Self::Udp,
            Some("tcp") => Self::Tcp,
            Some("doh") => Self::Doh,
            Some("dot") => Self::Dot,
            _ => Self::Local,
        }
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

    const fn label(self, language: Language) -> &'static str {
        match self {
            Self::Label => language.text("完整 DNS 标签", "Full DNS label"),
            Self::Contains => language.text("包含关键词", "Literal substring"),
        }
    }

    fn from_value(value: Option<&str>) -> Self {
        if value == Some("contains") {
            Self::Contains
        } else {
            Self::Label
        }
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

    const fn label(self, language: Language) -> &'static str {
        match self {
            Self::Remote => language.text("远程 SRS", "Remote SRS"),
            Self::Local => language.text("本地 SRS", "Local SRS"),
        }
    }

    fn from_value(value: Option<&str>) -> Self {
        if value == Some("local") {
            Self::Local
        } else {
            Self::Remote
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Language {
    Chinese,
    English,
}

impl Default for Language {
    fn default() -> Self {
        Self::Chinese
    }
}

impl Language {
    const fn text(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AppearanceMode {
    Dark,
    Light,
}

impl Default for AppearanceMode {
    fn default() -> Self {
        Self::Dark
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

struct ConfigDocument {
    path: PathBuf,
    saved: String,
    draft: String,
    value: Value,
    parsed: Option<FileConfig>,
    validation_error: Option<String>,
}

impl ConfigDocument {
    fn load(path: PathBuf, language: Language) -> (Self, Option<String>) {
        match fs::read_to_string(&path) {
            Ok(contents) => (Self::from_contents(path, contents), None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                Self::from_contents(path, DEFAULT_CONFIG.to_owned()),
                Some(
                    language
                        .text(
                            "配置文件不存在，已载入内置默认配置草稿。",
                            "The configuration file does not exist; the bundled default is loaded as a draft.",
                        )
                        .to_owned(),
                ),
            ),
            Err(error) => (
                Self::from_contents(path.clone(), String::new()),
                Some(format!(
                    "{} {}: {error}",
                    language.text("读取配置失败", "Could not read configuration"),
                    path.display()
                )),
            ),
        }
    }

    fn from_contents(path: PathBuf, contents: String) -> Self {
        let value = serde_json::from_str(&contents).unwrap_or_else(|_| Value::Object(Map::new()));
        let mut document = Self {
            path,
            saved: contents.clone(),
            draft: contents,
            value,
            parsed: None,
            validation_error: None,
        };
        document.revalidate();
        document
    }

    fn replace_draft(&mut self, value: String) {
        if let Ok(parsed) = serde_json::from_str::<Value>(&value) {
            self.value = parsed;
        }
        self.draft = value;
        self.revalidate();
    }

    fn reload(&mut self, language: Language) -> Option<String> {
        let (document, notice) = Self::load(self.path.clone(), language);
        *self = document;
        notice
    }

    fn save(&mut self, language: Language) -> Result<(), String> {
        config::write_config_atomically(&self.path, &self.draft).map_err(|error| {
            format!(
                "{} {}: {error:#}",
                language.text("保存配置失败", "Could not save configuration"),
                self.path.display()
            )
        })?;
        self.saved = self.draft.clone();
        Ok(())
    }

    fn revalidate(&mut self) {
        match config::parse_config_text(&self.draft) {
            Ok(config) => {
                self.parsed = Some(config);
                self.validation_error = None;
            }
            Err(error) => {
                self.parsed = None;
                self.validation_error = Some(format!("{error:#}"));
            }
        }
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
        let layers = self.layers_mut();
        layers.push(default_layer(&tag, kind));
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
            index.checked_add(1).filter(|next| *next < layers.len())
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
        let rule_sets = self.rule_sets_mut();
        rule_sets.push(default_rule_set(&tag, kind));
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
            .expect("configuration root is an object")
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
        self.draft = serde_json::to_string_pretty(&self.value)
            .expect("configuration values are serializable");
        self.revalidate();
    }

    fn is_valid(&self) -> bool {
        self.validation_error.is_none()
    }

    fn is_dirty(&self) -> bool {
        self.saved != self.draft
    }

    fn listener(&self) -> SocketAddr {
        self.parsed
            .as_ref()
            .map(|config| config.listener.address)
            .unwrap_or_else(|| "127.0.0.1:53".parse().expect("fallback listener is valid"))
    }

    fn resolver_summary(&self, language: Language) -> String {
        let Some(config) = &self.parsed else {
            return language
                .text(
                    "配置尚未通过校验。",
                    "Configuration has not passed validation.",
                )
                .to_owned();
        };

        let mut lines = Vec::with_capacity(config.layers.len() + 2);
        lines.push(format!(
            "{} {}",
            language.text("入口层：", "Entry: "),
            config.entry
        ));
        for layer in &config.layers {
            let mut line = format!("{} ({:?})", layer.tag, layer.kind);
            if let Some(next) = &layer.next {
                line.push_str(&format!(" -> {next}"));
            }
            if let Some(fallback) = &layer.fallback {
                line.push_str(&format!(
                    " | {} {fallback}",
                    language.text("回退", "fallback")
                ));
            }
            if let Some(plugin) = &layer.plugin {
                line.push_str(&format!(" | plugin: {plugin}"));
            }
            lines.push(line);
        }
        lines.push(format!(
            "{} {}   {} {}",
            language.text("规则集：", "Rule sets: "),
            config.rule_sets.len(),
            language.text("插件：", "Plugins: "),
            config.plugins.len()
        ));
        lines.join("\n")
    }

    fn validation_summary(&self, language: Language) -> String {
        match &self.validation_error {
            Some(error) => format!(
                "{}: {error}",
                language.text("配置无效", "Invalid configuration")
            ),
            None if self.is_dirty() => language
                .text("有效草稿，尚未保存", "Valid draft, not saved")
                .to_owned(),
            None => language
                .text("配置已保存", "Configuration saved")
                .to_owned(),
        }
    }
}

fn object_mut<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    parent
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("configuration object field remains an object")
}

fn array_mut<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    parent
        .entry(key.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("configuration array field remains an array")
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
        .expect("unique tag search is bounded by u32")
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
            layer
                .entry("timeout_ms".to_owned())
                .or_insert_with(|| json!(1800));
            layer
                .entry("refresh_secs".to_owned())
                .or_insert_with(|| json!(30));
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

fn set_text_input(ui: &WidgetRef, cx: &mut Cx, id: &[LiveId], value: String) {
    ui.text_input(id).set_text(cx, &value);
}

fn sync_layer_fields(ui: &WidgetRef, cx: &mut Cx, layer: Option<&Map<String, Value>>) {
    let string = |key: &str| {
        layer
            .and_then(|value| object_string(value, key))
            .unwrap_or_default()
            .to_owned()
    };
    let number = |key: &str, default: u64| {
        layer
            .map(|value| object_number(value, key, default))
            .unwrap_or(default)
            .to_string()
    };
    set_text_input(ui, cx, id!(layer_tag), string("tag"));
    set_text_input(ui, cx, id!(layer_next), string("next"));
    set_text_input(ui, cx, id!(layer_fallback), string("fallback"));
    set_text_input(ui, cx, id!(layer_plugin), string("plugin"));
    set_text_input(ui, cx, id!(layer_address), string("address"));
    set_text_input(ui, cx, id!(layer_url), string("url"));
    set_text_input(ui, cx, id!(layer_server_name), string("server_name"));
    set_text_input(ui, cx, id!(layer_timeout), number("timeout_ms", 3000));
    set_text_input(ui, cx, id!(layer_refresh), number("refresh_secs", 30));
    let kind = LayerKind::from_value(layer.and_then(|value| object_string(value, "type")));
    ui.drop_down(id!(layer_type)).set_selected_item(
        cx,
        LayerKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or(0),
    );
    let matcher = layer
        .and_then(|value| value.get("match"))
        .and_then(Value::as_object);
    let mode = MatchMode::from_value(matcher.and_then(|value| object_string(value, "mode")));
    ui.drop_down(id!(layer_match_mode)).set_selected_item(
        cx,
        MatchMode::ALL
            .iter()
            .position(|candidate| *candidate == mode)
            .unwrap_or(0),
    );
    set_text_input(
        ui,
        cx,
        id!(layer_keywords),
        matcher
            .map(|value| object_list(value, "keywords"))
            .unwrap_or_default(),
    );
    set_text_input(
        ui,
        cx,
        id!(layer_rulesets),
        matcher
            .map(|value| object_list(value, "rule_sets"))
            .unwrap_or_default(),
    );
}

fn sync_rule_fields(ui: &WidgetRef, cx: &mut Cx, rule_set: Option<&Map<String, Value>>) {
    let kind = RuleSetKind::from_value(rule_set.and_then(|value| object_string(value, "type")));
    let string = |key: &str| {
        rule_set
            .and_then(|value| object_string(value, key))
            .unwrap_or_default()
            .to_owned()
    };
    let source = match kind {
        RuleSetKind::Remote => string("url"),
        RuleSetKind::Local => string("path"),
    };
    set_text_input(ui, cx, id!(rules_tag), string("tag"));
    set_text_input(ui, cx, id!(rules_source), source);
    set_text_input(
        ui,
        cx,
        id!(rules_interval),
        rule_set
            .map(|value| {
                object_number(
                    value,
                    "update_interval_secs",
                    if kind == RuleSetKind::Remote {
                        86400
                    } else {
                        60
                    },
                )
            })
            .unwrap_or(60)
            .to_string(),
    );
    set_text_input(
        ui,
        cx,
        id!(rules_timeout),
        rule_set
            .map(|value| object_number(value, "timeout_ms", 10000))
            .unwrap_or(10000)
            .to_string(),
    );
    ui.drop_down(id!(rules_type)).set_selected_item(
        cx,
        RuleSetKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or(0),
    );
}

fn sync_plugin_fields(ui: &WidgetRef, cx: &mut Cx, plugin: Option<&Map<String, Value>>) {
    let string = |key: &str| {
        plugin
            .and_then(|value| object_string(value, key))
            .unwrap_or_default()
            .to_owned()
    };
    let preferred = plugin
        .and_then(|value| value.get("preferred"))
        .and_then(Value::as_object);
    let optimizer = plugin
        .and_then(|value| value.get("optimizer"))
        .and_then(Value::as_object);
    set_text_input(ui, cx, id!(plugin_tag), string("tag"));
    set_text_input(
        ui,
        cx,
        id!(plugin_ttl),
        plugin
            .map(|value| object_number(value, "rewrite_ttl_secs", 60))
            .unwrap_or(60)
            .to_string(),
    );
    set_text_input(
        ui,
        cx,
        id!(plugin_ipv4),
        preferred
            .map(|value| object_string(value, "ipv4").unwrap_or_default())
            .unwrap_or_default()
            .to_owned(),
    );
    set_text_input(
        ui,
        cx,
        id!(plugin_ipv6),
        preferred
            .map(|value| object_string(value, "ipv6").unwrap_or_default())
            .unwrap_or_default()
            .to_owned(),
    );
    ui.check_box(id!(optimizer_enabled)).set_active(
        cx,
        optimizer
            .map(|value| object_bool(value, "enabled", false))
            .unwrap_or(false),
    );
    let num = |key: &str, default: u64| {
        optimizer
            .map(|value| object_number(value, key, default))
            .unwrap_or(default)
            .to_string()
    };
    set_text_input(ui, cx, id!(optimizer_interval), num("interval_secs", 21600));
    set_text_input(
        ui,
        cx,
        id!(optimizer_host),
        optimizer
            .map(|value| object_string(value, "test_host").unwrap_or("www.cloudflare.com"))
            .unwrap_or("www.cloudflare.com")
            .to_owned(),
    );
    set_text_input(
        ui,
        cx,
        id!(optimizer_path),
        optimizer
            .map(|value| object_string(value, "test_path").unwrap_or("/cdn-cgi/trace"))
            .unwrap_or("/cdn-cgi/trace")
            .to_owned(),
    );
    set_text_input(ui, cx, id!(optimizer_port), num("test_port", 443));
    set_text_input(ui, cx, id!(optimizer_timeout), num("timeout_ms", 3000));
    set_text_input(ui, cx, id!(optimizer_concurrency), num("concurrency", 32));
    set_text_input(ui, cx, id!(optimizer_samples), num("samples_per_cidr", 40));
    set_text_input(
        ui,
        cx,
        id!(optimizer_probes),
        num("probes_per_candidate", 3),
    );
    set_text_input(ui, cx, id!(optimizer_max), num("max_candidates", 640));
    set_text_input(
        ui,
        cx,
        id!(optimizer_candidates),
        optimizer
            .map(|value| object_list(value, "candidates"))
            .unwrap_or_default(),
    );
    set_text_input(
        ui,
        cx,
        id!(optimizer_compatibility),
        optimizer
            .map(|value| object_list(value, "compatibility_hosts"))
            .unwrap_or_default(),
    );
    set_text_input(
        ui,
        cx,
        id!(optimizer_excluded),
        optimizer
            .map(|value| object_list(value, "excluded_candidates"))
            .unwrap_or_default(),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentAction {
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

impl AgentAction {
    const fn command(self) -> AgentCommand {
        match self {
            Self::StartEngine => AgentCommand::StartEngine,
            Self::StopEngine => AgentCommand::StopEngine,
            Self::RestartEngine => AgentCommand::RestartEngine,
            Self::EnableSystemDns => AgentCommand::EnableSystemDns,
            Self::DisableSystemDns => AgentCommand::DisableSystemDns,
            Self::EnableAutoStart => AgentCommand::EnableAutoStart,
            Self::DisableAutoStart => AgentCommand::DisableAutoStart,
            Self::RemoveLegacyService => AgentCommand::RemoveLegacyService,
            Self::Refresh => AgentCommand::Refresh,
            Self::Quit => AgentCommand::Quit,
        }
    }

    const fn requires_valid_configuration(self) -> bool {
        matches!(
            self,
            Self::StartEngine | Self::RestartEngine | Self::EnableSystemDns | Self::EnableAutoStart
        )
    }

    const fn requires_confirmation(self) -> bool {
        !matches!(self, Self::Refresh)
    }

    const fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::StopEngine
                | Self::DisableSystemDns
                | Self::DisableAutoStart
                | Self::RemoveLegacyService
                | Self::Quit
        )
    }

    const fn label(self, language: Language) -> &'static str {
        match self {
            Self::StartEngine => language.text("启动 DNS 引擎", "Start DNS engine"),
            Self::StopEngine => language.text("停止 DNS 引擎", "Stop DNS engine"),
            Self::RestartEngine => language.text("重启 DNS 引擎", "Restart DNS engine"),
            Self::EnableSystemDns => language.text("启用 EdgeSteer DNS", "Enable EdgeSteer DNS"),
            Self::DisableSystemDns => language.text("恢复自动 DNS", "Restore automatic DNS"),
            Self::EnableAutoStart => language.text("启用登录启动", "Enable open at login"),
            Self::DisableAutoStart => language.text("关闭登录启动", "Disable open at login"),
            Self::RemoveLegacyService => language.text("移除旧版服务", "Remove legacy service"),
            Self::Refresh => language.text("刷新运行状态", "Refresh runtime"),
            Self::Quit => language.text("退出 EdgeSteer", "Quit EdgeSteer"),
        }
    }

    const fn confirmation(self, language: Language) -> &'static str {
        match self {
            Self::StartEngine => language.text(
                "将启动由 EdgeSteer App 管理的 DNS 引擎。监听 53 端口时，macOS 可能请求管理员授权。",
                "This starts the DNS engine managed by EdgeSteer. macOS may request authorization for port 53.",
            ),
            Self::StopEngine => language.text(
                "将先恢复 EdgeSteer 接管的系统 DNS，再停止 DNS 引擎。",
                "This restores EdgeSteer-managed system DNS before stopping the DNS engine.",
            ),
            Self::RestartEngine => language.text(
                "将重启 DNS 引擎。已有解析请求可能短暂中断。",
                "This restarts the DNS engine. Existing DNS requests may be briefly interrupted.",
            ),
            Self::EnableSystemDns => language.text(
                "将让符合条件的物理网络服务使用 EdgeSteer 的回环 DNS。",
                "This changes eligible physical network services to use EdgeSteer's loopback DNS.",
            ),
            Self::DisableSystemDns => language.text(
                "将只恢复 EdgeSteer 自己接管的 DNS 服务到自动 DNS。",
                "This restores only the DNS services currently managed by EdgeSteer to automatic DNS.",
            ),
            Self::EnableAutoStart => language.text(
                "登录时会启动已安装的 EdgeSteer App，不会安装独立命令行服务。",
                "This opens the installed EdgeSteer App at login without installing a separate command-line service.",
            ),
            Self::DisableAutoStart => language.text(
                "将从当前用户的登录启动项中移除 EdgeSteer。",
                "This removes EdgeSteer from the current user's login items.",
            ),
            Self::RemoveLegacyService => language.text(
                "将移除旧版 root DNS 守护进程。macOS 可能请求管理员授权。",
                "This removes the legacy root DNS daemon. macOS may request authorization.",
            ),
            Self::Refresh => language.text("正在刷新。", "Refreshing."),
            Self::Quit => language.text(
                "EdgeSteer 会先恢复其接管的系统 DNS，再停止 DNS 引擎和菜单栏服务。",
                "EdgeSteer restores the system DNS it manages, then stops the DNS engine and menu-bar service.",
            ),
        }
    }
}

enum UiEvent {
    AgentStatus(Result<AgentStatus, String>),
    Integration(Result<IntegrationStatus, String>),
    AgentAction {
        action: AgentAction,
        result: Result<AgentResponse, String>,
    },
}

struct UiState {
    options: Option<UiOptions>,
    page: Page,
    selected_layer: usize,
    selected_rule_set: usize,
    selected_plugin: usize,
    preferences: UiPreferences,
    document: Option<ConfigDocument>,
    agent_status: Option<AgentStatus>,
    integration: Option<IntegrationStatus>,
    pending_action: Option<AgentAction>,
    busy: bool,
    notice_timer: Timer,
    to_ui: ToUIReceiver<UiEvent>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            options: None,
            page: Page::Overview,
            selected_layer: 0,
            selected_rule_set: 0,
            selected_plugin: 0,
            preferences: UiPreferences::default(),
            document: None,
            agent_status: None,
            integration: None,
            pending_action: None,
            busy: false,
            notice_timer: Timer::empty(),
            to_ui: ToUIReceiver::default(),
        }
    }
}

#[derive(Live, LiveHook)]
struct EdgeSteerMakepadApp {
    #[live]
    ui: WidgetRef,
    #[rust]
    state: UiState,
}

impl LiveRegister for EdgeSteerMakepadApp {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        cx.link(live_id!(theme), live_id!(theme_desktop_light));
    }
}

impl MatchEvent for EdgeSteerMakepadApp {
    fn handle_startup(&mut self, cx: &mut Cx) {
        #[cfg(target_os = "macos")]
        crate::agent::configure_menu_bar_activation_policy();

        self.state.options = take_ui_options();
        self.state.preferences = match UiPreferences::load() {
            Ok(preferences) => preferences,
            Err(error) => {
                self.show_notice(cx, error);
                UiPreferences::default()
            }
        };

        let Some(options) = self.state.options.as_ref() else {
            self.show_notice(
                cx,
                self.text(
                    "无法启动界面：缺少 EdgeSteer Agent 配置。",
                    "Could not start the UI: EdgeSteer Agent options are missing.",
                ),
            );
            return;
        };
        let (document, document_notice) =
            ConfigDocument::load(options.config_path.clone(), self.state.preferences.language);
        self.state.document = Some(document);

        self.apply_theme(cx);
        let _ = self
            .ui
            .image(id!(app_logo))
            .load_png_from_data(cx, APP_LOGO);
        self.sync_ui(cx, true);
        if let Some(notice) = document_notice {
            self.show_notice(cx, notice);
        }
        self.refresh_runtime(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let ui = self.ui.clone();

        if ui.button(id!(nav_overview)).clicked(actions) {
            self.state.page = Page::Overview;
            self.sync_page(cx);
        }
        if ui.button(id!(nav_resolver)).clicked(actions) {
            self.state.page = Page::Resolver;
            self.sync_page(cx);
        }
        if ui.button(id!(nav_rules)).clicked(actions) {
            self.state.page = Page::RuleSets;
            self.sync_page(cx);
        }
        if ui.button(id!(nav_cloudflare)).clicked(actions) {
            self.state.page = Page::Cloudflare;
            self.sync_page(cx);
        }
        if ui.button(id!(nav_json)).clicked(actions) {
            self.state.page = Page::Json;
            self.sync_page(cx);
        }

        self.handle_form_actions(cx, actions);
        if ui.button(id!(nav_system)).clicked(actions) {
            self.state.page = Page::System;
            self.sync_page(cx);
        }

        if let Some(contents) = ui.text_input(id!(config_editor)).changed(actions) {
            if let Some(document) = self.state.document.as_mut() {
                document.replace_draft(contents);
                self.sync_document_status(cx);
            }
            if self.document().is_some_and(ConfigDocument::is_valid) {
                self.sync_structured_controls(cx);
            }
        }

        if ui.button(id!(save_configuration)).clicked(actions)
            || ui.button(id!(save_configuration_page)).clicked(actions)
        {
            self.save_document(cx);
        }
        if ui.button(id!(reload_configuration)).clicked(actions) {
            self.reload_document(cx);
        }

        if ui.button(id!(engine_toggle)).clicked(actions) {
            let action = if self.engine_running() {
                AgentAction::StopEngine
            } else {
                AgentAction::StartEngine
            };
            self.request_agent_action(cx, action);
        }
        if ui.button(id!(engine_restart)).clicked(actions) {
            self.request_agent_action(cx, AgentAction::RestartEngine);
        }
        if ui.button(id!(runtime_refresh)).clicked(actions) {
            self.refresh_runtime(cx);
        }
        if ui.button(id!(startup_action)).clicked(actions) {
            let action = if self.autostart_enabled() {
                AgentAction::DisableAutoStart
            } else {
                AgentAction::EnableAutoStart
            };
            self.request_agent_action(cx, action);
        }
        if ui.button(id!(system_dns_action)).clicked(actions) {
            let action = if self.system_dns_enabled() {
                AgentAction::DisableSystemDns
            } else {
                AgentAction::EnableSystemDns
            };
            self.request_agent_action(cx, action);
        }
        if ui.button(id!(legacy_action)).clicked(actions) {
            self.request_agent_action(cx, AgentAction::RemoveLegacyService);
        }

        if let Some(index) = ui.drop_down(id!(language_select)).changed(actions) {
            let language = if index == 0 {
                Language::Chinese
            } else {
                Language::English
            };
            if language != self.state.preferences.language {
                self.state.preferences.language = language;
                self.persist_preferences(cx);
                self.sync_ui(cx, false);
            }
        }
        if let Some(index) = ui.drop_down(id!(appearance_select)).changed(actions) {
            let appearance = if index == 0 {
                AppearanceMode::Dark
            } else {
                AppearanceMode::Light
            };
            if appearance != self.state.preferences.appearance {
                self.state.preferences.appearance = appearance;
                self.persist_preferences(cx);
                self.apply_theme(cx);
                let _ = self
                    .ui
                    .image(id!(app_logo))
                    .load_png_from_data(cx, APP_LOGO);
                self.sync_ui(cx, false);
            }
        }
        if let Some(index) = ui.drop_down(id!(close_behavior_select)).changed(actions) {
            self.state.preferences.close_to_menu_bar = index == 0;
            self.persist_preferences(cx);
            self.sync_ui(cx, false);
        }

        if ui.button(id!(cancel_confirmation)).clicked(actions)
            || ui.modal(id!(confirmation_modal)).dismissed(actions)
        {
            self.state.pending_action = None;
            ui.modal(id!(confirmation_modal)).close(cx);
        }
        if ui.button(id!(confirm_primary)).clicked(actions)
            || ui.button(id!(confirm_danger)).clicked(actions)
        {
            if let Some(action) = self.state.pending_action.take() {
                ui.modal(id!(confirmation_modal)).close(cx);
                self.start_agent_action(cx, action);
            }
        }
    }

    fn handle_signal(&mut self, cx: &mut Cx) {
        while let Ok(event) = self.state.to_ui.try_recv() {
            match event {
                UiEvent::AgentStatus(result) => match result {
                    Ok(status) => self.state.agent_status = Some(status),
                    Err(error) => self.show_notice(cx, error),
                },
                UiEvent::Integration(result) => match result {
                    Ok(status) => self.state.integration = Some(status),
                    Err(error) => self.show_notice(cx, error),
                },
                UiEvent::AgentAction { action, result } => {
                    self.state.busy = false;
                    match result {
                        Ok(response) => {
                            let succeeded = response.ok;
                            self.state.agent_status = Some(response.status);
                            self.show_notice(cx, response.message);
                            if succeeded && action == AgentAction::Quit {
                                cx.quit();
                                return;
                            }
                        }
                        Err(error) => self.show_notice(cx, error),
                    }
                    self.refresh_runtime(cx);
                }
            }
            self.sync_ui(cx, false);
        }
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if self.state.notice_timer.is_timer(event).is_some() {
            self.ui.popup_notification(id!(notice_popup)).close(cx);
        }
    }
}

impl EdgeSteerMakepadApp {
    fn text(&self, chinese: &'static str, english: &'static str) -> &'static str {
        self.state.preferences.language.text(chinese, english)
    }

    fn handle_form_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let ui = self.ui.clone();
        let Some(document) = self.state.document.as_mut() else {
            return;
        };
        let original_draft = document.draft.clone();
        let mut refresh_controls = false;

        if let Some(value) = ui.text_input(id!(listener_address)).changed(actions) {
            document.set_listener_address(value);
        }
        if let Some(value) = ui.text_input(id!(entry_name)).changed(actions) {
            document.set_top_string("entry", value);
        }
        if let Some(value) = ui.text_input(id!(request_timeout)).changed(actions) {
            document.set_top_number("request_timeout_ms", value);
        }
        if let Some(value) = ui.text_input(id!(range_refresh)).changed(actions) {
            document.set_cloudflare_number("range_refresh_secs", value);
        }
        if let Some(value) = ui.check_box(id!(allow_remote)).changed(actions) {
            document.set_listener_allow_remote(value);
        }

        if let Some(index) = ui.drop_down(id!(layer_select)).changed(actions) {
            self.state.selected_layer = index;
            refresh_controls = true;
        }
        if ui.button(id!(layer_add)).clicked(actions) {
            self.state.selected_layer = document.add_layer(LayerKind::Local);
            refresh_controls = true;
        }
        if ui.button(id!(layer_up)).clicked(actions) {
            document.move_layer(self.state.selected_layer, true);
            self.state.selected_layer = self.state.selected_layer.saturating_sub(1);
            refresh_controls = true;
        }
        if ui.button(id!(layer_down)).clicked(actions) {
            document.move_layer(self.state.selected_layer, false);
            self.state.selected_layer = self.state.selected_layer.saturating_add(1);
            refresh_controls = true;
        }
        if ui.button(id!(layer_remove)).clicked(actions) {
            document.remove_layer(self.state.selected_layer);
            self.state.selected_layer = self
                .state
                .selected_layer
                .min(document.layers().len().saturating_sub(1));
            refresh_controls = true;
        }
        if let Some(value) = ui.text_input(id!(layer_tag)).changed(actions) {
            document.set_layer_string(self.state.selected_layer, "tag", value);
        }
        if let Some(index) = ui.drop_down(id!(layer_type)).changed(actions) {
            if let Some(kind) = LayerKind::ALL.get(index).copied() {
                document.set_layer_type(self.state.selected_layer, kind);
                refresh_controls = true;
            }
        }
        if let Some(value) = ui.text_input(id!(layer_next)).changed(actions) {
            document.set_layer_optional_string(self.state.selected_layer, "next", value);
        }
        if let Some(value) = ui.text_input(id!(layer_fallback)).changed(actions) {
            document.set_layer_optional_string(self.state.selected_layer, "fallback", value);
        }
        if let Some(value) = ui.text_input(id!(layer_plugin)).changed(actions) {
            document.set_layer_optional_string(self.state.selected_layer, "plugin", value);
        }
        if let Some(value) = ui.text_input(id!(layer_address)).changed(actions) {
            document.set_layer_string(self.state.selected_layer, "address", value);
        }
        if let Some(value) = ui.text_input(id!(layer_url)).changed(actions) {
            document.set_layer_string(self.state.selected_layer, "url", value);
        }
        if let Some(value) = ui.text_input(id!(layer_server_name)).changed(actions) {
            document.set_layer_string(self.state.selected_layer, "server_name", value);
        }
        if let Some(value) = ui.text_input(id!(layer_timeout)).changed(actions) {
            document.set_layer_number(self.state.selected_layer, "timeout_ms", value);
        }
        if let Some(value) = ui.text_input(id!(layer_refresh)).changed(actions) {
            document.set_layer_number(self.state.selected_layer, "refresh_secs", value);
        }
        if let Some(index) = ui.drop_down(id!(layer_match_mode)).changed(actions) {
            if let Some(mode) = MatchMode::ALL.get(index).copied() {
                document.set_layer_match_mode(self.state.selected_layer, mode);
                refresh_controls = true;
            }
        }
        if let Some(value) = ui.text_input(id!(layer_keywords)).changed(actions) {
            document.set_layer_match_list(self.state.selected_layer, "keywords", value);
        }
        if let Some(value) = ui.text_input(id!(layer_rulesets)).changed(actions) {
            document.set_layer_match_list(self.state.selected_layer, "rule_sets", value);
        }

        if let Some(index) = ui.drop_down(id!(rules_select)).changed(actions) {
            self.state.selected_rule_set = index;
            refresh_controls = true;
        }
        if let Some(index) = ui.drop_down(id!(rules_type)).changed(actions) {
            if let Some(kind) = RuleSetKind::ALL.get(index).copied() {
                document.set_rule_set_type(self.state.selected_rule_set, kind);
                refresh_controls = true;
            }
        }
        if ui.button(id!(rules_add)).clicked(actions) {
            self.state.selected_rule_set = document.add_rule_set(RuleSetKind::Remote);
            refresh_controls = true;
        }
        if ui.button(id!(rules_remove)).clicked(actions) {
            document.remove_rule_set(self.state.selected_rule_set);
            self.state.selected_rule_set = self
                .state
                .selected_rule_set
                .min(document.rule_sets().len().saturating_sub(1));
            refresh_controls = true;
        }
        if let Some(value) = ui.text_input(id!(rules_tag)).changed(actions) {
            document.set_rule_set_string(self.state.selected_rule_set, "tag", value);
        }
        if let Some(value) = ui.text_input(id!(rules_source)).changed(actions) {
            document.set_rule_set_source(self.state.selected_rule_set, value);
        }
        if let Some(value) = ui.text_input(id!(rules_interval)).changed(actions) {
            document.set_rule_set_number(
                self.state.selected_rule_set,
                "update_interval_secs",
                value,
            );
        }
        if let Some(value) = ui.text_input(id!(rules_timeout)).changed(actions) {
            document.set_rule_set_number(self.state.selected_rule_set, "timeout_ms", value);
        }

        if let Some(index) = ui.drop_down(id!(plugin_select)).changed(actions) {
            self.state.selected_plugin = index;
            refresh_controls = true;
        }
        if ui.button(id!(plugin_add)).clicked(actions) {
            self.state.selected_plugin = document.add_plugin();
            refresh_controls = true;
        }
        if ui.button(id!(plugin_remove)).clicked(actions) {
            document.remove_plugin(self.state.selected_plugin);
            self.state.selected_plugin = self
                .state
                .selected_plugin
                .min(document.plugins().len().saturating_sub(1));
            refresh_controls = true;
        }
        if let Some(value) = ui.text_input(id!(plugin_tag)).changed(actions) {
            document.set_plugin_string(self.state.selected_plugin, "tag", value);
        }
        if let Some(value) = ui.text_input(id!(plugin_ttl)).changed(actions) {
            document.set_plugin_number(self.state.selected_plugin, "rewrite_ttl_secs", value);
        }
        if let Some(value) = ui.text_input(id!(plugin_ipv4)).changed(actions) {
            document.set_plugin_preferred(self.state.selected_plugin, "ipv4", value);
        }
        if let Some(value) = ui.text_input(id!(plugin_ipv6)).changed(actions) {
            document.set_plugin_preferred(self.state.selected_plugin, "ipv6", value);
        }
        if let Some(value) = ui.check_box(id!(optimizer_enabled)).changed(actions) {
            document.set_optimizer_bool(self.state.selected_plugin, "enabled", value);
        }
        for (id, key) in [
            (id!(optimizer_interval), "interval_secs"),
            (id!(optimizer_port), "test_port"),
            (id!(optimizer_timeout), "timeout_ms"),
            (id!(optimizer_concurrency), "concurrency"),
            (id!(optimizer_samples), "samples_per_cidr"),
            (id!(optimizer_probes), "probes_per_candidate"),
            (id!(optimizer_max), "max_candidates"),
        ] {
            if let Some(value) = ui.text_input(id).changed(actions) {
                document.set_optimizer_field(self.state.selected_plugin, key, value);
            }
        }
        for (id, key) in [
            (id!(optimizer_host), "test_host"),
            (id!(optimizer_path), "test_path"),
        ] {
            if let Some(value) = ui.text_input(id).changed(actions) {
                document.set_optimizer_field(self.state.selected_plugin, key, value);
            }
        }
        for (id, key) in [
            (id!(optimizer_candidates), "candidates"),
            (id!(optimizer_compatibility), "compatibility_hosts"),
            (id!(optimizer_excluded), "excluded_candidates"),
        ] {
            if let Some(value) = ui.text_input(id).changed(actions) {
                document.set_optimizer_list(self.state.selected_plugin, key, value);
            }
        }

        let draft_changed = document.draft != original_draft;
        if refresh_controls {
            self.sync_structured_controls(cx);
        }
        if draft_changed {
            self.sync_json_editor(cx);
        }
        self.sync_document_status(cx);
    }

    fn document(&self) -> Option<&ConfigDocument> {
        self.state.document.as_ref()
    }

    fn engine_running(&self) -> bool {
        self.state
            .agent_status
            .as_ref()
            .is_some_and(|status| status.engine_running)
    }

    fn autostart_enabled(&self) -> bool {
        self.state
            .agent_status
            .as_ref()
            .is_some_and(|status| status.autostart_enabled)
            || self.state.integration.as_ref().is_some_and(|status| {
                matches!(status.startup_service, StartupService::Registered { .. })
            })
    }

    fn system_dns_enabled(&self) -> bool {
        self.state
            .agent_status
            .as_ref()
            .is_some_and(|status| status.system_dns_enabled)
            || self.state.integration.as_ref().is_some_and(|status| {
                status
                    .dns_services
                    .iter()
                    .any(|service| service.uses_loopback_dns())
            })
    }

    fn apply_theme(&mut self, cx: &mut Cx) {
        let theme = match self.state.preferences.appearance {
            AppearanceMode::Dark => live_id!(theme_desktop_dark),
            AppearanceMode::Light => live_id!(theme_desktop_light),
        };
        cx.link(live_id!(theme), theme);
        cx.reload_ui_dsl();
    }

    fn persist_preferences(&mut self, cx: &mut Cx) {
        if let Err(error) = self.state.preferences.save() {
            self.show_notice(cx, error);
        }
    }

    fn show_notice(&mut self, cx: &mut Cx, message: impl Into<String>) {
        let message = message.into();
        self.ui.label(id!(notice_text)).set_text(cx, &message);
        cx.stop_timer(self.state.notice_timer);
        self.state.notice_timer = cx.start_timeout(NOTICE_DURATION_SECONDS);
        self.ui.popup_notification(id!(notice_popup)).open(cx);
    }

    fn refresh_runtime(&mut self, cx: &mut Cx) {
        let Some(options) = self.state.options.as_ref() else {
            return;
        };
        let agent = options.agent.clone();
        let listener = self
            .document()
            .map(ConfigDocument::listener)
            .unwrap_or_else(|| "127.0.0.1:53".parse().expect("fallback listener is valid"));
        let sender = self.state.to_ui.sender();
        let status_sender = sender.clone();
        if let Err(error) = thread::Builder::new()
            .name("edgesteer-ui-status".to_owned())
            .spawn(move || {
                let _ = status_sender.send(UiEvent::AgentStatus(agent.status()));
            })
        {
            self.show_notice(
                cx,
                format!(
                    "{}: {error}",
                    self.text("无法读取运行状态", "Could not inspect runtime status")
                ),
            );
        }
        if let Err(error) = thread::Builder::new()
            .name("edgesteer-ui-integration".to_owned())
            .spawn(move || {
                let result = integration::inspect(listener).map_err(|error| format!("{error:#}"));
                let _ = sender.send(UiEvent::Integration(result));
            })
        {
            self.show_notice(
                cx,
                format!(
                    "{}: {error}",
                    self.text(
                        "无法读取系统集成状态",
                        "Could not inspect system integration"
                    )
                ),
            );
        }
        self.sync_ui(cx, false);
    }

    fn request_agent_action(&mut self, cx: &mut Cx, action: AgentAction) {
        if self.state.busy {
            self.show_notice(
                cx,
                self.text("已有操作正在执行。", "An operation is already in progress."),
            );
            return;
        }
        if action.requires_valid_configuration()
            && !self.document().is_some_and(ConfigDocument::is_valid)
        {
            self.show_notice(
                cx,
                self.text(
                    "请先修复配置校验错误，再修改 DNS 引擎或系统 DNS。",
                    "Fix configuration validation errors before changing the DNS engine or system DNS.",
                ),
            );
            return;
        }
        if action.requires_confirmation() {
            self.state.pending_action = Some(action);
            self.sync_confirmation(cx, action);
            self.ui.modal(id!(confirmation_modal)).open(cx);
        } else {
            self.start_agent_action(cx, action);
        }
    }

    fn start_agent_action(&mut self, cx: &mut Cx, action: AgentAction) {
        let agent = match self.state.options.as_ref() {
            Some(options) => options.agent.clone(),
            None => {
                self.show_notice(
                    cx,
                    self.text(
                        "EdgeSteer Agent 不可用。",
                        "EdgeSteer Agent is unavailable.",
                    ),
                );
                return;
            }
        };
        self.state.busy = true;
        self.show_notice(
            cx,
            format!(
                "{}...",
                match self.state.preferences.language {
                    Language::Chinese => format!("{}正在执行", action.label(Language::Chinese)),
                    Language::English => format!("{} in progress", action.label(Language::English)),
                }
            ),
        );
        let sender = self.state.to_ui.sender();
        if let Err(error) = thread::Builder::new()
            .name("edgesteer-ui-action".to_owned())
            .spawn(move || {
                let result = agent.request(action.command());
                let _ = sender.send(UiEvent::AgentAction { action, result });
            })
        {
            self.state.busy = false;
            self.show_notice(
                cx,
                format!(
                    "{}: {error}",
                    self.text("无法启动操作", "Could not start operation")
                ),
            );
        }
        self.sync_ui(cx, false);
    }

    fn save_document(&mut self, cx: &mut Cx) {
        let language = self.state.preferences.language;
        let result = self
            .state
            .document
            .as_mut()
            .ok_or_else(|| {
                language
                    .text("没有可保存的配置。", "There is no configuration to save.")
                    .to_owned()
            })
            .and_then(|document| document.save(language));
        match result {
            Ok(()) => {
                self.show_notice(cx, self.text("配置已保存。", "Configuration saved."));
                self.sync_ui(cx, false);
                self.start_agent_action(cx, AgentAction::Refresh);
            }
            Err(error) => self.show_notice(cx, error),
        }
    }

    fn reload_document(&mut self, cx: &mut Cx) {
        let language = self.state.preferences.language;
        let notice = self
            .state
            .document
            .as_mut()
            .and_then(|document| document.reload(language));
        if let Some(document) = self.document() {
            self.ui
                .text_input(id!(config_editor))
                .set_text(cx, &document.draft);
        }
        self.sync_structured_controls(cx);
        self.sync_ui(cx, false);
        self.refresh_runtime(cx);
        self.show_notice(
            cx,
            notice.unwrap_or_else(|| {
                self.text("配置已重新加载。", "Configuration reloaded.")
                    .to_owned()
            }),
        );
    }

    fn sync_confirmation(&mut self, cx: &mut Cx, action: AgentAction) {
        let language = self.state.preferences.language;
        self.ui.label(id!(confirmation_title)).set_text(
            cx,
            &format!(
                "{}: {}",
                language.text("确认", "Confirm"),
                action.label(language)
            ),
        );
        self.ui
            .label(id!(confirmation_detail))
            .set_text(cx, action.confirmation(language));
        self.ui
            .button(id!(cancel_confirmation))
            .set_text(cx, language.text("取消", "Cancel"));
        let confirm_label = if action == AgentAction::Quit {
            action.label(language)
        } else {
            language.text("继续", "Continue")
        };
        self.ui
            .button(id!(confirm_primary))
            .set_text(cx, confirm_label);
        self.ui
            .button(id!(confirm_danger))
            .set_text(cx, confirm_label);
        self.ui
            .button(id!(confirm_primary))
            .set_visible(cx, !action.is_destructive());
        self.ui
            .button(id!(confirm_danger))
            .set_visible(cx, action.is_destructive());
    }

    fn sync_page(&mut self, cx: &mut Cx) {
        self.ui
            .view(id!(overview_page))
            .set_visible(cx, self.state.page == Page::Overview);
        self.ui
            .view(id!(resolver_page))
            .set_visible(cx, self.state.page == Page::Resolver);
        self.ui
            .view(id!(rules_page))
            .set_visible(cx, self.state.page == Page::RuleSets);
        self.ui
            .view(id!(cloudflare_page))
            .set_visible(cx, self.state.page == Page::Cloudflare);
        self.ui
            .view(id!(json_page))
            .set_visible(cx, self.state.page == Page::Json);
        self.ui
            .view(id!(system_page))
            .set_visible(cx, self.state.page == Page::System);
    }

    fn sync_document_status(&mut self, cx: &mut Cx) {
        let language = self.state.preferences.language;
        let (state, valid, dirty, summary) = if let Some(document) = self.document() {
            (
                document.validation_summary(language),
                document.is_valid(),
                document.is_dirty(),
                document.resolver_summary(language),
            )
        } else {
            (
                language.text("正在加载", "Loading").to_owned(),
                false,
                false,
                language
                    .text("正在读取配置。", "Reading configuration.")
                    .to_owned(),
            )
        };
        self.ui.label(id!(document_state)).set_text(cx, &state);
        self.ui.label(id!(config_validation)).set_text(cx, &state);
        self.ui.label(id!(resolver_detail)).set_text(cx, &summary);
        self.ui
            .button(id!(save_configuration))
            .set_enabled(cx, valid && dirty && !self.state.busy);
        self.ui
            .button(id!(save_configuration_page))
            .set_enabled(cx, valid && dirty && !self.state.busy);
    }

    fn sync_structured_controls(&mut self, cx: &mut Cx) {
        let language = self.state.preferences.language;
        let Some(document) = self.document() else {
            return;
        };

        let layer_labels = document
            .layers()
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let object = layer.as_object();
                format!(
                    "{:02}  {} ({})",
                    index + 1,
                    object
                        .and_then(|value| object_string(value, "tag"))
                        .unwrap_or("layer"),
                    object
                        .map(|value| LayerKind::from_value(object_string(value, "type"))
                            .label(language))
                        .unwrap_or("?")
                )
            })
            .collect::<Vec<_>>();
        self.ui
            .drop_down(id!(layer_select))
            .set_labels(cx, layer_labels);
        self.ui.drop_down(id!(layer_select)).set_selected_item(
            cx,
            self.state
                .selected_layer
                .min(document.layers().len().saturating_sub(1)),
        );
        self.ui.drop_down(id!(layer_type)).set_labels(
            cx,
            LayerKind::ALL
                .iter()
                .map(|kind| kind.label(language).to_owned())
                .collect(),
        );
        self.ui.drop_down(id!(layer_match_mode)).set_labels(
            cx,
            MatchMode::ALL
                .iter()
                .map(|mode| mode.label(language).to_owned())
                .collect(),
        );

        let layer = document.layer(self.state.selected_layer);
        set_text_input(
            &self.ui,
            cx,
            id!(listener_address),
            document.listener().to_string(),
        );
        let root_entry = document
            .value
            .get("entry")
            .and_then(Value::as_str)
            .unwrap_or("");
        set_text_input(&self.ui, cx, id!(entry_name), root_entry.to_owned());
        set_text_input(
            &self.ui,
            cx,
            id!(request_timeout),
            document
                .value
                .get("request_timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(8000)
                .to_string(),
        );
        set_text_input(
            &self.ui,
            cx,
            id!(range_refresh),
            document
                .value
                .get("cloudflare")
                .and_then(Value::as_object)
                .map(|v| object_number(v, "range_refresh_secs", 86400))
                .unwrap_or(86400)
                .to_string(),
        );
        self.ui.check_box(id!(allow_remote)).set_active(
            cx,
            document
                .value
                .get("listener")
                .and_then(Value::as_object)
                .map(|v| object_bool(v, "allow_remote", false))
                .unwrap_or(false),
        );
        sync_layer_fields(&self.ui, cx, layer);

        let rule_labels = document
            .rule_sets()
            .iter()
            .enumerate()
            .map(|(index, value)| {
                format!(
                    "{:02}  {}",
                    index + 1,
                    value
                        .as_object()
                        .and_then(|v| object_string(v, "tag"))
                        .unwrap_or("rule-set")
                )
            })
            .collect();
        self.ui
            .drop_down(id!(rules_select))
            .set_labels(cx, rule_labels);
        self.ui.drop_down(id!(rules_select)).set_selected_item(
            cx,
            self.state
                .selected_rule_set
                .min(document.rule_sets().len().saturating_sub(1)),
        );
        self.ui.drop_down(id!(rules_type)).set_labels(
            cx,
            RuleSetKind::ALL
                .iter()
                .map(|kind| kind.label(language).to_owned())
                .collect(),
        );
        sync_rule_fields(
            &self.ui,
            cx,
            document.rule_set(self.state.selected_rule_set),
        );

        let plugin_labels = document
            .plugins()
            .iter()
            .enumerate()
            .map(|(index, value)| {
                format!(
                    "{:02}  {}",
                    index + 1,
                    value
                        .as_object()
                        .and_then(|v| object_string(v, "tag"))
                        .unwrap_or("plugin")
                )
            })
            .collect();
        self.ui
            .drop_down(id!(plugin_select))
            .set_labels(cx, plugin_labels);
        self.ui.drop_down(id!(plugin_select)).set_selected_item(
            cx,
            self.state
                .selected_plugin
                .min(document.plugins().len().saturating_sub(1)),
        );
        sync_plugin_fields(&self.ui, cx, document.plugin(self.state.selected_plugin));
    }

    fn sync_json_editor(&mut self, cx: &mut Cx) {
        if let Some(document) = self.document() {
            self.ui
                .text_input(id!(config_editor))
                .set_text(cx, &document.draft);
        }
    }

    fn sync_form_labels(&mut self, cx: &mut Cx) {
        let labels: &[(&[LiveId], &'static str, &'static str)] = &[
            (id!(listener_address_label), "监听地址", "Listener address"),
            (id!(entry_name_label), "入口层", "Entry layer"),
            (
                id!(request_timeout_label),
                "请求超时（毫秒）",
                "Request timeout (ms)",
            ),
            (
                id!(range_refresh_label),
                "CF 号段刷新（秒）",
                "Cloudflare range refresh (s)",
            ),
            (id!(layer_tag_label), "层标签", "Layer tag"),
            (id!(layer_type_label), "层类型", "Layer type"),
            (
                id!(layer_next_label),
                "下一层（未命中时）",
                "Next layer on filter miss",
            ),
            (
                id!(layer_fallback_label),
                "回退层（请求失败时）",
                "Fallback layer on failure",
            ),
            (id!(layer_plugin_label), "响应插件", "Response plugin"),
            (id!(layer_address_label), "上游地址", "Upstream address"),
            (id!(layer_url_label), "DoH URL", "DoH URL"),
            (
                id!(layer_server_name_label),
                "DoT 服务名",
                "DoT server name",
            ),
            (
                id!(layer_timeout_label),
                "层超时（毫秒）",
                "Layer timeout (ms)",
            ),
            (
                id!(layer_refresh_label),
                "本地 DNS 刷新（秒）",
                "Local DNS refresh (s)",
            ),
            (id!(layer_match_mode_label), "匹配模式", "Match mode"),
            (
                id!(layer_keywords_label),
                "关键词（逗号分隔）",
                "Keywords (comma separated)",
            ),
            (
                id!(layer_rulesets_label),
                "规则集标签（逗号分隔）",
                "Rule-set tags (comma separated)",
            ),
            (id!(rules_tag_label), "规则集标签", "Rule-set tag"),
            (id!(rules_source_label), "规则集来源", "Rule-set source"),
            (
                id!(rules_interval_label),
                "刷新间隔（秒）",
                "Refresh interval (s)",
            ),
            (
                id!(rules_timeout_label),
                "下载超时（毫秒）",
                "Download timeout (ms)",
            ),
            (id!(plugin_tag_label), "插件标签", "Plugin tag"),
            (id!(plugin_ttl_label), "重写 TTL（秒）", "Rewrite TTL (s)"),
            (
                id!(plugin_ipv4_label),
                "固定优选 IPv4（可选）",
                "Preferred IPv4 (optional)",
            ),
            (
                id!(plugin_ipv6_label),
                "固定优选 IPv6（可选）",
                "Preferred IPv6 (optional)",
            ),
            (
                id!(optimizer_interval_label),
                "优选间隔（秒）",
                "Probe interval (s)",
            ),
            (id!(optimizer_host_label), "探测主机", "Probe host"),
            (id!(optimizer_path_label), "探测路径", "Probe path"),
            (id!(optimizer_port_label), "探测端口", "Probe port"),
            (
                id!(optimizer_timeout_label),
                "探测超时（毫秒）",
                "Probe timeout (ms)",
            ),
            (id!(optimizer_concurrency_label), "并发数", "Concurrency"),
            (
                id!(optimizer_samples_label),
                "每个 CIDR 采样数",
                "Samples per CIDR",
            ),
            (
                id!(optimizer_probes_label),
                "每个候选探测数",
                "Probes per candidate",
            ),
            (id!(optimizer_max_label), "最大候选数", "Maximum candidates"),
            (
                id!(optimizer_candidates_label),
                "候选 IP/CIDR（逗号分隔）",
                "Candidate IPs/CIDRs (comma separated)",
            ),
            (
                id!(optimizer_compatibility_label),
                "兼容性主机（逗号分隔）",
                "Compatibility hosts (comma separated)",
            ),
            (
                id!(optimizer_excluded_label),
                "排除 IP/CIDR（逗号分隔）",
                "Excluded IPs/CIDRs (comma separated)",
            ),
        ];
        for (id, chinese, english) in labels {
            self.ui.label(id).set_text(cx, self.text(chinese, english));
        }
        self.ui
            .check_box(id!(optimizer_enabled))
            .set_text(self.text("启用定时优选", "Enable scheduled probing"));
    }

    fn sync_ui(&mut self, cx: &mut Cx, reset_editor: bool) {
        self.sync_page(cx);
        self.sync_document_status(cx);

        let language = self.state.preferences.language;
        self.sync_form_labels(cx);
        let config_path = self
            .document()
            .map(|document| document.path.display().to_string())
            .unwrap_or_else(|| "~/edgesteer.json".to_owned());
        let valid = self.document().is_some_and(ConfigDocument::is_valid);
        let listener = self
            .document()
            .map(ConfigDocument::listener)
            .map(|listener| listener.to_string())
            .unwrap_or_else(|| language.text("正在检查", "Checking").to_owned());
        let config_error = self
            .state
            .agent_status
            .as_ref()
            .and_then(|status| status.configuration_error.clone());
        let integration_ready = self
            .state
            .integration
            .as_ref()
            .is_some_and(|status| status.listener_ready);

        self.ui
            .button(id!(nav_overview))
            .set_text(cx, language.text("状态", "Status"));
        self.ui
            .button(id!(nav_resolver))
            .set_text(cx, language.text("解析层", "Resolvers"));
        self.ui
            .button(id!(nav_rules))
            .set_text(cx, language.text("规则集", "Rule sets"));
        self.ui
            .button(id!(nav_cloudflare))
            .set_text(cx, language.text("CF 优选", "CF preferred"));
        self.ui
            .button(id!(nav_json))
            .set_text(cx, language.text("高级 JSON", "Advanced JSON"));
        self.ui
            .button(id!(nav_system))
            .set_text(cx, language.text("系统", "System"));
        self.ui
            .button(id!(save_configuration))
            .set_text(cx, language.text("保存配置", "Save configuration"));

        self.ui
            .label(id!(overview_title))
            .set_text(cx, language.text("运行状态", "Runtime status"));
        self.ui.label(id!(overview_copy)).set_text(
            cx,
            language.text(
                "EdgeSteer 在菜单栏持续运行；此窗口用于查看状态和修改配置。",
                "EdgeSteer keeps running from the menu bar; this window shows status and edits configuration.",
            ),
        );
        self.ui
            .label(id!(engine_metric_title))
            .set_text(cx, language.text("DNS 引擎", "DNS engine"));
        self.ui
            .label(id!(listener_metric_title))
            .set_text(cx, language.text("DNS 监听器", "DNS listener"));
        self.ui
            .label(id!(dns_metric_title))
            .set_text(cx, language.text("系统 DNS", "System DNS"));
        self.ui.label(id!(engine_metric_value)).set_text(
            cx,
            if self.engine_running() {
                language.text("运行中", "Running")
            } else {
                language.text("已停止", "Stopped")
            },
        );
        self.ui
            .label(id!(listener_metric_value))
            .set_text(cx, &listener);
        self.ui.label(id!(dns_metric_value)).set_text(
            cx,
            if self.system_dns_enabled() {
                language.text("已接管", "Managed")
            } else {
                language.text("未接管", "Not managed")
            },
        );
        self.ui
            .label(id!(runtime_heading))
            .set_text(cx, language.text("DNS 引擎", "DNS engine"));
        let runtime_detail = config_error.unwrap_or_else(|| {
            if self.state.busy {
                language
                    .text("正在处理运行操作。", "A runtime operation is in progress.")
                    .to_owned()
            } else {
                language
                    .text(
                        "运行操作通过常驻菜单栏 Agent 执行。",
                        "Runtime operations are handled by the resident menu-bar Agent.",
                    )
                    .to_owned()
            }
        });
        self.ui
            .label(id!(runtime_detail))
            .set_text(cx, &runtime_detail);
        self.ui.button(id!(engine_toggle)).set_text(
            cx,
            if self.engine_running() {
                AgentAction::StopEngine.label(language)
            } else {
                AgentAction::StartEngine.label(language)
            },
        );
        self.ui
            .button(id!(engine_restart))
            .set_text(cx, AgentAction::RestartEngine.label(language));
        self.ui
            .button(id!(runtime_refresh))
            .set_text(cx, language.text("刷新", "Refresh"));
        self.ui
            .button(id!(engine_toggle))
            .set_enabled(cx, !self.state.busy && (self.engine_running() || valid));
        self.ui
            .button(id!(engine_restart))
            .set_enabled(cx, !self.state.busy && valid);
        self.ui
            .button(id!(runtime_refresh))
            .set_enabled(cx, !self.state.busy);
        self.ui
            .label(id!(resolver_heading))
            .set_text(cx, language.text("解析链", "Resolver chain"));

        self.ui
            .label(id!(resolver_title))
            .set_text(cx, language.text("解析层", "Resolver layers"));
        self.ui.label(id!(resolver_copy)).set_text(
            cx,
            language.text(
                "按入口顺序编辑动态本地 DNS、DoH、DoT 和 TCP/UDP 层；每层可配置匹配、下一层与故障回退。",
                "Edit dynamic local DNS, DoH, DoT, TCP, and UDP layers with matching, next, and failure fallback.",
            ),
        );
        self.ui
            .label(id!(rules_title))
            .set_text(cx, language.text("规则集", "Rule sets"));
        self.ui.label(id!(rules_copy)).set_text(
            cx,
            language.text(
                "管理本地或远程 SRS 规则集，并将标签绑定到解析层匹配。",
                "Manage local or remote SRS sources used by resolver matching.",
            ),
        );
        self.ui
            .label(id!(cloudflare_title))
            .set_text(cx, language.text("CF 优选", "CF preferred"));
        self.ui.label(id!(cloudflare_copy)).set_text(
            cx,
            language.text(
                "配置 Cloudflare 响应重写、固定优选地址和稳定优选探测范围。",
                "Configure Cloudflare response rewriting, preferred addresses, and stable probing.",
            ),
        );
        self.ui
            .label(id!(json_title))
            .set_text(cx, language.text("高级 JSON", "Advanced JSON"));
        self.ui.label(id!(json_copy)).set_text(cx, language.text("直接编辑完整配置。表单页和这里共享同一份严格校验，适合批量修改未暴露字段。", "Edit the complete configuration directly. The form pages and this editor share strict validation."));
        self.ui
            .label(id!(config_path_label))
            .set_text(cx, language.text("配置文件", "Configuration file"));
        self.ui
            .label(id!(config_path_value))
            .set_text(cx, &config_path);
        self.ui
            .button(id!(reload_configuration))
            .set_text(cx, language.text("重新加载", "Reload"));
        self.ui
            .button(id!(save_configuration_page))
            .set_text(cx, language.text("保存配置", "Save configuration"));
        if reset_editor {
            self.sync_json_editor(cx);
            self.sync_structured_controls(cx);
        }

        self.ui
            .label(id!(system_title))
            .set_text(cx, language.text("系统", "System"));
        self.ui.label(id!(system_copy)).set_text(
            cx,
            language.text(
                "菜单栏是主控制面；关闭此窗口只会释放图形界面，不会停止 DNS 服务。",
                "The menu bar is the primary control surface. Closing this window releases only the GUI and keeps DNS running.",
            ),
        );
        self.ui
            .label(id!(appearance_heading))
            .set_text(cx, language.text("应用", "Application"));
        self.ui.label(id!(appearance_copy)).set_text(
            cx,
            language.text(
                "选择界面语言、黑白风格与关闭窗口后的行为。",
                "Choose the UI language, black/white appearance, and what closing the window does.",
            ),
        );
        self.ui
            .drop_down(id!(language_select))
            .set_labels(cx, vec!["简体中文".to_owned(), "English".to_owned()]);
        self.ui
            .drop_down(id!(language_select))
            .set_selected_item(cx, if language == Language::Chinese { 0 } else { 1 });
        self.ui.drop_down(id!(appearance_select)).set_labels(
            cx,
            vec![
                language.text("深色", "Dark").to_owned(),
                language.text("浅色", "Light").to_owned(),
            ],
        );
        self.ui.drop_down(id!(appearance_select)).set_selected_item(
            cx,
            if self.state.preferences.appearance == AppearanceMode::Dark {
                0
            } else {
                1
            },
        );
        self.ui.drop_down(id!(close_behavior_select)).set_labels(
            cx,
            vec![
                language
                    .text("关闭窗口仍保持运行", "Keep running after window closes")
                    .to_owned(),
                language
                    .text(
                        "关闭窗口时退出 EdgeSteer",
                        "Quit EdgeSteer when the window closes",
                    )
                    .to_owned(),
            ],
        );
        self.ui
            .drop_down(id!(close_behavior_select))
            .set_selected_item(
                cx,
                if self.state.preferences.close_to_menu_bar {
                    0
                } else {
                    1
                },
            );

        self.ui
            .label(id!(startup_heading))
            .set_text(cx, language.text("登录启动", "Open at login"));
        self.ui
            .label(id!(startup_detail))
            .set_text(cx, &self.startup_detail(language));
        self.ui.button(id!(startup_action)).set_text(
            cx,
            if self.autostart_enabled() {
                AgentAction::DisableAutoStart.label(language)
            } else {
                AgentAction::EnableAutoStart.label(language)
            },
        );
        let autostart_allowed =
            !self.state.busy && (self.autostart_enabled() || (valid && self.has_app_bundle()));
        self.ui
            .button(id!(startup_action))
            .set_enabled(cx, autostart_allowed);

        self.ui
            .label(id!(system_dns_heading))
            .set_text(cx, language.text("系统 DNS", "System DNS"));
        let (system_dns_detail, system_dns_ready) =
            self.system_dns_detail(language, valid, integration_ready);
        self.ui
            .label(id!(system_dns_detail))
            .set_text(cx, &system_dns_detail);
        self.ui.button(id!(system_dns_action)).set_text(
            cx,
            if self.system_dns_enabled() {
                AgentAction::DisableSystemDns.label(language)
            } else {
                AgentAction::EnableSystemDns.label(language)
            },
        );
        self.ui.button(id!(system_dns_action)).set_enabled(
            cx,
            !self.state.busy && (self.system_dns_enabled() || system_dns_ready),
        );

        let legacy_detected = self.state.integration.as_ref().is_some_and(|status| {
            matches!(status.startup_service, StartupService::LegacyDaemon { .. })
        });
        self.ui
            .view(id!(legacy_panel))
            .set_visible(cx, legacy_detected);
        self.ui
            .label(id!(legacy_heading))
            .set_text(cx, language.text("旧版服务", "Legacy service"));
        self.ui.label(id!(legacy_detail)).set_text(
            cx,
            language.text(
                "检测到旧版命令行服务；仅在确认后移除。",
                "A legacy command-line service was detected; it is removed only after confirmation.",
            ),
        );
        self.ui
            .button(id!(legacy_action))
            .set_text(cx, AgentAction::RemoveLegacyService.label(language));
        self.ui
            .button(id!(legacy_action))
            .set_enabled(cx, !self.state.busy && legacy_detected);

        self.ui
            .label(id!(services_heading))
            .set_text(cx, language.text("网络服务", "Network services"));
        self.ui
            .label(id!(services_detail))
            .set_text(cx, &self.services_detail(language));
    }

    fn has_app_bundle(&self) -> bool {
        self.state
            .options
            .as_ref()
            .and_then(|options| options.app_bundle.as_ref())
            .is_some()
    }

    fn startup_detail(&self, language: Language) -> String {
        let Some(integration) = &self.state.integration else {
            return language
                .text("正在检查登录启动项。", "Checking open-at-login status.")
                .to_owned();
        };
        match &integration.startup_service {
            StartupService::Registered { .. } => language
                .text(
                    "已启用：登录时会打开已安装的 EdgeSteer App。",
                    "Enabled: the installed EdgeSteer App opens at login.",
                )
                .to_owned(),
            StartupService::LegacyDaemon { .. } => language
                .text(
                    "检测到旧版命令行服务，可在下方移除。",
                    "A legacy command-line service was detected and can be removed below.",
                )
                .to_owned(),
            StartupService::NotRegistered => language
                .text("当前未启用登录启动。", "Open at login is disabled.")
                .to_owned(),
            StartupService::Unsupported { reason } => {
                format!("{}: {reason}", language.text("当前平台", "This platform"))
            }
        }
    }

    fn system_dns_detail(
        &self,
        language: Language,
        valid: bool,
        integration_ready: bool,
    ) -> (String, bool) {
        if self.system_dns_enabled() {
            return (
                language
                    .text(
                        "EdgeSteer 正在接管它记录的物理网络服务；解除时会恢复自动 DNS。",
                        "EdgeSteer manages its recorded physical network services and restores automatic DNS when disabled.",
                    )
                    .to_owned(),
                true,
            );
        }
        if !valid {
            return (
                language
                    .text(
                        "请先修复配置校验错误。",
                        "Fix configuration validation errors first.",
                    )
                    .to_owned(),
                false,
            );
        }
        let Some(document) = self.document() else {
            return (
                language
                    .text("正在读取配置。", "Reading configuration.")
                    .to_owned(),
                false,
            );
        };
        let listener = document.listener();
        if listener.port() != 53 || !listener.ip().is_loopback() {
            return (
                language
                    .text(
                        "系统 DNS 需要回环地址的 53 端口监听器。",
                        "System DNS requires a loopback listener on port 53.",
                    )
                    .to_owned(),
                false,
            );
        }
        if !self.engine_running() {
            return (
                language
                    .text("请先启动 DNS 引擎。", "Start the DNS engine first.")
                    .to_owned(),
                false,
            );
        }
        if !integration_ready {
            return (
                language
                    .text(
                        "正在等待 DNS 监听器就绪。",
                        "Waiting for the DNS listener to become ready.",
                    )
                    .to_owned(),
                false,
            );
        }
        (
            language
                .text(
                    "监听器已就绪，可以接管系统 DNS。",
                    "The listener is ready for system DNS.",
                )
                .to_owned(),
            true,
        )
    }

    fn services_detail(&self, language: Language) -> String {
        let Some(integration) = &self.state.integration else {
            return language
                .text("正在读取网络服务。", "Reading network services.")
                .to_owned();
        };
        if integration.dns_services.is_empty() {
            return language
                .text(
                    "当前平台未提供可接管的物理网络服务。",
                    "This platform does not report physical network services that EdgeSteer can manage.",
                )
                .to_owned();
        }
        integration
            .dns_services
            .iter()
            .map(|service| {
                format!(
                    "{} ({}) | {} | {}",
                    service.name,
                    service.device,
                    if service.enabled {
                        language.text("已启用", "Enabled")
                    } else {
                        language.text("已停用", "Disabled")
                    },
                    service.dns_description()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl AppMain for EdgeSteerMakepadApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::WindowCloseRequested(close_request) = event {
            if !self.state.preferences.close_to_menu_bar {
                close_request.accept_close.set(false);
                self.request_agent_action(cx, AgentAction::Quit);
                return;
            }
        }
        if matches!(event, Event::WindowClosed(_)) {
            cx.quit();
            return;
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> ConfigDocument {
        ConfigDocument::from_contents(
            PathBuf::from("/tmp/edgesteer-ui-test.json"),
            DEFAULT_CONFIG.to_owned(),
        )
    }

    #[test]
    fn structured_layer_edit_uses_the_same_validation_boundary() {
        let mut document = document();
        let index = document.add_layer(LayerKind::Local);
        document.set_layer_type(index, LayerKind::Doh);
        document.set_layer_string(index, "address", "1.1.1.1:443".to_owned());
        document.set_layer_string(
            index,
            "url",
            "https://cloudflare-dns.com/dns-query".to_owned(),
        );

        assert!(document.is_valid());
        assert_eq!(
            document
                .layer(index)
                .and_then(|layer| object_string(layer, "type")),
            Some("doh")
        );
    }

    #[test]
    fn structured_rule_set_and_plugin_edits_round_trip() {
        let mut document = document();
        let rule_set = document.add_rule_set(RuleSetKind::Local);
        document.set_rule_set_string(rule_set, "tag", "local-test".to_owned());
        let plugin = document.add_plugin();
        document.set_plugin_preferred(plugin, "ipv4", "104.16.0.1".to_owned());
        document.set_optimizer_bool(plugin, "enabled", false);

        assert!(document.is_valid());
        assert_eq!(document.rule_sets().len(), 2);
        assert_eq!(document.plugins().len(), 2);
        assert_eq!(
            document
                .plugin(plugin)
                .and_then(|plugin| plugin.get("preferred"))
                .and_then(Value::as_object)
                .and_then(|preferred| object_string(preferred, "ipv4")),
            Some("104.16.0.1")
        );
    }

    #[test]
    fn comma_separated_fields_are_normalized_without_duplicates() {
        assert_eq!(
            string_list_value(" mi, local, mi, LOCAL "),
            json!(["mi", "local"])
        );
    }
}
