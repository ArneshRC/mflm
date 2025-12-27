use serde::Deserialize;

use crate::color::{Color, ParseColorError};

#[derive(Debug, Clone, Deserialize)]
pub struct Fonts {
    /// Pango font description string for the main UI font.
    /// Example: "Roboto Regular" or "Sans 12".
    pub main: String,
    /// Pango font description string for the monospace UI font.
    /// Example: "DejaVu Sans Mono".
    pub mono: String
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
            main: "Sans".to_string(),
            mono: "Monospace".to_string()
        }
    }
}

#[derive(Default, Debug, Clone, Deserialize)]
pub struct Login {
    /// Optional session target name to force.
    pub target: Option<String>,

    /// Optional username to force.
    pub username: Option<String>
}

fn default_gap_px() -> u32 {
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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ui {
    #[serde(default = "default_gap_px")]
    pub gap_px: u32,

    #[serde(default = "default_row_h")]
    pub row_h: u32,

    #[serde(default = "default_password_char")]
    pub password_char: String,

    #[serde(default = "default_text_align")]
    pub text_align: TextAlign
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            gap_px: default_gap_px(),
            row_h: default_row_h(),
            password_char: default_password_char(),
            text_align: default_text_align()
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
            .set_default("fonts.mono", Fonts::default().mono)?
            .set_default("colors.foreground", Colors::default().foreground)?
            .set_default("colors.background", Colors::default().background)?
            .set_default("colors.neutral", Colors::default().neutral)?
            .set_default("colors.selected", Colors::default().selected)?
            .set_default("colors.error", Colors::default().error)?
            // New UI category (preferred)
            .set_default("ui.gap_px", default_gap_px())?
            .set_default("ui.row_h", default_row_h())?
            .set_default("ui.password_char", default_password_char())?
            .set_default("ui.text_align", "center")?
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
