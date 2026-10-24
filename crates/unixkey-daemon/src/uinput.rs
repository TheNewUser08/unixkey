use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::{AsRawFd, RawFd};

use log::{info, warn};

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;
const UI_DEV_SETUP: u64 = 0x405c5503;
const UI_SET_EVBIT: u64 = 0x40045564;
const UI_SET_KEYBIT: u64 = 0x40045565;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
pub const KEY_BACKSPACE: u16 = 14;
const SYN_REPORT: u16 = 0;

pub struct UinputKeyboard {
    file: File,
}

impl UinputKeyboard {
    pub fn try_new() -> Option<Self> {
        let file = OpenOptions::new()
            .read(false)
            .write(true)
            .open("/dev/uinput")
            .map_err(|e| {
                warn!("Could not open /dev/uinput: {e}. Will fallback to zwp_virtual_keyboard_v1.");
                e
            })
            .ok()?;

        let fd = file.as_raw_fd();
        unsafe {
            if libc_ioctl(fd, UI_SET_EVBIT, EV_KEY as u64) < 0 {
                warn!("ioctl(UI_SET_EVBIT) failed");
                return None;
            }
            if libc_ioctl(fd, UI_SET_KEYBIT, KEY_BACKSPACE as u64) < 0 {
                warn!("ioctl(UI_SET_KEYBIT) failed");
                return None;
            }

            let mut setup = std::mem::zeroed::<UinputSetup>();
            setup.id.bustype = 0x03; // BUS_USB
            setup.id.vendor = 0x1234;
            setup.id.product = 0x5678;
            setup.id.version = 1;
            let name = b"UniXKey Virtual Backspace";
            setup.name[..name.len()].copy_from_slice(name);

            if libc_ioctl(fd, UI_DEV_SETUP, &setup as *const _ as u64) < 0 {
                warn!("ioctl(UI_DEV_SETUP) failed");
                return None;
            }
            if libc_ioctl(fd, UI_DEV_CREATE, 0) < 0 {
                warn!("ioctl(UI_DEV_CREATE) failed");
                return None;
            }
        }

        info!("Created Linux /dev/uinput virtual keyboard for instant Direct Commit mode");
        Some(Self { file })
    }

    pub fn send_backspaces(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        let mut events = Vec::with_capacity(count * 4);
        for _ in 0..count {
            events.push(InputEvent {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_KEY,
                code: KEY_BACKSPACE,
                value: 1,
            });
            events.push(InputEvent {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_SYN,
                code: SYN_REPORT,
                value: 0,
            });
            events.push(InputEvent {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_KEY,
                code: KEY_BACKSPACE,
                value: 0,
            });
            events.push(InputEvent {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_SYN,
                code: SYN_REPORT,
                value: 0,
            });
        }

        let slice = unsafe {
            std::slice::from_raw_parts(
                events.as_ptr() as *const u8,
                events.len() * std::mem::size_of::<InputEvent>(),
            )
        };
        let _ = self.file.write_all(slice);
        let _ = self.file.flush();
    }
}

impl Drop for UinputKeyboard {
    fn drop(&mut self) {
        unsafe {
            let _ = libc_ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY, 0);
        }
    }
}

unsafe fn libc_ioctl(fd: RawFd, req: u64, arg: u64) -> i32 {
    unsafe extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }
    unsafe { ioctl(fd, req, arg) }
}
