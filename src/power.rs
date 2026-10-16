// Power actions (shutdown, reboot, ...) shown in a row below the form and
// triggered with F-keys.

use std::process::{Command, Stdio};

use log::{info, warn};
use thiserror::Error;

use crate::settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Shutdown,
    Reboot,
    Suspend,
    Hibernate
}

impl Kind {
    fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "shutdown" => Some(Kind::Shutdown),
            "reboot" => Some(Kind::Reboot),
            "suspend" => Some(Kind::Suspend),
            "hibernate" => Some(Kind::Hibernate),
            _ => None
        }
    }

    fn default_label(self) -> &'static str {
        match self {
            Kind::Shutdown => "Shutdown",
            Kind::Reboot => "Reboot",
            Kind::Suspend => "Suspend",
            Kind::Hibernate => "Hibernate"
        }
    }

    fn default_command(self) -> Vec<String> {
        let verb = match self {
            Kind::Shutdown => "poweroff",
            Kind::Reboot => "reboot",
            Kind::Suspend => "suspend",
            Kind::Hibernate => "hibernate"
        };
        vec!["systemctl".to_string(), verb.to_string()]
    }
}

#[derive(Error, Debug)]
pub enum PowerError {
    #[error("failed to run {command:?}: {source}")]
    Spawn {
        command: Vec<String>,
        source: std::io::Error
    },
    #[error("{command:?} exited with {status}: {stderr}")]
    Failed {
        command: Vec<String>,
        status: std::process::ExitStatus,
        stderr: String
    }
}

#[derive(Debug, Clone)]
pub struct Action {
    pub kind: Kind,
    /// F-key number, 1-12.
    pub key: u8,
    pub label: String,
    pub icon: String,
    /// Never empty.
    pub command: Vec<String>
}

impl Action {
    pub fn text(&self) -> String {
        let key = self.key;
        match (self.icon.as_str(), self.label.as_str()) {
            ("", "") => format!("[F{key}]"),
            (icon, "") => format!("[F{key}] {icon}"),
            ("", label) => format!("[F{key}] {label}"),
            (icon, label) => format!("[F{key}] {icon}  {label}")
        }
    }

    pub fn run(&self) -> Result<(), PowerError> {
        info!("Running power action {:?}: {:?}", self.kind, self.command);
        // stdin is null so nothing (e.g. a polkit tty agent) can read the
        // greeter's tty
        let output = Command::new(&self.command[0])
            .args(&self.command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|source| PowerError::Spawn {
                command: self.command.clone(),
                source
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(PowerError::Failed {
                command: self.command.clone(),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr)
                    .trim()
                    .to_string()
            })
        }
    }
}

/// An action plus its rendered text and measured size.
pub struct Item {
    pub action: Action,
    pub text: String,
    /// Cell width including padding.
    pub w: u32,
    pub text_h: u32
}

fn parse_key(key: &str) -> Option<u8> {
    let key = key.trim();
    let n = key
        .strip_prefix('F')
        .or_else(|| key.strip_prefix('f'))?
        .parse::<u8>()
        .ok()?;
    (1..=12).contains(&n).then_some(n)
}

/// Validates the configured actions, skipping bad entries with a warning.
pub fn resolve(cfg: &[settings::PowerAction]) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    for (i, entry) in cfg.iter().enumerate() {
        let kind = match Kind::from_name(&entry.kind) {
            Some(kind) => kind,
            None => {
                warn!("Skipping power.action[{i}]: unknown kind {:?}", entry.kind);
                continue;
            }
        };
        let key = match parse_key(&entry.key) {
            Some(key) => key,
            None => {
                warn!(
                    "Skipping power.action[{i}]: key must be F1-F12, got {:?}",
                    entry.key
                );
                continue;
            }
        };
        if actions.iter().any(|a| a.key == key) {
            warn!("Skipping power.action[{i}]: F{key} is already bound");
            continue;
        }
        let command = entry
            .command
            .clone()
            .unwrap_or_else(|| kind.default_command());
        if command.first().map_or(true, |c| c.trim().is_empty()) {
            warn!("Skipping power.action[{i}]: empty command");
            continue;
        }
        actions.push(Action {
            kind,
            key,
            label: entry
                .label
                .clone()
                .unwrap_or_else(|| kind.default_label().to_string()),
            icon: entry.icon.trim().to_string(),
            command
        });
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, key: &str) -> settings::PowerAction {
        settings::PowerAction {
            kind: kind.to_string(),
            key: key.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn keys() {
        assert_eq!(parse_key("F1"), Some(1));
        assert_eq!(parse_key(" f12 "), Some(12));
        assert_eq!(parse_key("F0"), None);
        assert_eq!(parse_key("F13"), None);
        assert_eq!(parse_key("1"), None);
    }

    #[test]
    fn defaults() {
        let a = &resolve(&[entry("Shutdown", "F1")])[0];
        assert_eq!(a.kind, Kind::Shutdown);
        assert_eq!(a.label, "Shutdown");
        assert_eq!(a.command, ["systemctl", "poweroff"]);
        assert_eq!(a.text(), "[F1] Shutdown");
    }

    #[test]
    fn skips_bad_entries() {
        let mut empty_cmd = entry("reboot", "F3");
        empty_cmd.command = Some(vec![]);
        let actions = resolve(&[
            entry("shutdown", "F1"),
            entry("explode", "F2"),
            entry("reboot", "F1"),
            empty_cmd,
            entry("suspend", "F13"),
            entry("suspend", "F4")
        ]);
        let kinds: Vec<_> = actions.iter().map(|a| (a.kind, a.key)).collect();
        assert_eq!(kinds, [(Kind::Shutdown, 1), (Kind::Suspend, 4)]);
    }

    #[test]
    fn text_with_icon() {
        let mut e = entry("reboot", "F2");
        e.icon = "R".to_string();
        assert_eq!(resolve(&[e.clone()])[0].text(), "[F2] R  Reboot");
        e.label = Some(String::new());
        assert_eq!(resolve(&[e])[0].text(), "[F2] R");
    }
}
