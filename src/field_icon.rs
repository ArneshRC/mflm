// Optional icons left of the username and password fields, e.g. Nerd Font
// glyphs. Off unless configured.

use log::warn;

use crate::draw::Font;

#[derive(Debug, Clone)]
pub struct FieldIcon {
    pub text: String,
    /// Offset from the top of the row that puts the icon's baseline on the
    /// field text's baseline.
    pub dy: u32
}

#[derive(Debug, Default)]
pub struct FieldIcons {
    pub username: Option<FieldIcon>,
    pub password: Option<FieldIcon>,
    /// Width of the widest icon plus the gap after it. 0 without icons.
    pub col_w: u32,
    /// How far field text moves down so an icon taller than the text still
    /// fits in the row.
    pub text_dy: u32
}

impl FieldIcons {
    pub fn is_empty(&self) -> bool {
        self.username.is_none() && self.password.is_none()
    }
}

struct Measured {
    text: String,
    w: u32,
    baseline: u32
}

fn codepoints(s: &str) -> String {
    s.chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

fn measure(field: &str, icon: &str, font: &Font) -> Option<Measured> {
    let icon = icon.trim();
    if icon.is_empty() {
        return None;
    }

    let measured = font.missing_glyphs(icon).and_then(|missing| {
        let (w, _) = font.measure(icon)?;
        Ok((missing, w, font.baseline(icon)?))
    });
    match measured {
        Ok((0, w, baseline)) => Some(Measured {
            text: icon.to_string(),
            w,
            baseline
        }),
        Ok(_) => {
            warn!(
                "{field} icon ({}) is not in any installed font; not drawing it. Fonts must be installed system-wide for the greeter user",
                codepoints(icon)
            );
            None
        }
        Err(e) => {
            warn!("Unable to measure {field} icon; not drawing it: {e}");
            None
        }
    }
}

/// Measures the configured icons against `text_font`, the font of the field
/// text. Icons that are empty or have no font providing them are dropped.
pub fn resolve(
    username: &str,
    password: &str,
    gap_px: u32,
    icon_font: &Font,
    text_font: &Font,
    show_username: bool
) -> FieldIcons {
    let username = if show_username {
        measure("Username", username, icon_font)
    } else {
        None
    };
    let password = measure("Password", password, icon_font);
    if username.is_none() && password.is_none() {
        return FieldIcons::default();
    }

    let text_baseline = match text_font.baseline("Ag") {
        Ok(b) => b,
        Err(e) => {
            warn!("Unable to measure field text; not drawing icons: {e}");
            return FieldIcons::default();
        }
    };

    let icons = || username.iter().chain(password.iter());
    let icon_w = icons().map(|m| m.w).max().unwrap_or(0);
    let text_dy = icons()
        .map(|m| m.baseline.saturating_sub(text_baseline))
        .max()
        .unwrap_or(0);

    // text_dy >= m.baseline - text_baseline, so this never underflows
    let place = |m: Measured| FieldIcon {
        dy: text_dy + text_baseline - m.baseline,
        text: m.text
    };

    FieldIcons {
        username: username.map(place),
        password: password.map(place),
        col_w: icon_w.saturating_add(gap_px),
        text_dy
    }
}

