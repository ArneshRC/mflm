#![deny(rust_2018_idioms)]

use std::{fs, fs::OpenOptions, io, path::Path};

use chrono::Local;
use framebuffer::{Framebuffer, KdMode, VarScreeninfo};
use freedesktop_desktop_entry::DesktopEntry;
use log::{debug, error, info, warn};
use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
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
mod greetd;
mod greeter_loop;
mod layout;
mod settings;

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

    colors: settings::ResolvedColors,

    forced_username: Option<String>,
    lock_target: bool,
    hide_target: bool,
    hide_username: bool,
    gap_below_session_px: u32,
    gap_below_username_px: u32,
    row_h: u32,
    password_char: String,
    text_align: settings::TextAlign,
    session_left_arrow: String,
    session_right_arrow: String,

    screen_size: (u32, u32),
    dimensions: (u32, u32),
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
        dimensions: (u32, u32),
        greetd: greetd::GreetD,
        targets: Vec<Target>,
        fonts: &settings::Fonts,
        colors: settings::ResolvedColors,
        login: &settings::Login,
        ui: &settings::Ui
    ) -> Self {
        let forced_username = login
            .username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let (target_index, forced_target_found) = match login
            .target
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
                        "Configured login.target {forced:?} did not match any discovered session; leaving session selection enabled"
                    );
                    (0, false)
                }
            },
            None => (0, false)
        };

        let lock_target = forced_target_found && ui.hide_target;

        if let Some(u) = forced_username.as_deref() {
            info!("Forcing username from config (len={})", u.len());
            debug!("Forced username: {u:?}");
        }

        let mode = if forced_username.is_some() && ui.hide_username {
            Mode::EditingPassword
        } else {
            Mode::EditingUsername
        };

        let password_char = ui.password_char.trim();
        let password_char = if password_char.is_empty() {
            "*".to_string()
        } else {
            password_char.to_string()
        };

        let session_left_arrow = ui.session_left_arrow.trim().to_string();
        let session_right_arrow = ui.session_right_arrow.trim().to_string();

        Self {
            buf: &mut fb.frame,
            device: &fb.device,
            heading_font: draw::Font::new(&fonts.heading, fonts.heading_size_px),
            main_font: draw::Font::new(&fonts.main, fonts.main_size_px),
            colors,
            forced_username,
            lock_target,
            hide_target: ui.hide_target,
            hide_username: ui.hide_username,
            gap_below_session_px: ui.gap_below_session_px,
            gap_below_username_px: ui.gap_below_username_px,
            row_h: ui.row_h,
            password_char,
            text_align: ui.text_align,
            session_left_arrow,
            session_right_arrow,
            screen_size,
            dimensions,
            mode,
            greetd,
            targets,
            target_index, // TODO: remember last user selection
            var_screen_info: &fb.var_screen_info,
            should_refresh: false
        }
    }

    pub(crate) fn show_target_row(&self) -> bool {
        if self.lock_target {
            !self.hide_target
        } else {
            true
        }
    }

    pub(crate) fn show_username_row(&self) -> bool {
        if self.forced_username.is_some() {
            !self.hide_username
        } else {
            true
        }
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
            debug!(
                "Configured fonts: heading={:?} ({}px), main={:?} ({}px)",
                s.fonts.heading,
                s.fonts.heading_size_px,
                s.fonts.main,
                s.fonts.main_size_px
            );
            debug!(
                "Configured login: target={:?} username={:?}",
                s.login.target,
                s.login.username
            );
            debug!(
                "Configured ui: hide_target={} hide_username={} gap_below_session_px={} gap_below_username_px={} row_h={} password_char={:?} text_align={:?} form_width={} form_height={}",
                s.ui.hide_target,
                s.ui.hide_username,
                s.ui.gap_below_session_px,
                s.ui.gap_below_username_px,
                s.ui.row_h,
                s.ui.password_char,
                s.ui.text_align,
                s.ui.form_width,
                s.ui.form_height
            );
            s
        }
        Err(e) => {
            warn!("Failed to load config; using defaults: {e}");
            let s = settings::Settings::default();
            debug!(
                "Default fonts: heading={:?} ({}px), main={:?} ({}px)",
                s.fonts.heading,
                s.fonts.heading_size_px,
                s.fonts.main,
                s.fonts.main_size_px
            );
            debug!(
                "Default login: target={:?} username={:?}",
                s.login.target,
                s.login.username
            );
            debug!(
                "Default ui: hide_target={} hide_username={} gap_below_session_px={} gap_below_username_px={} row_h={} password_char={:?} text_align={:?} form_width={} form_height={}",
                s.ui.hide_target,
                s.ui.hide_username,
                s.ui.gap_below_session_px,
                s.ui.gap_below_username_px,
                s.ui.row_h,
                s.ui.password_char,
                s.ui.text_align,
                s.ui.form_width,
                s.ui.form_height
            );
            s
        }
    };

    let colors = match settings.resolve_colors() {
        Ok(c) => {
            debug!(
                "Configured colors: fg={:?} bg={:?} neutral={:?} selected={:?} error={:?}",
                settings.colors.foreground,
                settings.colors.background,
                settings.colors.neutral,
                settings.colors.selected,
                settings.colors.error
            );
            c
        }
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
        (settings.ui.form_width, settings.ui.form_height),
        greetd,
        targets,
        &settings.fonts,
        colors,
        &settings.login,
        &settings.ui,
    );

    lm.clear();
    let bg = lm.colors.neutral;
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

    // Debug = verbose. Simplelog's default config includes timestamps; we also
    // log a clear startup banner with full date/time.
    WriteLogger::init(LevelFilter::Debug, LogConfig::default(), file)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}
