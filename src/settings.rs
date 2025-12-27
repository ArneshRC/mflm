use serde::Deserialize;

use crate::color::{Color, ParseColorError};

#[derive(Debug, Clone, Deserialize)]
pub struct Fonts {
    /// Pango font description string for the main UI font (used for session/user/pass rows).
    /// Example: "DejaVu Sans Mono" or "Sans".
    pub main: String,

    /// Pango font description string for the heading UI font.
    /// Example: "Sans Bold".
    pub heading: String,

    /// Font size for main UI text (pixels).
    #[serde(default = "default_main_font_size_px")]
    pub main_size_px: f32,

    /// Font size for heading UI text (pixels).
    #[serde(default = "default_heading_font_size_px")]
    pub heading_size_px: f32
}

#[derive(Debug, Clone, Deserialize)]
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
    pub error: String
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            foreground: "#FFFFFF".to_string(),
            background: "#000000".to_string(),
            neutral: "#BFBFBF".to_string(),
            selected: "#BFBF3F".to_string(),
            error: "#BF3F3F".to_string()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedColors {
    pub foreground: Color,
    pub background: Color,
    pub neutral: Color,
    pub selected: Color,
    pub error: Color
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            main: "Monospace".to_string(),
            heading: "Sans".to_string(),
            main_size_px: default_main_font_size_px(),
            heading_size_px: default_heading_font_size_px()
        }
    }
}

fn default_main_font_size_px() -> f32 {
    42.0
}

fn default_heading_font_size_px() -> f32 {
    72.0
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct Login {
    /// Optional session target name to force.
    pub target: Option<String>,

    /// Optional username to force.
    pub username: Option<String>
}

fn default_gap_below_session_px() -> u32 {
    64
}

fn default_gap_below_username_px() -> u32 {
    32
}

fn default_row_h() -> u32 {
    72
}

fn default_password_char() -> String {
    "*".to_string()
}

fn default_text_align() -> TextAlign {
    TextAlign::Center
}

fn default_form_width() -> u32 {
    512
}

fn default_form_height() -> u32 {
    168
}

fn default_session_left_arrow() -> String {
    "❮".to_string()
}

fn default_session_right_arrow() -> String {
    "❯".to_string()
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ui {
    #[serde(default = "default_gap_below_session_px")]
    pub gap_below_session_px: u32,

    #[serde(default = "default_gap_below_username_px")]
    pub gap_below_username_px: u32,

    #[serde(default = "default_row_h")]
    pub row_h: u32,

    #[serde(default = "default_password_char")]
    pub password_char: String,

    #[serde(default = "default_text_align")]
    pub text_align: TextAlign,

    #[serde(default = "default_form_width")]
    pub form_width: u32,

    #[serde(default = "default_form_height")]
    pub form_height: u32,

    #[serde(default = "default_session_left_arrow")]
    pub session_left_arrow: String,

    #[serde(default = "default_session_right_arrow")]
    pub session_right_arrow: String
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            gap_below_session_px: default_gap_below_session_px(),
            gap_below_username_px: default_gap_below_username_px(),
            row_h: default_row_h(),
            password_char: default_password_char(),
            text_align: default_text_align(),
            form_width: default_form_width(),
            form_height: default_form_height(),
            session_left_arrow: default_session_left_arrow(),
            session_right_arrow: default_session_right_arrow()
        }
    }
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub fonts: Fonts,

    #[serde(default)]
    pub colors: Colors,

    #[serde(default)]
    pub login: Login,

    #[serde(default)]
    pub ui: Ui
}

impl Settings {
    /// Loads configuration from /etc/mflm/config.toml
    pub fn load() -> Result<Self, config::ConfigError> {
        let builder = config::Config::builder()
            .set_default("fonts.main", Fonts::default().main)?
            .set_default("fonts.heading", Fonts::default().heading)?
            .set_default("fonts.main_size_px", default_main_font_size_px() as f64)?
            .set_default("fonts.heading_size_px", default_heading_font_size_px() as f64)?
            .set_default("colors.foreground", Colors::default().foreground)?
            .set_default("colors.background", Colors::default().background)?
            .set_default("colors.neutral", Colors::default().neutral)?
            .set_default("colors.selected", Colors::default().selected)?
            .set_default("colors.error", Colors::default().error)?
            .set_default("ui.gap_below_session_px", default_gap_below_session_px())?
            .set_default("ui.gap_below_username_px", default_gap_below_username_px())?
            .set_default("ui.row_h", default_row_h())?
            .set_default("ui.password_char", default_password_char())?
            .set_default("ui.text_align", "center")?
            .set_default("ui.form_width", default_form_width())?
            .set_default("ui.form_height", default_form_height())?
            .set_default("ui.session_left_arrow", default_session_left_arrow())?
            .set_default("ui.session_right_arrow", default_session_right_arrow())?
            .add_source(
                config::File::from(std::path::Path::new(
                    "/etc/mflm/config.toml"
                ))
                .format(config::FileFormat::Toml)
                .required(false)
            );

        let cfg = builder.build()?;
        cfg.try_deserialize::<Self>()
    }

    pub fn resolve_colors(&self) -> Result<ResolvedColors, ParseColorError> {
        Ok(ResolvedColors {
            foreground: Color::from_hex(&self.colors.foreground)?,
            background: Color::from_hex(&self.colors.background)?,
            neutral: Color::from_hex(&self.colors.neutral)?,
            selected: Color::from_hex(&self.colors.selected)?,
            error: Color::from_hex(&self.colors.error)?
        })
    }
}
