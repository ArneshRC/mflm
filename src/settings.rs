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

#[derive(Debug, Clone)]
pub struct Ui {
    pub gap_px: u32,
    pub row_h: u32,
    pub password_char: String,
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

// Raw structs used to preserve backward compatibility with legacy [login]
// keys (gap_px/row_h/password_char) while moving these to [ui].
#[derive(Debug, Clone, Default, Deserialize)]
struct LoginRaw {
    pub target: Option<String>,
    pub username: Option<String>,

    #[serde(default)]
    pub gap_px: Option<u32>,

    #[serde(default)]
    pub row_h: Option<u32>,

    #[serde(default)]
    pub password_char: Option<String>
}

#[derive(Debug, Clone, Default, Deserialize)]
struct UiRaw {
    #[serde(default)]
    pub gap_px: Option<u32>,

    #[serde(default)]
    pub row_h: Option<u32>,

    #[serde(default)]
    pub password_char: Option<String>,

    #[serde(default)]
    pub text_align: Option<TextAlign>
}


#[derive(Default, Debug, Clone)]
pub struct Settings {
    pub fonts: Fonts,
    pub colors: Colors,

    pub login: Login,
    pub ui: Ui
}


#[derive(Default, Debug, Clone, Deserialize)]
struct SettingsRaw {
    #[serde(default)]
    pub fonts: Fonts,

    #[serde(default)]
    pub colors: Colors,

    #[serde(default)]
    pub login: LoginRaw,

    #[serde(default)]
    pub ui: UiRaw
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
            // Legacy keys kept for compatibility (if /etc config still uses [login])
            .set_default("login.gap_px", default_gap_px())?
            .set_default("login.row_h", default_row_h())?
            .set_default("login.password_char", default_password_char())?
            .add_source(
                config::File::from(std::path::Path::new(
                    "/etc/mflm/config.toml"
                ))
                .format(config::FileFormat::Toml)
                .required(false)
            );

        let cfg = builder.build()?;

        let raw = cfg.try_deserialize::<SettingsRaw>()?;

        let gap_px = raw
            .ui
            .gap_px
            .or(raw.login.gap_px)
            .unwrap_or_else(default_gap_px);
        let row_h = raw
            .ui
            .row_h
            .or(raw.login.row_h)
            .unwrap_or_else(default_row_h);
        let password_char = raw
            .ui
            .password_char
            .or(raw.login.password_char)
            .unwrap_or_else(default_password_char);
        let password_char = {
            let s = password_char.trim();
            if s.is_empty() {
                default_password_char()
            } else {
                s.to_string()
            }
        };
        let text_align = raw.ui.text_align.unwrap_or_else(default_text_align);

        Ok(Self {
            fonts: raw.fonts,
            colors: raw.colors,
            login: Login {
                target: raw.login.target,
                username: raw.login.username
            },
            ui: Ui {
                gap_px,
                row_h,
                password_char,
                text_align
            }
        })
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
