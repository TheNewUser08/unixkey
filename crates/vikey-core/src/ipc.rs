use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::{CommitMode, Config};
use crate::syllable::{InputMethod, ToneStyle};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NiriWindow {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub is_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub enabled: bool,
    pub input_method: InputMethod,
    pub tone_style: ToneStyle,
    pub active_commit_mode: CommitMode,
    pub ime_grab_active: bool,
    pub focused_window: Option<NiriWindow>,
    pub config: Config,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "arg", rename_all = "snake_case")]
pub enum IpcRequest {
    GetStatus,
    Toggle,
    SetEnabled(bool),
    SetInputMethod(InputMethod),
    SetToneStyle(ToneStyle),
    SetCommitMode(CommitMode),
    UpdateConfig(Box<Config>),
    ReloadConfig,
    ListNiriWindows,
    Subscribe,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum IpcResponse {
    Status(Box<DaemonStatus>),
    Windows(Vec<NiriWindow>),
    Ok,
    Error(String),
}

pub fn runtime_socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("vikey.sock");
    }
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/tmp/vikey-{uid}.sock"))
}

unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}
