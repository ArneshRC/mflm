#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    red: f32,
    green: f32,
    blue: f32,
    opacity: f32,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseColorError {
    #[error("invalid hex color length ({len}); expected 6 (RRGGBB) or 8 (AARRGGBB)")]
    InvalidLength { len: usize },

    #[error("invalid hex color: {0}")]
    InvalidHex(String),
}

const fn rgb(red: f32, green: f32, blue: f32) -> Color {
    Color {
        red,
        green,
        blue,
        opacity: 1.0,
    }
}

impl Color {
    pub const BLACK: Self = rgb(0.0, 0.0, 0.0);
    pub const GRAY: Self = rgb(0.75, 0.75, 0.75);
    pub const WHITE: Self = rgb(1.0, 1.0, 1.0);
    pub const RED: Self = rgb(0.75, 0.25, 0.25);
    pub const YELLOW: Self = rgb(0.75, 0.75, 0.25);

    pub fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            red: (r as f32) / 255.0,
            green: (g as f32) / 255.0,
            blue: (b as f32) / 255.0,
            opacity: (a as f32) / 255.0,
        }
    }

    /// Parses "#RRGGBB", "RRGGBB", "#AARRGGBB", or "AARRGGBB".
    pub fn from_hex(s: &str) -> Result<Self, ParseColorError> {
        let hex = s.trim().trim_start_matches('#');
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                Ok(Self::from_rgba_u8(r, g, b, 0xFF))
            }
            8 => {
                let a = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                let r = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                let g = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                let b = u8::from_str_radix(&hex[6..8], 16)
                    .map_err(|_| ParseColorError::InvalidHex(s.to_string()))?;
                Ok(Self::from_rgba_u8(r, g, b, a))
            }
            len => Err(ParseColorError::InvalidLength { len }),
        }
    }

    pub fn blend(&self, other: &Color, ratio: f32) -> Self {
        let ratio = ratio.clamp(0.0, 1.0);

        Self {
            red: self.red + ((other.red - self.red) * ratio),
            green: self.green + ((other.green - self.green) * ratio),
            blue: self.blue + ((other.blue - self.blue) * ratio),
            opacity: self.opacity + ((other.opacity - self.opacity) * ratio),
        }
    }

    pub fn as_argb8888(&self) -> u32 {
        let argb = [self.opacity, self.red, self.green, self.blue];
        u32::from_be_bytes(argb.map(|x| (x * 255.0) as u8))
    }

    pub fn as_rgba_f32(&self) -> (f64, f64, f64, f64) {
        (self.red as f64, self.green as f64, self.blue as f64, self.opacity as f64)
    }
}
