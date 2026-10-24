use std::os::fd::AsFd;
use std::os::raw::c_char;
use std::ptr;

use anyhow::{Result, bail};
use memmap2::MmapOptions;
use unixkey_core::ToggleShortcut;
use xkbcommon_dl::{
    XKB_MOD_NAME_ALT, XKB_MOD_NAME_CTRL, XKB_MOD_NAME_LOGO, XKB_MOD_NAME_SHIFT, xkb_context,
    xkb_context_flags, xkb_keymap, xkb_keymap_compile_flags, xkb_keymap_format, xkb_state,
    xkb_state_component, xkbcommon_option,
};

pub const EVDEV_KEY_BACKSPACE: u32 = 14;
pub const EVDEV_KEY_TAB: u32 = 15;
pub const EVDEV_KEY_ENTER: u32 = 28;
pub const EVDEV_KEY_LEFTCTRL: u32 = 29;
pub const EVDEV_KEY_LEFTSHIFT: u32 = 42;
pub const EVDEV_KEY_Z: u32 = 44;
pub const EVDEV_KEY_RIGHTSHIFT: u32 = 54;
pub const EVDEV_KEY_LEFTALT: u32 = 56;
pub const EVDEV_KEY_SPACE: u32 = 57;
pub const EVDEV_KEY_RIGHTCTRL: u32 = 97;
pub const EVDEV_KEY_RIGHTALT: u32 = 100;

#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierSnapshot {
    pub mods_depressed: u32,
    pub mods_latched: u32,
    pub mods_locked: u32,
    pub group: u32,
}

pub struct XkbTracker {
    ctx: *mut xkb_context,
    keymap: *mut xkb_keymap,
    state: *mut xkb_state,
    last_mods: ModifierSnapshot,
    ctrl_shift_armed: bool,
    other_key_during_chord: bool,
}

unsafe impl Send for XkbTracker {}

impl XkbTracker {
    pub fn new() -> Result<Self> {
        let Some(lib) = xkbcommon_option() else {
            bail!("libxkbcommon.so.0 could not be loaded");
        };
        let ctx = unsafe { (lib.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS) };
        if ctx.is_null() {
            bail!("xkb_context_new returned null");
        }
        Ok(Self {
            ctx,
            keymap: ptr::null_mut(),
            state: ptr::null_mut(),
            last_mods: ModifierSnapshot::default(),
            ctrl_shift_armed: false,
            other_key_during_chord: false,
        })
    }

    pub fn load_keymap_from_fd<F: AsFd>(&mut self, fd: F, size: usize) -> Result<()> {
        let Some(lib) = xkbcommon_option() else {
            bail!("libxkbcommon.so.0 not available");
        };

        let mmap = unsafe { MmapOptions::new().len(size).map_copy_read_only(&fd.as_fd())? };
        // XKB text v1 keymaps may have a trailing NUL byte
        let actual_len = mmap.iter().position(|&b| b == 0).unwrap_or(size);

        let new_keymap = unsafe {
            (lib.xkb_keymap_new_from_buffer)(
                self.ctx,
                mmap.as_ptr() as *const c_char,
                actual_len,
                xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        if new_keymap.is_null() {
            bail!("xkb_keymap_new_from_buffer failed");
        }

        let new_state = unsafe { (lib.xkb_state_new)(new_keymap) };
        if new_state.is_null() {
            unsafe { (lib.xkb_keymap_unref)(new_keymap) };
            bail!("xkb_state_new failed");
        }

        unsafe {
            if !self.state.is_null() {
                (lib.xkb_state_unref)(self.state);
            }
            if !self.keymap.is_null() {
                (lib.xkb_keymap_unref)(self.keymap);
            }
        }

        self.keymap = new_keymap;
        self.state = new_state;

        // Re-apply last known modifier mask
        let m = self.last_mods;
        self.update_modifiers(m.mods_depressed, m.mods_latched, m.mods_locked, m.group);
        Ok(())
    }

    pub fn update_modifiers(
        &mut self,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    ) {
        self.last_mods = ModifierSnapshot {
            mods_depressed,
            mods_latched,
            mods_locked,
            group,
        };
        if self.state.is_null() {
            return;
        }
        let Some(lib) = xkbcommon_option() else {
            return;
        };
        unsafe {
            (lib.xkb_state_update_mask)(
                self.state,
                mods_depressed,
                mods_latched,
                mods_locked,
                0,
                0,
                group,
            );
        }

        if self.is_ctrl_active() && self.is_shift_active() && !self.is_alt_active() && !self.is_logo_active() {
            self.ctrl_shift_armed = true;
        }
    }

    pub fn last_modifiers(&self) -> ModifierSnapshot {
        self.last_mods
    }

    pub fn is_ctrl_active(&self) -> bool {
        self.is_mod_active(XKB_MOD_NAME_CTRL)
    }

    pub fn is_alt_active(&self) -> bool {
        self.is_mod_active(XKB_MOD_NAME_ALT)
    }

    pub fn is_shift_active(&self) -> bool {
        self.is_mod_active(XKB_MOD_NAME_SHIFT)
    }

    pub fn is_logo_active(&self) -> bool {
        self.is_mod_active(XKB_MOD_NAME_LOGO)
    }

    pub fn has_shortcut_modifiers(&self) -> bool {
        self.is_ctrl_active() || self.is_alt_active() || self.is_logo_active()
    }

    fn is_mod_active(&self, mod_name: &[u8]) -> bool {
        if self.state.is_null() {
            return false;
        }
        let Some(lib) = xkbcommon_option() else {
            return false;
        };
        let res = unsafe {
            (lib.xkb_state_mod_name_is_active)(
                self.state,
                mod_name.as_ptr() as *const c_char,
                xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
            )
        };
        res > 0
    }

    /// Translates an evdev keycode to a Unicode `char` using the current XKB state.
    pub fn key_get_char(&self, evdev_key: u32) -> Option<char> {
        if self.state.is_null() {
            return None;
        }
        let Some(lib) = xkbcommon_option() else {
            return None;
        };
        let xkb_keycode = evdev_key + 8;
        let utf32 = unsafe { (lib.xkb_state_key_get_utf32)(self.state, xkb_keycode) };
        if utf32 == 0 {
            return None;
        }
        char::from_u32(utf32)
    }

    pub fn key_repeats(&self, evdev_key: u32) -> bool {
        if self.keymap.is_null() {
            return true;
        }
        let Some(lib) = xkbcommon_option() else {
            return true;
        };
        let xkb_keycode = evdev_key + 8;
        unsafe { (lib.xkb_keymap_key_repeats)(self.keymap, xkb_keycode) > 0 }
    }

    /// Checks whether a key press or release triggers the configured `ToggleShortcut`.
    /// Returns `true` if the toggle shortcut was triggered and the key event should be consumed.
    pub fn check_toggle_shortcut(
        &mut self,
        shortcut: ToggleShortcut,
        evdev_key: u32,
        pressed: bool,
    ) -> bool {
        let is_modifier_key = matches!(
            evdev_key,
            EVDEV_KEY_LEFTCTRL
                | EVDEV_KEY_RIGHTCTRL
                | EVDEV_KEY_LEFTSHIFT
                | EVDEV_KEY_RIGHTSHIFT
                | EVDEV_KEY_LEFTALT
                | EVDEV_KEY_RIGHTALT
        );

        if pressed && !is_modifier_key {
            self.other_key_during_chord = true;
        }

        match shortcut {
            ToggleShortcut::None => false,
            ToggleShortcut::AltZ => {
                pressed
                    && evdev_key == EVDEV_KEY_Z
                    && self.is_alt_active()
                    && !self.is_ctrl_active()
                    && !self.is_logo_active()
            }
            ToggleShortcut::CtrlSpace => {
                pressed
                    && evdev_key == EVDEV_KEY_SPACE
                    && self.is_ctrl_active()
                    && !self.is_alt_active()
                    && !self.is_logo_active()
            }
            ToggleShortcut::CtrlShift => {
                if !pressed
                    && matches!(
                        evdev_key,
                        EVDEV_KEY_LEFTCTRL
                            | EVDEV_KEY_RIGHTCTRL
                            | EVDEV_KEY_LEFTSHIFT
                            | EVDEV_KEY_RIGHTSHIFT
                    )
                {
                    let should_toggle = self.ctrl_shift_armed && !self.other_key_during_chord;
                    self.ctrl_shift_armed = false;
                    self.other_key_during_chord = false;
                    return should_toggle;
                }
                if !pressed && !self.is_ctrl_active() && !self.is_shift_active() {
                    self.ctrl_shift_armed = false;
                    self.other_key_during_chord = false;
                }
                false
            }
        }
    }
}

impl Drop for XkbTracker {
    fn drop(&mut self) {
        if let Some(lib) = xkbcommon_option() {
            unsafe {
                if !self.state.is_null() {
                    (lib.xkb_state_unref)(self.state);
                }
                if !self.keymap.is_null() {
                    (lib.xkb_keymap_unref)(self.keymap);
                }
                if !self.ctx.is_null() {
                    (lib.xkb_context_unref)(self.ctx);
                }
            }
        }
    }
}
