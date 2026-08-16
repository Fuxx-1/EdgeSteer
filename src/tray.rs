use std::io::Cursor;

use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const OPEN_ID: &str = "edgesteer-open";
const ENGINE_ID: &str = "edgesteer-engine";
const SYSTEM_DNS_ID: &str = "edgesteer-system-dns";
const AUTOSTART_ID: &str = "edgesteer-autostart";
const QUIT_ID: &str = "edgesteer-quit";

const LOGO: &[u8] = include_bytes!("../assets/edgesteer-logo.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    ToggleEngine,
    ToggleSystemDns,
    ToggleAutostart,
    Quit,
}

#[derive(Debug, Clone)]
pub struct Labels {
    pub open: String,
    pub dns: String,
    pub running: String,
    pub stopped: String,
    pub start_engine: String,
    pub stop_engine: String,
    pub system_dns: String,
    pub open_at_login: String,
    pub quit: String,
}

#[derive(Debug, Clone)]
pub struct Presentation {
    pub labels: Labels,
    pub listener: String,
    pub engine_running: bool,
    pub engine_action_enabled: bool,
    pub system_dns_enabled: bool,
    pub system_dns_action_enabled: bool,
    pub autostart_enabled: bool,
    pub autostart_action_enabled: bool,
}

pub struct TrayController {
    _tray: TrayIcon,
    open: MenuItem,
    status: MenuItem,
    engine: MenuItem,
    system_dns: CheckMenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

impl TrayController {
    pub fn new(presentation: &Presentation) -> Result<Self, String> {
        let open = MenuItem::with_id(OPEN_ID, &presentation.labels.open, true, None);
        let status = MenuItem::new("", false, None);
        let engine = MenuItem::with_id(ENGINE_ID, "", true, None);
        let system_dns = CheckMenuItem::with_id(
            SYSTEM_DNS_ID,
            &presentation.labels.system_dns,
            true,
            false,
            None,
        );
        let autostart = CheckMenuItem::with_id(
            AUTOSTART_ID,
            &presentation.labels.open_at_login,
            true,
            false,
            None,
        );
        let quit = MenuItem::with_id(QUIT_ID, &presentation.labels.quit, true, None);
        let separator_one = PredefinedMenuItem::separator();
        let separator_two = PredefinedMenuItem::separator();

        let menu = Menu::new();
        menu.append_items(&[
            &open,
            &status,
            &separator_one,
            &engine,
            &system_dns,
            &autostart,
            &separator_two,
            &quit,
        ])
        .map_err(|error| format!("build menu bar menu: {error}"))?;

        let tray = TrayIconBuilder::new()
            .with_id("edgesteer")
            .with_tooltip("EdgeSteer")
            .with_icon(app_icon()?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_menu_on_right_click(true)
            .build()
            .map_err(|error| format!("create menu bar icon: {error}"))?;

        let mut controller = Self {
            _tray: tray,
            open,
            status,
            engine,
            system_dns,
            autostart,
            quit,
        };
        controller.sync(presentation);
        Ok(controller)
    }

    pub fn sync(&mut self, presentation: &Presentation) {
        self.open.set_text(&presentation.labels.open);
        self.status.set_text(format!(
            "{}: {} - {}",
            presentation.labels.dns,
            if presentation.engine_running {
                &presentation.labels.running
            } else {
                &presentation.labels.stopped
            },
            presentation.listener
        ));
        self.engine.set_text(if presentation.engine_running {
            &presentation.labels.stop_engine
        } else {
            &presentation.labels.start_engine
        });
        self.engine.set_enabled(presentation.engine_action_enabled);

        self.system_dns.set_text(&presentation.labels.system_dns);
        self.system_dns.set_checked(presentation.system_dns_enabled);
        self.system_dns
            .set_enabled(presentation.system_dns_action_enabled);

        self.autostart.set_text(&presentation.labels.open_at_login);
        self.autostart.set_checked(presentation.autostart_enabled);
        self.autostart
            .set_enabled(presentation.autostart_action_enabled);

        self.quit.set_text(&presentation.labels.quit);
    }

    pub fn next_event(&self) -> Option<TrayEvent> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let event = match event.id.as_ref() {
                OPEN_ID => TrayEvent::Open,
                ENGINE_ID => TrayEvent::ToggleEngine,
                SYSTEM_DNS_ID => TrayEvent::ToggleSystemDns,
                AUTOSTART_ID => TrayEvent::ToggleAutostart,
                QUIT_ID => TrayEvent::Quit,
                _ => continue,
            };
            return Some(event);
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                return Some(TrayEvent::Open);
            }
        }

        None
    }
}

fn app_icon() -> Result<Icon, String> {
    let mut decoder = png::Decoder::new(Cursor::new(LOGO));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("read bundled logo: {error}"))?;
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| "bundled logo has no decoded frame size".to_owned())?;
    let mut pixels = vec![0; output_size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| format!("decode bundled logo: {error}"))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("bundled logo must be an 8-bit RGBA PNG".to_owned());
    }
    pixels.truncate(info.buffer_size());
    Icon::from_rgba(pixels, info.width, info.height)
        .map_err(|error| format!("load bundled logo for menu bar: {error}"))
}

#[cfg(test)]
mod tests {
    use super::app_icon;

    #[test]
    fn bundled_logo_decodes_to_a_tray_icon() {
        assert!(app_icon().is_ok());
    }
}
