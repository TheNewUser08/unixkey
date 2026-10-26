use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::buffer::EngineConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommitMode {
    /// Standard Wayland IME pre-edit buffer (native Wayland protocol).
    #[default]
    Preedit,
    /// Direct character commit without underline (UniKey / Bamboo style).
    /// Real-time IDE code recommendations and live search suggestions work while typing.
    #[serde(alias = "virtual_backspace")]
    Direct,
}

impl CommitMode {
    pub fn label(self) -> &'static str {
        match self {
            CommitMode::Preedit => "Pre-edit Buffer (Underlined)",
            CommitMode::Direct => "Direct Commit (Fast Autocomplete / No Underline)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            CommitMode::Preedit => {
                "Composes words in an underlined Wayland buffer. Standard & stable, but search bars and IDE recommendations wait for word completion."
            }
            CommitMode::Direct => {
                "Commits every letter immediately. Real-time IDE code completion and live search suggestions work while typing. Uses delete_surrounding_text (virtual-keyboard Backspace fallback)."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToggleShortcut {
    #[default]
    CtrlShift,
    AltZ,
    CtrlSpace,
    None,
}

impl ToggleShortcut {
    pub fn label(self) -> &'static str {
        match self {
            ToggleShortcut::CtrlShift => "Ctrl + Shift",
            ToggleShortcut::AltZ => "Alt + Z",
            ToggleShortcut::CtrlSpace => "Ctrl + Space",
            ToggleShortcut::None => "Disabled (Use Niri Keybind)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub name: String,
    pub app_id_regex: String,
    #[serde(default)]
    pub title_regex: Option<String>,
    /// If `Some(false)`, automatically switches to EN when this app is focused.
    /// If `Some(true)`, automatically switches to VN.
    #[serde(default)]
    pub default_enabled: Option<bool>,
    /// Optional per-app commit mode override.
    #[serde(default)]
    pub commit_mode: Option<CommitMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled_by_default: bool,
    pub dark_mode: bool,
    pub engine: EngineConfig,
    pub default_commit_mode: CommitMode,
    pub backspace_delay_ms: u64,
    pub toggle_shortcut: ToggleShortcut,
    pub remember_per_window: bool,
    pub enable_tray: bool,
    pub app_rules: Vec<AppRule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled_by_default: true,
            dark_mode: false,
            engine: EngineConfig::default(),
            default_commit_mode: CommitMode::Preedit,
            backspace_delay_ms: 10,
            toggle_shortcut: ToggleShortcut::CtrlShift,
            remember_per_window: true,
            enable_tray: true,
            app_rules: vec![
                AppRule {
                    name: "Terminal Emulators (Auto EN)".to_string(),
                    app_id_regex: "^(foot|footclient|Alacritty|kitty|org\\.wezfurlong\\.wezterm|com\\.mitchellh\\.ghostty|org\\.gnome\\.Ptyxis|org\\.gnome\\.Console)$".to_string(),
                    title_regex: None,
                    default_enabled: Some(false),
                    commit_mode: None,
                },
                AppRule {
                    name: "Steam & Games (Auto EN)".to_string(),
                    app_id_regex: "^(steam|steam_app_.*|gamescope)$".to_string(),
                    title_regex: None,
                    default_enabled: Some(false),
                    commit_mode: None,
                },
                AppRule {
                    name: "Browsers & Electron".to_string(),
                    app_id_regex: "^(zen|zen-.*|app\\.zen_browser\\.zen|firefox|org\\.mozilla\\.firefox|chromium.*|google-chrome.*|brave.*|code|vesktop|discord|slack|obsidian)$".to_string(),
                    title_regex: None,
                    default_enabled: None,
                    commit_mode: None,
                },
            ],
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            PathBuf::from(xdg)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            return PathBuf::from("/tmp/unixkey-config.toml");
        };

        let new_path = base.join("unixkey").join("config.toml");
        let old_path = base.join("vikey").join("config.toml");

        if new_path.exists() {
            return new_path;
        }
        if old_path.exists() {
            // Attempt automatic copy to new path for seamless migration
            if let Some(parent) = new_path.parent() {
                let _ = fs::create_dir_all(parent);
                let _ = fs::copy(&old_path, &new_path);
            }
            if new_path.exists() {
                return new_path;
            }
            return old_path;
        }
        new_path
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path)
            && let Ok(cfg) = toml::from_str::<Config>(&contents)
        {
            return cfg;
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            PathBuf::from(xdg)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            PathBuf::from("/tmp")
        };
        let path = base.join("unixkey").join("config.toml");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, data).map_err(|e| e.to_string())
    }
}
