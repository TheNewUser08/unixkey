use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use calloop::channel::Sender;
use log::{debug, info, warn};
use serde::Deserialize;
use serde_json::Value;
use vikey_core::NiriWindow;

#[derive(Debug, Clone)]
pub enum NiriEvent {
    WindowsReplaced(Vec<NiriWindow>),
    WindowOpenedOrChanged(NiriWindow),
    WindowClosed(u64),
    FocusChanged(Option<u64>),
}

#[derive(Deserialize)]
struct RawNiriWindow {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    is_focused: bool,
}

impl From<RawNiriWindow> for NiriWindow {
    fn from(r: RawNiriWindow) -> Self {
        Self {
            id: r.id,
            app_id: r.app_id,
            title: r.title,
            is_focused: r.is_focused,
        }
    }
}

pub struct NiriTracker {
    windows: HashMap<u64, NiriWindow>,
    focused_id: Option<u64>,
}

impl Default for NiriTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl NiriTracker {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            focused_id: None,
        }
    }

    /// Applies a `NiriEvent` and returns `true` if the focused window (`id` or `app_id`) changed.
    pub fn apply_event(&mut self, ev: NiriEvent) -> bool {
        let prev_focus = self.focused_window().cloned();

        match ev {
            NiriEvent::WindowsReplaced(list) => {
                self.windows.clear();
                self.focused_id = None;
                for w in list {
                    if w.is_focused {
                        self.focused_id = Some(w.id);
                    }
                    self.windows.insert(w.id, w);
                }
            }
            NiriEvent::WindowOpenedOrChanged(w) => {
                if w.is_focused {
                    for existing in self.windows.values_mut() {
                        existing.is_focused = false;
                    }
                    self.focused_id = Some(w.id);
                }
                self.windows.insert(w.id, w);
            }
            NiriEvent::WindowClosed(id) => {
                self.windows.remove(&id);
                if self.focused_id == Some(id) {
                    self.focused_id = None;
                }
            }
            NiriEvent::FocusChanged(new_id) => {
                for existing in self.windows.values_mut() {
                    existing.is_focused = Some(existing.id) == new_id;
                }
                self.focused_id = new_id;
            }
        }

        let new_focus = self.focused_window();
        prev_focus.as_ref().map(|w| (&w.id, &w.app_id))
            != new_focus.map(|w| (&w.id, &w.app_id))
    }

    pub fn focused_window(&self) -> Option<&NiriWindow> {
        let id = self.focused_id?;
        self.windows.get(&id)
    }

    pub fn list_windows(&self) -> Vec<NiriWindow> {
        let mut list: Vec<NiriWindow> = self.windows.values().cloned().collect();
        list.sort_by_key(|w| (!w.is_focused, w.id));
        list
    }
}

/// Spawns a background thread that connects to `$NIRI_SOCKET` and streams `NiriEvent`s
/// into the daemon's `calloop` channel.
pub fn spawn_niri_listener<F, E>(tx: Sender<E>, map_event: F)
where
    F: Fn(NiriEvent) -> E + Send + 'static,
    E: Send + 'static,
{
    thread::Builder::new()
        .name("niri-ipc-listener".to_string())
        .spawn(move || {
            loop {
                let Ok(sock_path) = std::env::var("NIRI_SOCKET") else {
                    debug!("NIRI_SOCKET is not set; retrying in 5s");
                    thread::sleep(Duration::from_secs(5));
                    continue;
                };

                match UnixStream::connect(&sock_path) {
                    Ok(mut stream) => {
                        info!("Connected to Niri IPC socket at {sock_path}");
                        if let Err(e) = stream.write_all(b"\"EventStream\"\n") {
                            warn!("Failed to request Niri EventStream: {e}");
                            thread::sleep(Duration::from_secs(2));
                            continue;
                        }
                        let _ = stream.flush();

                        let reader = BufReader::new(stream);
                        for line in reader.lines() {
                            let Ok(line) = line else {
                                warn!("Niri IPC stream closed");
                                break;
                            };
                            if let Some(ev) = parse_niri_event_line(&line)
                                && tx.send(map_event(ev)).is_err()
                            {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Could not connect to NIRI_SOCKET ({sock_path}): {e}");
                    }
                }

                thread::sleep(Duration::from_secs(3));
            }
        })
        .expect("Failed to spawn niri-ipc-listener thread");
}

fn parse_niri_event_line(line: &str) -> Option<NiriEvent> {
    let val: Value = serde_json::from_str(line).ok()?;
    let obj = val.as_object()?;

    if let Some(inner) = obj.get("WindowsChanged") {
        let windows_val = inner.get("windows")?.clone();
        let raw_list: Vec<RawNiriWindow> = serde_json::from_value(windows_val).ok()?;
        return Some(NiriEvent::WindowsReplaced(
            raw_list.into_iter().map(Into::into).collect(),
        ));
    }

    if let Some(inner) = obj.get("WindowOpenedOrChanged") {
        let win_val = inner.get("window")?.clone();
        let raw_win: RawNiriWindow = serde_json::from_value(win_val).ok()?;
        return Some(NiriEvent::WindowOpenedOrChanged(raw_win.into()));
    }

    if let Some(inner) = obj.get("WindowClosed") {
        let id = inner.get("id")?.as_u64()?;
        return Some(NiriEvent::WindowClosed(id));
    }

    if let Some(inner) = obj.get("WindowFocusChanged") {
        let id = inner.get("id").and_then(Value::as_u64);
        return Some(NiriEvent::FocusChanged(id));
    }

    None
}
