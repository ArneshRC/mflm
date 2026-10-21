#![deny(rust_2018_idioms)]

use std::{fs, fs::OpenOptions, io, path::Path};

use chrono::Local;
use framebuffer::{Framebuffer, KdMode, VarScreeninfo};
use freedesktop_desktop_entry::DesktopEntry;
use log::{debug, error, info, warn};
use simplelog::{ConfigBuilder as LogConfigBuilder, LevelFilter, WriteLogger};
use termion::raw::IntoRawMode;
use thiserror::Error;

const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

// from linux/fb.h
const FB_ACTIVATE_NOW: u32 = 0;
const FB_ACTIVATE_FORCE: u32 = 128;

mod buffer;
mod color;
mod draw;
mod field_icon;
mod greetd;
mod greeter_loop;
mod layout;
mod power;
mod settings;
mod template;
mod vt;

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    SelectingSession,
    EditingUsername,
    EditingPassword
}

#[derive(Error, Debug)]
#[non_exhaustive]
enum Error {
    #[error("Error performing buffer operation: {0}")]
    Buffer(#[from] buffer::BufferError),
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error)
}

#[derive(Clone, Debug)]
enum Status {
    Info(String),
    Error(String)
}

struct Target {
    name: String,
    exec: Vec<String>
}

impl Target {
    fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        let path = path.as_ref();
        let data = match fs::read_to_string(path) {
            Ok(data) => data,
            Err(e) => {
                debug!("Skipping target at {:?}: failed to read desktop entry: {e}", path);
                return None;
            }
        };

        let entry = match DesktopEntry::decode(path, &data) {
            Ok(entry) => entry,
            Err(e) => {
                debug!("Skipping target at {:?}: failed to parse desktop entry: {e}", path);
                return None;
            }
        };

        let cmdline = match entry.exec() {
            Some(cmdline) => cmdline,
            None => {
                debug!("Skipping target at {:?}: missing Exec=", path);
                return None;
            }
        };

        let exec = match shell_words::split(cmdline) {
            Ok(exec) => exec,
            Err(e) => {
                debug!(
                    "Skipping target at {:?}: failed to parse Exec command line ({cmdline:?}): {e}",
                    path
                );
                return None;
            }
        };

        let name = entry.name(None).unwrap_or(entry.appid.into()).into_owned();

        Some(Self { name, exec })
    }
}

struct LoginManager<'a> {
    buf: &'a mut [u8],
    device: &'a fs::File,

    heading_font: draw::Font,
    main_font: draw::Font,
    icon_font: draw::Font,
    field_icons: field_icon::FieldIcons,

    colors: settings::ResolvedColors,
    header: template::Template,

    power_font: draw::Font,
    power_items: Vec<power::Item>,
    power_item_h: u32,
    power_gap: u32,
    power_offset: u32,
    // index into power_items of the action awaiting confirmation
    power_armed: Option<usize>,
    power_hint_font: draw::Font,
    power_hint_size: (u32, u32),
    power_hint_offset: u32,

    // None when [status] enabled = false
    status_font: Option<draw::Font>,
    status_h: u32,
    status_offset: u32,
    status: Option<Status>,

    forced_username: Option<String>,
    lock_target: bool,
    lock_username: bool,
    session_gap_below: u32,
    username_gap_below: u32,
    row_h: u32,
    password_mask: String,
    text_align: settings::TextAlign,
    form_padding: u32,
    header_offset: u32,
    clear_screen_on_success: bool,
    session_left_arrow: String,
    session_right_arrow: String,

    screen_size: (u32, u32),
    form_width: u32,
    mode: Mode,
    greetd: greetd::GreetD,
    targets: Vec<Target>,
    target_index: usize,

    var_screen_info: &'a VarScreeninfo,
    should_refresh: bool
}

impl<'a> LoginManager<'a> {
    fn new(
        fb: &'a mut Framebuffer,
        screen_size: (u32, u32),
        greetd: greetd::GreetD,
        targets: Vec<Target>,
        settings: &settings::Settings,
        colors: settings::ResolvedColors
    ) -> Self {
        let form = &settings.form;
        let forced_username = form
            .username
            .default
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let (target_index, forced_target_found) = match form
            .session
            .default
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(forced) => match targets.iter().position(|t| t.name == forced)
            {
                Some(i) => {
                    info!("Using configured target session as default: {forced:?}");
                    (i, true)
                }
                None => {
                    warn!(
                        "Configured form.session.default {forced:?} did not match any discovered session; leaving session selection enabled"
                    );
                    (0, false)
                }
            },
            None => (0, false)
        };

        let lock_target = forced_target_found && form.session.locked;

        if let Some(u) = forced_username.as_deref() {
            info!("Forcing username from config (len={})", u.len());
            debug!("Forced username: {u:?}");
        }

        let lock_username = forced_username.is_some() && form.username.locked;
        let mode = if lock_username {
            Mode::EditingPassword
        } else {
            Mode::EditingUsername
        };

        let password_mask = form.password.mask.trim();
        let password_mask = if password_mask.is_empty() {
            "*".to_string()
        } else {
            password_mask.to_string()
        };

        let [session_left_arrow, session_right_arrow] =
            form.session.arrows.clone().map(|a| a.trim().to_string());

        let power = &settings.power;
        let power_spec = settings.power_font();
        let power_font = draw::Font::new(&power_spec.family, power_spec.size);
        let pad_x = (power_spec.size * 0.6).round().max(0.0) as u32;
        let pad_y = (power_spec.size * 0.3).round().max(0.0) as u32;
        let mut power_items = Vec::new();
        for action in power::resolve(&power.actions) {
            let text = action.text();
            match power_font.measure(&text) {
                Ok((w, text_h)) => power_items.push(power::Item {
                    action,
                    text,
                    w: w + pad_x * 2,
                    text_h
                }),
                Err(e) => warn!("Skipping power action {text:?}: {e}")
            }
        }
        let power_item_h = power_items
            .iter()
            .map(|item| item.text_h)
            .max()
            .unwrap_or(0)
            + pad_y * 2;

        let hint_spec = settings.power_hint_font();
        let power_hint_font =
            draw::Font::new(&hint_spec.family, hint_spec.size);
        let power_hint_size =
            power_hint_font.measure(power::HINT).unwrap_or_else(|e| {
                warn!("Unable to measure power hint; not drawing it: {e}");
                (0, 0)
            });

        let (status_font, status_h) = if settings.status.enabled {
            let spec = settings.status_font();
            let font = draw::Font::new(&spec.family, spec.size);
            match font.measure("Ag") {
                Ok((_, h)) => (Some(font), h),
                Err(e) => {
                    warn!("Unable to measure status font; not drawing it: {e}");
                    (None, 0)
                }
            }
        } else {
            (None, 0)
        };

        let main_spec = settings.form_font();
        let main_font = draw::Font::new(&main_spec.family, main_spec.size);
        let icon_spec = settings.icon_font();
        let icon_font = draw::Font::new(&icon_spec.family, icon_spec.size);
        let heading_spec = settings.header_font();
        let heading_font =
            draw::Font::new(&heading_spec.family, heading_spec.size);
        let mut lm = Self {
            buf: &mut fb.frame,
            device: &fb.device,
            heading_font,
            main_font,
            icon_font,
            field_icons: field_icon::FieldIcons::default(),
            colors,
            header: template::Template::parse(&settings.header.text),
            power_font,
            power_items,
            power_item_h,
            power_gap: power.gap,
            power_offset: power.offset,
            power_armed: None,
            power_hint_font,
            power_hint_size,
            power_hint_offset: power.hint.offset,
            status_font,
            status_h,
            status_offset: settings.status.offset,
            status: None,
            forced_username,
            lock_target,
            lock_username,
            session_gap_below: form.session.gap_below,
            username_gap_below: form.username.gap_below,
            row_h: form.row_height,
            password_mask,
            text_align: form.text_align,
            form_padding: form.padding,
            header_offset: settings.header.offset,
            clear_screen_on_success: settings.login.clear_screen_on_success,
            session_left_arrow,
            session_right_arrow,
            screen_size,
            form_width: form.width,
            mode,
            greetd,
            targets,
            target_index, // TODO: remember last user selection
            var_screen_info: &fb.var_screen_info,
            should_refresh: false
        };

        lm.field_icons = field_icon::resolve(
            &form.username.icon,
            &form.password.icon,
            form.icons.gap,
            &lm.icon_font,
            &lm.main_font,
            lm.show_username_row()
        );
        if !lm.field_icons.is_empty() {
            let form = lm.form_layout();
            let with = layout::field_columns(
                form.w,
                lm.form_padding,
                lm.field_icons.col_w,
                lm.text_align
            );
            let without =
                layout::field_columns(form.w, lm.form_padding, 0, lm.text_align);
            if with.text_w < without.text_w / 2 {
                warn!("Field icons leave less than half the field for text; not drawing them");
                lm.field_icons = field_icon::FieldIcons::default();
            }
        }

        if !lm.power_items.is_empty() && lm.power_row_layout().is_none() {
            warn!("Power row does not fit on screen; not drawing it");
        }
        if let (Some((status_y, status_h)), Some(row)) =
            (lm.status_layout(), lm.power_row_layout())
        {
            if status_y + status_h > row.y {
                warn!("Status line overlaps the power row; raise power.offset");
            }
        }

        lm
    }

    fn power_action_for_key(&self, key: u8) -> Option<usize> {
        self.power_items.iter().position(|item| item.action.key == key)
    }

    pub(crate) fn show_target_row(&self) -> bool {
        !self.lock_target
    }

    pub(crate) fn show_username_row(&self) -> bool {
        !self.lock_username
    }
}

fn main() {
    if let Err(e) = init_logging() {
        // If the log file can't be opened (permissions, missing /var, etc), we
        // can't reliably provide the requested file logging.
        eprintln!(
            "Failed to initialize file logger (/var/log/mflm/mflm.log): {e}"
        );
        return;
    }

    info!("mflm starting at {}", Local::now().to_rfc3339());
    debug!("argv: {:?}", std::env::args().collect::<Vec<_>>());

    let settings = match settings::Settings::load() {
        Ok(s) => {
            info!("Loaded configuration successfully");
            s
        }
        Err(e) => {
            warn!("Failed to load config; using defaults: {e}");
            settings::Settings::default()
        }
    };
    debug!("Settings: {settings:#?}");

    let colors = match settings.resolve_colors() {
        Ok(c) => c,
        Err(e) => {
            warn!("Invalid colors in config; using defaults: {e}");
            settings::Settings::default()
                .resolve_colors()
                .expect("default colors must be valid")
        }
    };

    let mut framebuffer = match Framebuffer::new("/dev/fb0") {
        Ok(fb) => fb,
        Err(e) => {
            error!("Unable to open framebuffer device /dev/fb0: {e}");
            return;
        }
    };

    let w = framebuffer.var_screen_info.xres;
    let h = framebuffer.var_screen_info.yres;

    let raw = match std::io::stdout().into_raw_mode() {
        Ok(raw) => raw,
        Err(e) => {
            error!("Unable to enter raw mode: {e}");
            return;
        }
    };

    if let Err(e) = Framebuffer::set_kd_mode(KdMode::Graphics) {
        error!("Unable to enter graphics mode: {e}");
        drop(raw);
        return;
    }

    let greetd = match greetd::GreetD::new() {
        Ok(g) => g,
        Err(e) => {
            error!("Unable to connect to greetd: {e}");
            let _ = Framebuffer::set_kd_mode(KdMode::Text);
            drop(raw);
            return;
        }
    };

    info!("Scanning session targets");
    let mut targets = Vec::new();
    for dir in ["/usr/share/wayland-sessions", "/usr/share/xsessions"] {
        match fs::read_dir(dir) {
            Ok(rd) => {
                for entry in rd.flatten() {
                    if let Some(target) = Target::load(entry.path()) {
                        targets.push(target);
                    }
                }
            }
            Err(e) => {
                warn!("Unable to read sessions dir {dir}: {e}");
            }
        }
    }

    if targets.is_empty() {
        error!("No session targets found; cannot continue");
        let _ = Framebuffer::set_kd_mode(KdMode::Text);
        drop(raw);
        return;
    }

    info!("Loaded {} session targets", targets.len());

    let mut lm = LoginManager::new(
        &mut framebuffer,
        (w, h),
        greetd,
        targets,
        &settings,
        colors
    );

    lm.clear();
    let bg = lm.colors.muted;
    if let Err(e) = lm.draw_bg(&bg) {
        error!("Unable to draw background: {e}");
        let _ = Framebuffer::set_kd_mode(KdMode::Text);
        drop(raw);
        return;
    }
    lm.refresh();

    lm.greeter_loop();
    if let Err(e) = Framebuffer::set_kd_mode(KdMode::Text) {
        error!("Unable to leave graphics mode: {e}");
    }
    drop(raw);
}

fn init_logging() -> Result<(), io::Error> {
    let log_dir = Path::new("/var/log/mflm");
    let log_path = log_dir.join("mflm.log");

    fs::create_dir_all(log_dir)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Debug = verbose
    let mut log_config_builder = LogConfigBuilder::new();
    if log_config_builder.set_time_offset_to_local().is_err() {
        warn!("Failed to set log timestamps to local time");
    }
    let log_config = log_config_builder.build();

    WriteLogger::init(LevelFilter::Debug, log_config, file)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}
