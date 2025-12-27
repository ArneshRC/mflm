#![deny(rust_2018_idioms)]

use std::fs::OpenOptions;
use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;

use chrono::Local;
use color::Color;
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
mod settings;

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    SelectingSession,
    EditingUsername,
    EditingPassword,
}

#[derive(Error, Debug)]
#[non_exhaustive]
enum Error {
    #[error("Error performing buffer operation: {0}")]
    Buffer(#[from] buffer::BufferError),
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

struct Target {
    name: String,
    exec: Vec<String>,
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

    headline_font: draw::Font,
    prompt_font: draw::Font,

    colors: settings::ResolvedColors,

    forced_username: Option<String>,
    lock_target: bool,

    screen_size: (u32, u32),
    dimensions: (u32, u32),
    mode: Mode,
    greetd: greetd::GreetD,
    targets: Vec<Target>,
    target_index: usize,

    var_screen_info: &'a VarScreeninfo,
    should_refresh: bool,
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
    ) -> Self {
        let forced_username = login
            .username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let (target_index, lock_target) = match login
            .target
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(forced) => match targets.iter().position(|t| t.name == forced) {
                Some(i) => {
                    info!("Forcing target session from config: {forced:?}");
                    (i, true)
                }
                None => {
                    warn!(
                        "Configured login.target {forced:?} did not match any discovered session; leaving session selection enabled"
                    );
                    (0, false)
                }
            },
            None => (0, false),
        };

        if let Some(u) = forced_username.as_deref() {
            info!("Forcing username from config (len={})", u.len());
            debug!("Forced username: {u:?}");
        }

        let mode = if forced_username.is_some() {
            Mode::EditingPassword
        } else {
            Mode::EditingUsername
        };

        Self {
            buf: &mut fb.frame,
            device: &fb.device,
            headline_font: draw::Font::new(&fonts.main, 72.0),
            prompt_font: draw::Font::new(&fonts.mono, 32.0),
            colors,
            forced_username,
            lock_target,
            screen_size,
            dimensions,
            mode,
            greetd,
            targets,
            target_index, // TODO: remember last user selection
            var_screen_info: &fb.var_screen_info,
            should_refresh: false,
        }
    }

    fn mode_allowed(&self, mode: Mode) -> bool {
        match mode {
            Mode::SelectingSession => !self.lock_target,
            Mode::EditingUsername => self.forced_username.is_none(),
            Mode::EditingPassword => true,
        }
    }

    fn next_allowed_mode(&self, from: Mode) -> Mode {
        let mut cur = from;
        for _ in 0..3 {
            cur = match cur {
                Mode::SelectingSession => Mode::EditingUsername,
                Mode::EditingUsername => Mode::EditingPassword,
                Mode::EditingPassword => Mode::SelectingSession,
            };
            if self.mode_allowed(cur) {
                return cur;
            }
        }
        from
    }

    fn prev_allowed_mode(&self, from: Mode) -> Mode {
        let mut cur = from;
        for _ in 0..3 {
            cur = match cur {
                Mode::SelectingSession => Mode::EditingPassword,
                Mode::EditingUsername => Mode::SelectingSession,
                Mode::EditingPassword => Mode::EditingUsername,
            };
            if self.mode_allowed(cur) {
                return cur;
            }
        }
        from
    }

    fn refresh(&mut self) {
        if self.should_refresh {
            self.should_refresh = false;
            let mut screeninfo = self.var_screen_info.clone();
            screeninfo.activate |= FB_ACTIVATE_NOW | FB_ACTIVATE_FORCE;
            if let Err(e) = Framebuffer::put_var_screeninfo(self.device, &screeninfo) {
                error!("Failed to refresh framebuffer: {e}");
            }
        }
    }

    fn clear(&mut self) {
        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        buf.memset(&self.colors.background);
        self.should_refresh = true;
    }

    fn offset(&self) -> (u32, u32) {
        (
            (self.screen_size.0 - self.dimensions.0) / 2,
            (self.screen_size.1 - self.dimensions.1) / 2,
        )
    }

    fn draw_bg(&mut self, box_color: &Color) -> Result<(), Error> {
        let (x, y) = self.offset();
        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let bg = self.colors.background;
        let fg = self.colors.foreground;

        draw::draw_box(
            &mut buf.subdimensions((x, y, self.dimensions.0, self.dimensions.1))?,
            box_color,
            (self.dimensions.0, self.dimensions.1),
        )?;

        let hostname = hostname::get()?.to_string_lossy().into_owned();

        self.headline_font.auto_draw_text(
            &mut buf.offset(((self.screen_size.0 / 2) - 300, 32))?,
            &bg,
            &fg,
            &format!("Welcome to {hostname}"),
        )?;

        self.headline_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((32, 24))?,
            &bg,
            &fg,
            "Login",
        )?;

        let (session_color, username_color, password_color) = match self.mode {
            Mode::SelectingSession => (self.colors.selected, fg, fg),
            Mode::EditingUsername => (fg, self.colors.selected, fg),
            Mode::EditingPassword => (fg, fg, self.colors.selected),
        };

        let label_w = 416 - 256;
        let field_w = self.dimensions.0 - 416 - 32;
        let row_h = 32;

        if self.lock_target {
            let mut label = buf.subdimensions((x + 256, y + 24, label_w, row_h))?;
            label.memset(&bg);
            let mut field = buf.subdimensions((x + 416, y + 24, field_w, row_h))?;
            field.memset(&bg);
        }

        if self.forced_username.is_some() {
            let mut label = buf.subdimensions((x + 256, y + 64, label_w, row_h))?;
            label.memset(&bg);
            let mut field = buf.subdimensions((x + 416, y + 64, field_w, row_h))?;
            field.memset(&bg);
        }

        if !self.lock_target {
            self.prompt_font.auto_draw_text(
                &mut buf
                    .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                    .offset((256, 24))?,
                &bg,
                &session_color,
                "session:",
            )?;
        }

        if self.forced_username.is_none() {
            self.prompt_font.auto_draw_text(
                &mut buf
                    .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                    .offset((256, 64))?,
                &bg,
                &username_color,
                "username:",
            )?;
        }

        self.prompt_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((256, 104))
                ?,
            &bg,
            &password_color,
            "password:",
        )?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_target(&mut self) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 24);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let bg = self.colors.background;
        buf.memset(&bg);

        self.prompt_font.auto_draw_text(
            &mut buf,
            &bg,
            &self.colors.foreground,
            &self.targets[self.target_index].name,
        )?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_username(&mut self, username: &str, redraw: bool) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 64);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let bg = self.colors.background;
        if redraw {
            buf.memset(&bg);
        }

        self.prompt_font
            .auto_draw_text(&mut buf, &bg, &self.colors.foreground, username)?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_password(&mut self, password: &str, redraw: bool) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 104);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let bg = self.colors.background;
        if redraw {
            buf.memset(&bg);
        }

        let mut stars = "".to_string();
        for _ in 0..password.len() {
            stars += "*";
        }

        self.prompt_font
            .auto_draw_text(&mut buf, &bg, &self.colors.foreground, &stars)?;

        self.should_refresh = true;

        Ok(())
    }

    fn goto_next_mode(&mut self) {
        self.mode = self.next_allowed_mode(self.mode);
    }

    fn goto_prev_mode(&mut self) {
        self.mode = self.prev_allowed_mode(self.mode);
    }

    fn greeter_loop(&mut self) {
        let mut username = self
            .forced_username
            .clone()
            .unwrap_or_else(|| String::with_capacity(USERNAME_CAP));
        let mut password = String::with_capacity(PASSWORD_CAP);
        let mut last_username_len = usize::MAX;
        let mut last_password_len = password.len();
        let mut last_target_index = self.target_index;
        let mut last_mode = self.mode;
        let mut had_failure = false;

        let stdin_handle = std::io::stdin();
        let stdin_lock = stdin_handle.lock();
        let mut stdin_bytes = stdin_lock.bytes();

        let mut read_byte = || -> Option<u8> { stdin_bytes.next().and_then(Result::ok) };

        if !self.lock_target {
            if let Err(e) = self.draw_target() {
                error!("Fatal: unable to draw target session: {e}");
                return;
            }
        }

        loop {
            if self.forced_username.is_none() && username.len() != last_username_len {
                if let Err(e) =
                    self.draw_username(&username, username.len() < last_username_len)
                {
                    error!("Fatal: unable to draw username prompt: {e}");
                    return;
                }
                last_username_len = username.len();
            }
            if password.len() != last_password_len {
                if let Err(e) =
                    self.draw_password(&password, password.len() < last_password_len)
                {
                    error!("Fatal: unable to draw password prompt: {e}");
                    return;
                }
                last_password_len = password.len();
            }
            if !self.lock_target && last_target_index != self.target_index {
                if let Err(e) = self.draw_target() {
                    error!("Fatal: unable to draw target session: {e}");
                    return;
                }
                last_target_index = self.target_index;
            }
            if last_mode != self.mode {
                let bg = self.colors.neutral;
                if let Err(e) = self.draw_bg(&bg) {
                    error!("Fatal: unable to draw background: {e}");
                    return;
                }
                last_mode = self.mode;
            }

            if had_failure {
                let bg = self.colors.neutral;
                if let Err(e) = self.draw_bg(&bg) {
                    error!("Fatal: unable to draw background: {e}");
                    return;
                }
                had_failure = false;
            }

            let b = match read_byte() {
                Some(b) => b,
                None => {
                    warn!("stdin closed; exiting greeter loop");
                    return;
                }
            };

            match b as char {
                '\x15' | '\x0B' => match self.mode {
                    // ctrl-k/ctrl-u
                    Mode::SelectingSession => (),
                    Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.clear();
                        }
                    }
                    Mode::EditingPassword => password.clear(),
                },
                '\x03' | '\x04' => {
                    // ctrl-c/ctrl-D
                    username.clear();
                    password.clear();
                    if let Err(e) = self.greetd.cancel() {
                        warn!("Failed to cancel greetd session: {e}");
                    }
                    return;
                }
                '\x7F' => match self.mode {
                    // backspace
                    Mode::SelectingSession => (),
                    Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.pop();
                        }
                    }
                    Mode::EditingPassword => {
                        password.pop();
                    }
                },
                '\t' => self.goto_next_mode(),
                '\r' => match self.mode {
                    Mode::SelectingSession => {
                        self.mode = if self.forced_username.is_some() {
                            Mode::EditingPassword
                        } else {
                            Mode::EditingUsername
                        };
                    }
                    Mode::EditingUsername => {
                        if self.forced_username.is_none() && !username.is_empty() {
                            self.mode = Mode::EditingPassword;
                        }
                    }
                    Mode::EditingPassword => {
                        if password.is_empty() {
                            if self.forced_username.is_none() {
                                username.clear();
                                self.mode = Mode::EditingUsername;
                            }
                        } else {
                            let bg = self.colors.selected;
                            if let Err(e) = self.draw_bg(&bg) {
                                error!("Fatal: unable to draw background: {e}");
                                return;
                            }
                            info!(
                                "Attempting login via greetd (session_index={}, username_len={})",
                                self.target_index,
                                username.len()
                            );

                            let username_for_login = self
                                .forced_username
                                .clone()
                                .unwrap_or_else(|| username.clone());
                            let password_for_login = std::mem::take(&mut password);
                            let res = self.greetd.login(
                                username_for_login,
                                password_for_login,
                                self.targets[self.target_index].exec.clone(),
                            );

                            if self.forced_username.is_none() {
                                username = String::with_capacity(USERNAME_CAP);
                            } else {
                                username = self.forced_username.clone().unwrap();
                            }
                            password = String::with_capacity(PASSWORD_CAP);
                            match res {
                                Ok(_) => {
                                    info!("Login succeeded; exiting greeter loop");
                                    return;
                                }
                                Err(e) => {
                                    warn!("Login failed: {e}");
                                    let bg = self.colors.error;
                                    if let Err(e) = self.draw_bg(&bg) {
                                        error!("Fatal: unable to draw background: {e}");
                                        return;
                                    }
                                    self.mode = if self.forced_username.is_some() {
                                        Mode::EditingPassword
                                    } else {
                                        Mode::EditingUsername
                                    };
                                    if let Err(e) = self.greetd.cancel() {
                                        warn!("Failed to cancel greetd session after login failure: {e}");
                                    }
                                    had_failure = true;
                                }
                            }
                        }
                    }
                },
                // this is terrible
                '\x1b' => match read_byte() {
                    Some(b'[') => match read_byte() {
                        Some(b'A') => self.goto_prev_mode(),
                        Some(b'B') => self.goto_next_mode(),
                        Some(b'C') => match self.mode {
                            Mode::SelectingSession => {
                                if !self.lock_target {
                                    self.target_index =
                                        (self.target_index + 1) % self.targets.len()
                                }
                            }
                            _ => (), // TODO: cursor
                        },
                        Some(b'D') => match self.mode {
                            Mode::SelectingSession => {
                                if !self.lock_target {
                                    if self.target_index == 0 {
                                        self.target_index = self.targets.len();
                                    }
                                    self.target_index -= 1;
                                }
                            }
                            _ => (), // TODO: cursor
                        },
                        _ => (), // shrug
                    },
                    _ => (), // shrug
                },
                v => match self.mode {
                    Mode::SelectingSession => (),
                    Mode::EditingUsername => {
                        if self.forced_username.is_none() {
                            username.push(v as char)
                        }
                    }
                    Mode::EditingPassword => password.push(v as char),
                },
            }
            self.refresh();
        }
    }
}

fn main() {
    if let Err(e) = init_logging() {
        // If the log file can't be opened (permissions, missing /var, etc), we
        // can't reliably provide the requested file logging.
        eprintln!("Failed to initialize file logger (/var/log/mflm/mflm.log): {e}");
        return;
    }

    info!("mflm starting at {}", Local::now().to_rfc3339());
    debug!("argv: {:?}", std::env::args().collect::<Vec<_>>());

    let settings = match settings::Settings::load() {
        Ok(s) => {
            info!("Loaded configuration successfully");
            debug!("Configured fonts: main={:?}, mono={:?}", s.fonts.main, s.fonts.mono);
            s
        }
        Err(e) => {
            warn!("Failed to load config; using defaults: {e}");
            let s = settings::Settings::default();
            debug!("Default fonts: main={:?}, mono={:?}", s.fonts.main, s.fonts.mono);
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
        (1024, 168),
        greetd,
        targets,
        &settings.fonts,
        colors,
        &settings.login,
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
    let file = OpenOptions::new().create(true).append(true).open(&log_path)?;

    // Debug = verbose. Simplelog's default config includes timestamps; we also
    // log a clear startup banner with full date/time.
    WriteLogger::init(LevelFilter::Debug, LogConfig::default(), file)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}
