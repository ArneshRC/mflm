use serde::Deserialize;

use crate::color::{Color, ParseColorError};

/// `{ family, size }`. Missing keys fall back to the element's parent font,
/// see the `*_font` methods on `Settings`.
#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Font {
    /// Pango font description, e.g. "DejaVu Sans Mono" or "Sans Bold".
    pub family: Option<String>,
    /// Pixels.
    pub size: Option<f32>
}

#[derive(Debug, Clone)]
pub struct ResolvedFont {
    pub family: String,
    pub size: f32
}

impl Font {
    fn resolve(&self, family: &str, size: f32) -> ResolvedFont {
        ResolvedFont {
            family: self.family.as_deref().unwrap_or(family).to_string(),
            size: self.size.unwrap_or(size)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Login {
    /// Blank the screen once login succeeds.
    pub clear_screen_on_success: bool
}

impl Default for Login {
    fn default() -> Self {
        Self {
            clear_screen_on_success: true
        }
    }
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Default font for everything. Defaults to Monospace, 42px.
    pub font: Font,
    pub colors: Colors
}

/// Hex: "#RRGGBB" or "#AARRGGBB".
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub text: String,
    pub background: String,
    /// Unfocused underlines, info status and the power hint.
    pub muted: String,
    /// Focused field and active session.
    pub accent: String,
    /// Errors (e.g. auth failure).
    pub error: String,
    /// Background of a power action awaiting confirmation. Defaults to
    /// `accent`.
    pub confirm: Option<String>
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            text: "#fafafa".to_string(),
            background: "#000000".to_string(),
            muted: "#cccccc".to_string(),
            accent: "#aaccee".to_string(),
            error: "#ecbcad".to_string(),
            confirm: None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedColors {
    pub text: Color,
    pub background: Color,
    pub muted: Color,
    pub accent: Color,
    pub error: Color,
    pub confirm: Color
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Header {
    /// Supports placeholders: {hostname}. Use {{ and }} for literal braces.
    pub text: String,
    /// Defaults to Sans, 72px.
    pub font: Font,
    /// Gap between the top of the screen and the heading.
    pub offset: u32
}

impl Default for Header {
    fn default() -> Self {
        Self {
            text: "Welcome to {hostname}".to_string(),
            font: Font::default(),
            offset: 24
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
pub struct Form {
    pub width: u32,
    pub row_height: u32,
    pub text_align: TextAlign,
    /// Horizontal inset of the text in a row.
    pub padding: u32,
    /// Defaults to `theme.font`.
    pub font: Font,
    pub icons: Icons,
    pub session: Session,
    pub username: Username,
    pub password: Password
}

impl Default for Form {
    fn default() -> Self {
        Self {
            width: 512,
            row_height: 72,
            text_align: TextAlign::Center,
            padding: 15,
            font: Font::default(),
            icons: Icons::default(),
            session: Session::default(),
            username: Username::default(),
            password: Password::default()
        }
    }
}

/// Shared settings for the username and password icons.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Icons {
    /// Defaults to `form.font`.
    pub font: Font,
    /// Gap between an icon and its field's text.
    pub gap: u32
}

impl Default for Icons {
    fn default() -> Self {
        Self {
            font: Font::default(),
            gap: 24
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Session name to preselect.
    pub default: Option<String>,
    /// Hide the row and always use `default`.
    pub locked: bool,
    /// Left and right arrow around the session name.
    pub arrows: [String; 2],
    pub gap_below: u32
}

impl Default for Session {
    fn default() -> Self {
        Self {
            default: None,
            locked: false,
            arrows: ["❮".to_string(), "❯".to_string()],
            gap_below: 64
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Username {
    /// Username to prefill.
    pub default: Option<String>,
    /// Hide the row and always use `default`.
    pub locked: bool,
    /// Usually a Nerd Font glyph such as "". Empty means no icon.
    pub icon: String,
    pub gap_below: u32
}

impl Default for Username {
    fn default() -> Self {
        Self {
            default: None,
            locked: false,
            icon: String::new(),
            gap_below: 32
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Password {
    /// E.g. "". Empty means no icon.
    pub icon: String,
    /// Drawn once per typed character.
    pub mask: String
}

impl Default for Password {
    fn default() -> Self {
        Self {
            icon: String::new(),
            mask: "•".to_string()
        }
    }
}

/// Line below the form showing "Logging in…", "Login failed" and similar.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Status {
    /// When false, errors flash the form in the error color instead.
    pub enabled: bool,
    /// Defaults to `theme.font` at 18px.
    pub font: Font,
    /// Gap between the bottom of the form and the line.
    pub offset: u32
}

impl Default for Status {
    fn default() -> Self {
        Self {
            enabled: true,
            font: Font::default(),
            offset: 48
        }
    }
}

/// One entry of `[[power.actions]]`. Fields are loosely typed so a bad entry
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
    /// Defaults to `theme.font` at 18px.
    pub font: Font,
    /// Horizontal gap between actions.
    pub gap: u32,
    /// Gap between the bottom of the form and the row.
    pub offset: u32,
    pub hint: PowerHint,
    /// Actions in on-screen order. Empty means no power row.
    pub actions: Vec<PowerAction>
}

impl Default for Power {
    fn default() -> Self {
        Self {
            font: Font::default(),
            gap: 32,
            offset: 240,
            hint: PowerHint::default(),
            actions: Vec::new()
        }
    }
}

/// "[Enter] Accept / [Esc] Cancel" under an armed action.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PowerHint {
    /// Defaults to `power.font` at 70% of its size.
    pub font: Font,
    /// Gap between the power row and the hint.
    pub offset: u32
}

impl Default for PowerHint {
    fn default() -> Self {
        Self {
            font: Font::default(),
            offset: 8
        }
    }
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub login: Login,
    pub theme: Theme,
    pub header: Header,
    pub form: Form,
    pub status: Status,
    pub power: Power
}

impl Settings {
    /// Loads configuration from /etc/mflm/config.toml. Missing keys fall back
    /// to the `Default` impls above.
    pub fn load() -> Result<Self, config::ConfigError> {
        Self::from_source(
            config::File::from(std::path::Path::new("/etc/mflm/config.toml"))
                .format(config::FileFormat::Toml)
                .required(false)
        )
    }

    fn from_source<S>(source: S) -> Result<Self, config::ConfigError>
    where
        S: config::Source + Send + Sync + 'static
    {
        config::Config::builder()
            .add_source(source)
            .build()?
            .try_deserialize::<Self>()
    }

    pub fn theme_font(&self) -> ResolvedFont {
        self.theme.font.resolve("Monospace", 42.0)
    }

    pub fn header_font(&self) -> ResolvedFont {
        self.header.font.resolve("Sans", 72.0)
    }

    pub fn form_font(&self) -> ResolvedFont {
        let theme = self.theme_font();
        self.form.font.resolve(&theme.family, theme.size)
    }

    pub fn icon_font(&self) -> ResolvedFont {
        let form = self.form_font();
        self.form.icons.font.resolve(&form.family, form.size)
    }

    pub fn status_font(&self) -> ResolvedFont {
        self.status.font.resolve(&self.theme_font().family, 18.0)
    }

    pub fn power_font(&self) -> ResolvedFont {
        self.power.font.resolve(&self.theme_font().family, 18.0)
    }

    pub fn power_hint_font(&self) -> ResolvedFont {
        let power = self.power_font();
        self.power.hint.font.resolve(&power.family, power.size * 0.7)
    }

    pub fn resolve_colors(&self) -> Result<ResolvedColors, ParseColorError> {
        let colors = &self.theme.colors;
        let accent = Color::from_hex(&colors.accent)?;
        Ok(ResolvedColors {
            text: Color::from_hex(&colors.text)?,
            background: Color::from_hex(&colors.background)?,
            muted: Color::from_hex(&colors.muted)?,
            accent,
            error: Color::from_hex(&colors.error)?,
            confirm: match &colors.confirm {
                Some(c) => Color::from_hex(c)?,
                None => accent
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Settings {
        Settings::from_source(config::File::from_str(
            toml,
            config::FileFormat::Toml
        ))
        .unwrap()
    }

    #[test]
    fn fonts_inherit() {
        let s = parse(
            r#"
            [theme]
            font = { family = "Mono", size = 30 }
            [header]
            font = { family = "Muli" }
            [form.icons]
            font = { family = "Symbols" }
            [power]
            font = { size = 20 }
            "#
        );
        assert_eq!(s.header_font().family, "Muli");
        assert_eq!(s.header_font().size, 72.0);
        assert_eq!(s.form_font().family, "Mono");
        assert_eq!(s.icon_font().family, "Symbols");
        assert_eq!(s.icon_font().size, 30.0);
        assert_eq!(s.status_font().family, "Mono");
        assert_eq!(s.status_font().size, 18.0);
        assert_eq!(s.power_hint_font().family, "Mono");
        assert_eq!(s.power_hint_font().size, 14.0);
    }

    #[test]
    fn nested_sections() {
        let s = parse(
            r#"
            [form.session]
            default = "Niri"
            locked = true
            arrows = ["<", ">"]
            [form.username]
            default = "me"
            [form.password]
            mask = "*"
            [power.hint]
            offset = 4
            [[power.actions]]
            kind = "reboot"
            key = "F2"
            "#
        );
        assert_eq!(s.form.session.default.as_deref(), Some("Niri"));
        assert!(s.form.session.locked);
        assert_eq!(s.form.session.arrows[1], ">");
        assert_eq!(s.form.session.gap_below, 64);
        assert_eq!(s.form.username.default.as_deref(), Some("me"));
        assert!(!s.form.username.locked);
        assert_eq!(s.form.password.mask, "*");
        assert_eq!(s.power.hint.offset, 4);
        assert_eq!(s.power.actions[0].key, "F2");
    }
}
