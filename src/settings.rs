use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Fonts {
    /// Pango font description string for the main UI font.
    /// Example: "Roboto Regular" or "Sans 12".
    pub main: String,
    /// Pango font description string for the monospace UI font.
    /// Example: "DejaVu Sans Mono".
    pub mono: String,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            main: "Sans".to_string(),
            mono: "Monospace".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub fonts: Fonts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fonts: Fonts::default(),
        }
    }
}

impl Settings {
    /// Loads configuration from /etc/mflm/config.toml
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder()
            .set_default("fonts.main", Fonts::default().main)?
            .set_default("fonts.mono", Fonts::default().mono)?
            .add_source(
                config::File::from(std::path::Path::new("/etc/mflm/config.toml"))
                    .format(config::FileFormat::Toml)
                    .required(false),
            );

        let cfg = builder.build()?;
        cfg.try_deserialize::<Self>()
    }
}
