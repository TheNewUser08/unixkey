use std::collections::{HashMap, HashSet};
use std::os::fd::AsFd;
use std::process::Command;
use std::thread;
use std::time::Duration;

use regex::Regex;
use unixkey_core::{
    CommitMode, Config, DaemonStatus, InputMethod, IpcRequest, IpcResponse, WordBuffer,
};
use wayland_client::protocol::wl_keyboard::KeyState;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ContentHint, ContentPurpose,
};
use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_keyboard_grab_v2::{
    self, ZwpInputMethodKeyboardGrabV2,
};
use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::{
    self, ZwpInputMethodV2,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

use crate::ipc_server::{IpcCommand, SubscriberRegistry};
use crate::niri::{NiriEvent, NiriTracker};
use crate::tray::{TrayAction, TrayHandle, TrayState};
use crate::xkb::{EVDEV_KEY_BACKSPACE, EVDEV_KEY_ENTER, EVDEV_KEY_TAB, XkbTracker};

struct CompiledAppRule {
    app_id_re: Regex,
    title_re: Option<Regex>,
    default_enabled: Option<bool>,
    commit_mode: Option<CommitMode>,
}

pub struct DaemonState {
    pub conn: Connection,
    pub should_quit: bool,
    pub config: Config,
    compiled_rules: Vec<CompiledAppRule>,

    // Wayland globals & objects
    pub seat: Option<WlSeat>,
    pub im_manager: Option<ZwpInputMethodManagerV2>,
    pub vk_manager: Option<ZwpVirtualKeyboardManagerV1>,
    pub input_method: Option<ZwpInputMethodV2>,
    pub virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    pub keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,
    pub vk_has_keymap: bool,
    pub unavailable_retries: u32,

    // Double-buffered zwp_input_method_v2 state
    pub done_serial: u32,
    pub pending_active: bool,
    pub pending_activated_in_batch: bool,
    pub active: bool,
    pub pending_has_surrounding: bool,
    pub has_surrounding: bool,
    pub pending_sensitive: bool,
    pub bypass_sensitive: bool,

    // Keyboard & composition state
    pub xkb: XkbTracker,
    pub word_buffer: WordBuffer,
    pub swallowed_keys: HashSet<u32>,
    pub enabled: bool,
    pub active_commit_mode: CommitMode,

    // Key repeat state
    pub repeat_rate: i32,
    pub repeat_delay: i32,
    pub repeating_key: Option<(u32, u32)>, // (evdev_key, last_time_ms)

    // Niri window tracking & per-window memory
    pub niri: NiriTracker,
    pub window_lang_memory: HashMap<u64, bool>,

    // Tray & IPC subscribers
    pub tray: Option<TrayHandle>,
    pub subscribers: SubscriberRegistry,
}

impl DaemonState {
    pub fn new(conn: Connection, config: Config, tray: Option<TrayHandle>) -> anyhow::Result<Self> {
        let xkb = XkbTracker::new()?;
        let enabled = config.enabled_by_default;
        let active_commit_mode = config.default_commit_mode;
        let compiled_rules = compile_rules(&config);

        Ok(Self {
            conn,
            should_quit: false,
            config,
            compiled_rules,
            seat: None,
            im_manager: None,
            vk_manager: None,
            input_method: None,
            virtual_keyboard: None,
            keyboard_grab: None,
            vk_has_keymap: false,
            unavailable_retries: 0,
            done_serial: 0,
            pending_active: false,
            pending_activated_in_batch: false,
            active: false,
            pending_has_surrounding: false,
            has_surrounding: false,
            pending_sensitive: false,
            bypass_sensitive: false,
            xkb,
            word_buffer: WordBuffer::new(),
            swallowed_keys: HashSet::new(),
            enabled,
            active_commit_mode,
            repeat_rate: 30,
            repeat_delay: 400,
            repeating_key: None,
            niri: NiriTracker::new(),
            window_lang_memory: HashMap::new(),
            tray,
            subscribers: SubscriberRegistry::default(),
        })
    }

    pub fn bind_ime_objects_if_ready(&mut self, qh: &QueueHandle<Self>) {
        let (Some(seat), Some(im_mgr), Some(vk_mgr)) =
            (&self.seat, &self.im_manager, &self.vk_manager)
        else {
            return;
        };

        if self.virtual_keyboard.is_none() {
            let vk = vk_mgr.create_virtual_keyboard(seat, qh, ());
            self.virtual_keyboard = Some(vk);
            eprintln!("[unixkey-daemon] Bound zwp_virtual_keyboard_v1 on seat");
        }

        if self.input_method.is_none() {
            let im = im_mgr.get_input_method(seat, qh, ());
            self.input_method = Some(im);
            eprintln!("[unixkey-daemon] Bound zwp_input_method_v2 on seat");
        }
    }

    pub fn status_snapshot(&self) -> DaemonStatus {
        DaemonStatus {
            enabled: self.enabled,
            input_method: self.config.engine.input_method,
            tone_style: self.config.engine.tone_style,
            active_commit_mode: self.active_commit_mode,
            ime_grab_active: self.keyboard_grab.is_some(),
            focused_window: self.niri.focused_window().cloned(),
            config: self.config.clone(),
        }
    }

    pub fn notify_state_changed(&self) {
        if let Some(tray) = &self.tray {
            tray.update(TrayState {
                enabled: self.enabled,
                input_method: self.config.engine.input_method,
                tone_style: self.config.engine.tone_style,
            });
        }
        self.subscribers.broadcast_status(&self.status_snapshot());
    }

    pub fn flush_and_reset_buffer(&mut self) {
        if !self.word_buffer.is_empty() {
            if let Some(im) = &self.input_method {
                let (final_word, _) = self.word_buffer.finalize_word_full(&self.config.engine);
                im.set_preedit_string(String::new(), 0, 0);
                if !final_word.is_empty() {
                    im.commit_string(final_word);
                }
                im.commit(self.done_serial);
                let _ = self.conn.flush();
            } else {
                self.word_buffer.clear();
            }
        }
        self.repeating_key = None;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.flush_and_reset_buffer();
            self.enabled = enabled;
        }
        self.config.enabled_by_default = self.enabled;
        let _ = self.config.save();

        if self.config.remember_per_window
            && let Some(win) = self.niri.focused_window()
        {
            if win.app_id.as_deref() == Some("unixkey-gui") || win.app_id.as_deref() == Some("vikey-gui") {
                // Toggled from inside the GUI control window: apply to all remembered windows
                for val in self.window_lang_memory.values_mut() {
                    *val = self.enabled;
                }
            } else {
                self.window_lang_memory.insert(win.id, self.enabled);
            }
        }
        self.notify_state_changed();
    }

    pub fn handle_niri_event(&mut self, ev: NiriEvent) {
        let focus_changed = self.niri.apply_event(ev);
        if !focus_changed {
            return;
        }

        self.flush_and_reset_buffer();

        // Don't switch language state just because the user focused unixkey-gui itself
        let app_id = self.niri.focused_window().and_then(|w| w.app_id.as_deref());
        if app_id == Some("unixkey-gui") || app_id == Some("vikey-gui") {
            self.notify_state_changed();
            return;
        }

        let mut new_commit_mode = self.config.default_commit_mode;
        let mut rule_default_enabled: Option<bool> = None;

        if let Some(win) = self.niri.focused_window() {
            let app_id = win.app_id.as_deref().unwrap_or("");
            let title = win.title.as_deref().unwrap_or("");

            for rule in &self.compiled_rules {
                if rule.app_id_re.is_match(app_id) {
                    let title_matches = rule
                        .title_re
                        .as_ref()
                        .is_none_or(|re| re.is_match(title));
                    if title_matches {
                        if let Some(cm) = rule.commit_mode {
                            new_commit_mode = cm;
                        }
                        if let Some(en) = rule.default_enabled {
                            rule_default_enabled = Some(en);
                        }
                        break;
                    }
                }
            }

            if self.config.remember_per_window {
                if let Some(&remembered) = self.window_lang_memory.get(&win.id) {
                    self.enabled = remembered;
                } else {
                    let initial_en = rule_default_enabled.unwrap_or(self.config.enabled_by_default);
                    self.enabled = initial_en;
                    self.window_lang_memory.insert(win.id, initial_en);
                }
            } else if let Some(en) = rule_default_enabled {
                self.enabled = en;
            }
        }

        self.active_commit_mode = new_commit_mode;
        self.notify_state_changed();
    }

    pub fn handle_tray_action(&mut self, action: TrayAction) {
        match action {
            TrayAction::ToggleEnabled => {
                self.set_enabled(!self.enabled);
            }
            TrayAction::SetInputMethod(im) => {
                self.flush_and_reset_buffer();
                self.config.engine.input_method = im;
                let _ = self.config.save();
                self.notify_state_changed();
            }
            TrayAction::SetToneStyle(ts) => {
                self.flush_and_reset_buffer();
                self.config.engine.tone_style = ts;
                let _ = self.config.save();
                self.notify_state_changed();
            }
            TrayAction::OpenSettings => {}
            TrayAction::Quit => {
                self.should_quit = true;
            }
        }
    }

    pub fn handle_ipc_command(&mut self, cmd: IpcCommand) {
        let resp = match cmd.req {
            IpcRequest::GetStatus => IpcResponse::Status(Box::new(self.status_snapshot())),
            IpcRequest::Toggle => {
                self.set_enabled(!self.enabled);
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::SetEnabled(en) => {
                self.set_enabled(en);
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::SetInputMethod(im) => {
                self.flush_and_reset_buffer();
                self.config.engine.input_method = im;
                let _ = self.config.save();
                self.notify_state_changed();
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::SetToneStyle(ts) => {
                self.flush_and_reset_buffer();
                self.config.engine.tone_style = ts;
                let _ = self.config.save();
                self.notify_state_changed();
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::SetCommitMode(_cm) => {
                self.flush_and_reset_buffer();
                self.config.default_commit_mode = CommitMode::Preedit;
                self.active_commit_mode = CommitMode::Preedit;
                let _ = self.config.save();
                self.notify_state_changed();
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::UpdateConfig(new_cfg) => {
                self.flush_and_reset_buffer();
                self.config = *new_cfg;
                self.compiled_rules = compile_rules(&self.config);
                self.active_commit_mode = self.config.default_commit_mode;
                let _ = self.config.save();
                self.notify_state_changed();
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::ReloadConfig => {
                self.flush_and_reset_buffer();
                self.config = Config::load_or_default();
                self.compiled_rules = compile_rules(&self.config);
                self.active_commit_mode = self.config.default_commit_mode;
                self.notify_state_changed();
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::ListNiriWindows => IpcResponse::Windows(self.niri.list_windows()),
            IpcRequest::Subscribe => {
                if let Some(sub_stream) = cmd.subscriber_stream {
                    self.subscribers.add(sub_stream);
                }
                IpcResponse::Status(Box::new(self.status_snapshot()))
            }
            IpcRequest::Quit => {
                self.should_quit = true;
                IpcResponse::Ok
            }
        };

        let _ = cmd.reply_tx.send(resp);
    }

    fn forward_raw_key(&self, time: u32, key: u32, state: KeyState) {
        if self.vk_has_keymap
            && let Some(vk) = &self.virtual_keyboard
        {
            vk.key(time, key, state.into());
        }
    }

    pub fn process_key_event(&mut self, time: u32, key: u32, state: KeyState, is_repeat: bool) {
        let pressed = matches!(state, KeyState::Pressed);

        // 1. Check built-in toggle shortcut (Ctrl+Shift, Alt+Z, Ctrl+Space)
        if !is_repeat
            && self
                .xkb
                .check_toggle_shortcut(self.config.toggle_shortcut, key, pressed)
        {
            self.set_enabled(!self.enabled);
            if !pressed {
                // If toggle occurred on modifier key release (Ctrl+Shift), still forward release
                // so modifiers never stick in client applications!
                if !self.swallowed_keys.remove(&key) {
                    self.forward_raw_key(time, key, KeyState::Released);
                }
            } else {
                self.swallowed_keys.insert(key);
            }
            return;
        }

        // 2. Handle Key Release
        if !pressed {
            if self.repeating_key.is_some_and(|(k, _)| k == key) {
                self.repeating_key = None;
            }
            if !self.swallowed_keys.remove(&key) {
                self.forward_raw_key(time, key, KeyState::Released);
            }
            return;
        }

        // 3. Key Press: check if IME transformation should be bypassed
        if !self.enabled || self.bypass_sensitive || self.xkb.has_shortcut_modifiers() {
            if !self.word_buffer.is_empty() {
                self.flush_and_reset_buffer();
            }
            self.forward_raw_key(time, key, KeyState::Pressed);
            return;
        }

        // 4. Handle Backspace inside active word buffer
        if key == EVDEV_KEY_BACKSPACE {
            if !self.word_buffer.is_empty() {
                let _ = self.word_buffer.pop_char(&self.config.engine);
                if let Some(im) = &self.input_method {
                    let disp = self.word_buffer.displayed().to_string();
                    let len = disp.len() as i32;
                    im.set_preedit_string(disp, len, len);
                    im.commit(self.done_serial);
                    let _ = self.conn.flush();
                }
                self.swallowed_keys.insert(key);
                if !is_repeat && self.xkb.key_repeats(key) {
                    self.repeating_key = Some((key, time));
                }
                return;
            }
            self.forward_raw_key(time, key, KeyState::Pressed);
            return;
        }

        // 5. Handle Enter or Tab directly as word boundaries
        if matches!(key, EVDEV_KEY_ENTER | EVDEV_KEY_TAB) {
            let had_preedit =
                !self.word_buffer.is_empty() && self.active_commit_mode == CommitMode::Preedit;
            self.finalize_word_on_boundary(None);
            if had_preedit {
                // Allow client (e.g. Zen Browser address bar or chat input) to commit preedit
                // before receiving Enter/Tab on wl_keyboard.
                thread::sleep(Duration::from_millis(6));
            }
            self.forward_raw_key(time, key, KeyState::Pressed);
            return;
        }

        // 6. Translate keycode to Unicode char via XKB
        let Some(ch) = self.xkb.key_get_char(key) else {
            // Non-printable navigation/function key (Arrows, Home, End, Esc, PageUp, etc.)
            self.flush_and_reset_buffer();
            self.forward_raw_key(time, key, KeyState::Pressed);
            return;
        };

        let is_word_char = ch.is_alphabetic()
            || (self.config.engine.input_method == InputMethod::Vni
                && ch.is_ascii_digit()
                && !self.word_buffer.is_empty());

        if is_word_char {
            self.swallowed_keys.insert(key);
            if !is_repeat && self.xkb.key_repeats(key) {
                self.repeating_key = Some((key, time));
            }

            let _ = self.word_buffer.push_char(ch, &self.config.engine);
            if let Some(im) = &self.input_method {
                let disp = self.word_buffer.displayed().to_string();
                let len = disp.len() as i32;
                im.set_preedit_string(disp, len, len);
                im.commit(self.done_serial);
                let _ = self.conn.flush();
            }
            return;
        }

        // 7. Printable non-word character (Space, punctuation, numbers in Telex)
        if !self.word_buffer.is_empty() {
            let consumed_boundary_char = self.finalize_word_on_boundary(Some(ch));
            if consumed_boundary_char {
                self.swallowed_keys.insert(key);
                if !is_repeat && self.xkb.key_repeats(key) {
                    self.repeating_key = Some((key, time));
                }
                return;
            }
        } else if !ch.is_control()
            && let Some(im) = &self.input_method
        {
            // Commit printable punctuation/spaces directly through commit_string so they
            // never race with composed text across wl_keyboard vs zwp_text_input_v3!
            im.commit_string(ch.to_string());
            im.commit(self.done_serial);
            let _ = self.conn.flush();
            self.swallowed_keys.insert(key);
            if !is_repeat && self.xkb.key_repeats(key) {
                self.repeating_key = Some((key, time));
            }
            return;
        }

        self.forward_raw_key(time, key, KeyState::Pressed);
    }

    /// Finalizes the active word when a boundary character (e.g. Space or punctuation) is pressed.
    /// Returns `true` if `boundary_char` was committed via `commit_string`.
    fn finalize_word_on_boundary(&mut self, boundary_char: Option<char>) -> bool {
        if self.word_buffer.is_empty() {
            return false;
        }

        let (mut final_word, _) = self.word_buffer.finalize_word_full(&self.config.engine);

        let mut consumed_boundary = false;
        if let Some(bch) = boundary_char
            && !bch.is_control()
        {
            final_word.push(bch);
            consumed_boundary = true;
        }
        if let Some(im) = &self.input_method {
            im.set_preedit_string(String::new(), 0, 0);
            if !final_word.is_empty() {
                im.commit_string(final_word);
            }
            im.commit(self.done_serial);
            let _ = self.conn.flush();
        }
        consumed_boundary
    }
}

fn compile_rules(cfg: &Config) -> Vec<CompiledAppRule> {
    let mut out = Vec::new();
    for rule in &cfg.app_rules {
        let Ok(app_id_re) = Regex::new(&rule.app_id_regex) else {
            continue;
        };
        let title_re = rule
            .title_regex
            .as_ref()
            .and_then(|s| Regex::new(s).ok());
        out.push(CompiledAppRule {
            app_id_re,
            title_re,
            default_enabled: rule.default_enabled,
            commit_mode: rule.commit_mode,
        });
    }
    out
}

// --- Wayland Dispatch Implementations ---

impl Dispatch<WlRegistry, ()> for DaemonState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    let seat = registry.bind::<WlSeat, _, _>(name, version.min(7), qh, ());
                    state.seat = Some(seat);
                    state.bind_ime_objects_if_ready(qh);
                }
                "zwp_input_method_manager_v2" if state.im_manager.is_none() => {
                    let mgr = registry.bind::<ZwpInputMethodManagerV2, _, _>(name, 1, qh, ());
                    state.im_manager = Some(mgr);
                    state.bind_ime_objects_if_ready(qh);
                }
                "zwp_virtual_keyboard_manager_v1" if state.vk_manager.is_none() => {
                    let mgr = registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(name, 1, qh, ());
                    state.vk_manager = Some(mgr);
                    state.bind_ime_objects_if_ready(qh);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for DaemonState {
    fn event(
        _state: &mut Self,
        _seat: &WlSeat,
        _event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for DaemonState {
    fn event(
        _state: &mut Self,
        _mgr: &ZwpInputMethodManagerV2,
        _event: <ZwpInputMethodManagerV2 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for DaemonState {
    fn event(
        _state: &mut Self,
        _mgr: &ZwpVirtualKeyboardManagerV1,
        _event: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for DaemonState {
    fn event(
        _state: &mut Self,
        _vk: &ZwpVirtualKeyboardV1,
        _event: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for DaemonState {
    fn event(
        state: &mut Self,
        im: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                state.pending_active = true;
                state.pending_activated_in_batch = true;
                state.pending_has_surrounding = false;
                state.pending_sensitive = false;
            }
            zwp_input_method_v2::Event::Deactivate => {
                state.pending_active = false;
            }
            zwp_input_method_v2::Event::SurroundingText { .. } => {
                state.pending_has_surrounding = true;
            }
            zwp_input_method_v2::Event::TextChangeCause { .. } => {
                // Note: Do NOT clear `word_buffer` here!
                // GTK3 / Firefox / Zen Browser emit SurroundingText + Done after every
                // preedit or commit_string update without setting ChangeCause::InputMethod.
            }
            zwp_input_method_v2::Event::ContentType { hint, purpose } => {
                let is_sensitive_purpose = matches!(
                    purpose,
                    WEnum::Value(
                        ContentPurpose::Password | ContentPurpose::Pin | ContentPurpose::Terminal
                    )
                );
                let is_sensitive_hint = match hint {
                    WEnum::Value(h) => {
                        h.intersects(ContentHint::SensitiveData | ContentHint::HiddenText)
                    }
                    _ => false,
                };
                state.pending_sensitive = is_sensitive_purpose || is_sensitive_hint;
            }
            zwp_input_method_v2::Event::Done => {
                state.done_serial = state.done_serial.wrapping_add(1);
                state.has_surrounding = state.pending_has_surrounding;
                state.bypass_sensitive = state.pending_sensitive;
                let re_activated = state.pending_activated_in_batch;
                state.pending_activated_in_batch = false;

                if state.pending_active != state.active || (state.pending_active && re_activated) {
                    state.active = state.pending_active;
                    state.word_buffer.clear();
                    state.repeating_key = None;

                    if state.active {
                        if state.keyboard_grab.is_none() {
                            let grab = im.grab_keyboard(qh, ());
                            state.keyboard_grab = Some(grab);
                            eprintln!("[unixkey-daemon] IME active on focused text input (keyboard grabbed)");
                        }
                    } else {
                        if let Some(grab) = state.keyboard_grab.take() {
                            grab.release();
                        }
                        state.swallowed_keys.clear();
                    }
                    state.notify_state_changed();
                }
            }
            zwp_input_method_v2::Event::Unavailable => {
                im.destroy();
                state.input_method = None;

                if state.unavailable_retries < 2 {
                    state.unavailable_retries += 1;
                    eprintln!(
                        "[unixkey-daemon] Another IME (fcitx5/ibus) is holding the Wayland seat; stopping it and taking over..."
                    );
                    let _ = Command::new("pkill").args(["-x", "fcitx5"]).status();
                    let _ = Command::new("pkill").args(["-x", "ibus-daemon"]).status();
                    thread::sleep(Duration::from_millis(300));
                    state.done_serial = 0;
                    state.active = false;
                    state.pending_active = false;
                    state.bind_ime_objects_if_ready(qh);
                } else {
                    eprintln!(
                        "[unixkey-daemon] ERROR: zwp_input_method_v2 unavailable after retries. Another IME is still active."
                    );
                    state.should_quit = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, ()> for DaemonState {
    fn event(
        state: &mut Self,
        _grab: &ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Keymap { format, fd, size } => {
                if let Err(e) = state.xkb.load_keymap_from_fd(&fd, size as usize) {
                    eprintln!("[unixkey-daemon] Failed to load XKB keymap: {e}");
                }
                if let Some(vk) = &state.virtual_keyboard {
                    vk.keymap(format.into(), fd.as_fd(), size);
                    state.vk_has_keymap = true;
                    let m = state.xkb.last_modifiers();
                    vk.modifiers(m.mods_depressed, m.mods_latched, m.mods_locked, m.group);
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                state
                    .xkb
                    .update_modifiers(mods_depressed, mods_latched, mods_locked, group);
                if state.vk_has_keymap
                    && let Some(vk) = &state.virtual_keyboard
                {
                    vk.modifiers(mods_depressed, mods_latched, mods_locked, group);
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Key {
                time, key, state: key_state, ..
            } => {
                if let WEnum::Value(ks) = key_state {
                    state.process_key_event(time, key, ks, false);
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                state.repeat_rate = rate.max(0);
                state.repeat_delay = delay.max(0);
            }
            _ => {}
        }
    }
}
