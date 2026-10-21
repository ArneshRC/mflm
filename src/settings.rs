use serde::Deserialize;

use crate::color::{Color, ParseColorError};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Fonts {
    /// Pango font description string for the main UI font (used for session/user/pass rows).
    /// Example: "DejaVu Sans Mono" or "Sans".
    pub main: String,

    /// Pango font description string for the heading UI font.
    /// Example: "Sans Bold".
    pub heading: String,

    /// Font size for main UI text (pixels).
    pub main_size_px: f32,

    /// Font size for heading UI text (pixels).
    pub heading_size_px: f32
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            main: "Monospace".to_string(),
            heading: "Sans".to_string(),
            main_size_px: 42.0,
            heading_size_px: 72.0
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Colors {
    /// Hex: "#RRGGBB" or "#AARRGGBB".
    pub foreground: String,
    /// Hex: "#RRGGBB" or "#AARRGGBB".
    pub background: String,
    /// Used for the default box and other neutral UI.
    pub neutral: String,
    /// Used for selections / active fields / in-progress actions.
    pub selected: String,
    /// Used for errors (e.g. auth failure).
    pub error: String,
    /// Background of a power action awaiting confirmation. Defaults to
    /// `selected`.
    pub confirm: Option<String>
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            foreground: "#fafafa".to_string(),
            background: "#000000".to_string(),
            neutral: "#cccccc".to_string(),
            selected: "#aaccee".to_string(),
            error: "#ecbcad".to_string(),
            confirm: None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedColors {
    pub foreground: Color,
    pub background: Color,
    pub neutral: Color,
    pub selected: Color,
    pub error: Color,
    pub confirm: Color
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Login {
    /// Optional session target name to force.
    pub target: Option<String>,

    /// Optional username to force.
    pub username: Option<String>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Header {
    /// Heading text. Supports placeholders: {hostname}. Use {{ and }} for
    /// literal braces.
    pub text: String
}

impl Default for Header {
    fn default() -> Self {
        Self {
            text: "Welcome to {hostname}".to_string()
        }
    }
}

/// One entry of `[[power.action]]`. Fields are loosely typed so a bad entry
/// is skipped (see `power::resolve`) instead of failing the whole config.
#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PowerAction {
    /// "shutdown", "reboot", "suspend" or "hibernate".
    pub kind: String,
    /// "F1" through "F12".
    pub key: String,
    /// Defaults per kind, e.g. "Shutdown".
    pub label: Option<String>,
    /// Usually a Nerd Font glyph. Empty means no icon.
    pub icon: String,
    /// argv, run without a shell. Defaults to `systemctl <verb>`.
    pub command: Option<Vec<String>>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Power {
    /// Pango font description. Defaults to `fonts.main`.
    pub font: Option<String>,
    pub size_px: f32,
    /// Horizontal gap between actions.
    pub gap_px: u32,
    /// Gap between the bottom of the form and the row.
    pub offset_y_px: u32,
    /// Size of the "[Enter] Accept / [Esc] Cancel" hint under an armed
    /// action. Defaults to 70% of `size_px`.
    pub hint_size_px: Option<f32>,
    /// Gap between the row and the hint.
    pub hint_offset_y_px: u32,
    /// Actions in on-screen order. Empty means no power row.
    pub action: Vec<PowerAction>
}

impl Default for Power {
    fn default() -> Self {
        Self {
            font: None,
            size_px: 18.0,
            gap_px: 32,
            offset_y_px: 240,
            hint_size_px: None,
            hint_offset_y_px: 8,
            action: Vec::new()
        }
    }
}

/// Line below the form showing "Logging in…", "Login failed" and similar.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Status {
    /// When false, errors flash the form in the error color instead.
    pub show: bool,
    /// Pango font description. Defaults to `fonts.main`.
    pub font: Option<String>,
    pub size_px: f32,
    /// Gap between the bottom of the form and the line.
    pub offset_y_px: u32
}

impl Default for Status {
    fn default() -> Self {
        Self {
            show: true,
            font: None,
            size_px: 18.0,
            offset_y_px: 24
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Ui {
    pub hide_target: bool,
    pub hide_username: bool,
    pub gap_below_session_px: u32,
    pub gap_below_username_px: u32,
    pub row_h: u32,
    pub password_char: String,
    pub text_align: TextAlign,
    pub input_margin_px: u32,
    pub heading_offset_y_px: u32,
    pub blackout_on_success: bool,
    pub form_width: u32,
    pub form_height: u32,
    pub session_left_arrow: String,
    pub session_right_arrow: String,
    /// Shown left of the username, usually a Nerd Font glyph such as
    /// "". Empty means no icon.
    pub username_icon: String,
    /// Shown left of the password, e.g. "". Empty means no icon.
    pub password_icon: String,
    /// Pango font description for the icons. Defaults to `fonts.main`.
    pub icon_font: Option<String>,
    /// Gap between an icon and its field's text.
    pub icon_gap_px: u32
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            hide_target: false,
            hide_username: false,
            gap_below_session_px: 64,
            gap_below_username_px: 32,
            row_h: 72,
            password_char: "•".to_string(),
            text_align: TextAlign::Center,
            input_margin_px: 15,
            heading_offset_y_px: 24,
            blackout_on_success: true,
            form_width: 512,
            form_height: 168,
            session_left_arrow: "❮".to_string(),
            session_right_arrow: "❯".to_string(),
            username_icon: String::new(),
            password_icon: String::new(),
            icon_font: None,
            icon_gap_px: 24
        }
    }
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub fonts: Fonts,
    pub colors: Colors,
    pub login: Login,
    pub header: Header,
    pub power: Power,
    pub status: Status,
    pub ui: Ui
}

impl Settings {
    /// Loads configuration from /etc/mflm/config.toml. Missing keys fall back
    /// to the `Default` impls above.
    pub fn load() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(
                config::File::from(std::path::Path::new(
                    "/etc/mflm/config.toml"
                ))
                .format(config::FileFormat::Toml)
                .required(false)
            )
            .build()?
            .try_deserialize::<Self>()
    }

    pub fn resolve_colors(&self) -> Result<ResolvedColors, ParseColorError> {
        let selected = Color::from_hex(&self.colors.selected)?;
        Ok(ResolvedColors {
            foreground: Color::from_hex(&self.colors.foreground)?,
            background: Color::from_hex(&self.colors.background)?,
            neutral: Color::from_hex(&self.colors.neutral)?,
            selected,
            error: Color::from_hex(&self.colors.error)?,
            confirm: match &self.colors.confirm {
                Some(c) => Color::from_hex(c)?,
                None => selected
            }
        })
    }
}
