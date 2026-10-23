mod ipc_server;
mod niri;
mod tray;
mod wayland;
mod xkb;

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use calloop::EventLoop;
use calloop::channel::{self, Event as ChannelEvent};
use calloop_wayland_source::WaylandSource;
use log::info;
use vikey_core::{Config, runtime_socket_path};
use wayland_client::Connection;
use wayland_client::protocol::wl_keyboard::KeyState;

use crate::ipc_server::{IpcCommand, spawn_ipc_server};
use crate::niri::{NiriEvent, spawn_niri_listener};
use crate::tray::{TrayAction, TrayState, spawn_system_tray};
use crate::wayland::DaemonState;

enum DaemonMessage {
    Niri(NiriEvent),
    Tray(TrayAction),
    Ipc(IpcCommand),
}

fn main() -> Result<()> {
    let config = Config::load_or_default();

    // Ensure GTK3/GTK4/Qt6 apps (including Zen Browser / Firefox) use native Wayland text-input-v3
    // instead of legacy fcitx5/ibus DBus modules.
    let _ = std::process::Command::new("systemctl")
        .args([
            "--user",
            "set-environment",
            "GTK_IM_MODULE=wayland",
            "QT_IM_MODULE=wayland",
        ])
        .status();
    let _ = std::process::Command::new("dbus-update-activation-environment")
        .args(["--systemd", "GTK_IM_MODULE=wayland", "QT_IM_MODULE=wayland"])
        .status();

    let conn = Connection::connect_to_env()
        .context("Failed to connect to Wayland display (is WAYLAND_DISPLAY set?)")?;
    let display = conn.display();

    let mut event_queue = conn.new_event_queue::<DaemonState>();
    let qh = event_queue.handle();
    let _registry = display.get_registry(&qh, ());

    let mut event_loop: EventLoop<DaemonState> =
        EventLoop::try_new().context("Failed to initialize calloop event loop")?;
    let loop_handle = event_loop.handle();

    let (msg_tx, msg_rx) = channel::channel::<DaemonMessage>();

    // 1. Spawn Niri IPC window tracker ($NIRI_SOCKET)
    spawn_niri_listener(msg_tx.clone(), DaemonMessage::Niri);

    // 2. Spawn StatusNotifierItem DBus System Tray (if enabled)
    let tray_handle = if config.enable_tray {
        Some(spawn_system_tray(
            TrayState {
                enabled: config.enabled_by_default,
                input_method: config.engine.input_method,
                tone_style: config.engine.tone_style,
            },
            msg_tx.clone(),
            DaemonMessage::Tray,
        ))
    } else {
        None
    };

    // 3. Spawn Unix Socket Control Server ($XDG_RUNTIME_DIR/vikey.sock)
    spawn_ipc_server(msg_tx.clone(), DaemonMessage::Ipc)?;

    // 4. Register message channel on calloop
    loop_handle
        .insert_source(msg_rx, |event, _, state: &mut DaemonState| {
            if let ChannelEvent::Msg(msg) = event {
                match msg {
                    DaemonMessage::Niri(ev) => state.handle_niri_event(ev),
                    DaemonMessage::Tray(act) => state.handle_tray_action(act),
                    DaemonMessage::Ipc(cmd) => state.handle_ipc_command(cmd),
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("Failed to insert channel source: {e}"))?;

    let mut state = DaemonState::new(conn.clone(), config, tray_handle)?;

    // Perform initial Wayland roundtrip to bind seat, input_method_v2, and virtual_keyboard_v1
    event_queue
        .roundtrip(&mut state)
        .context("Initial Wayland roundtrip failed")?;

    WaylandSource::new(conn, event_queue)
        .insert(loop_handle)
        .map_err(|e| anyhow::anyhow!("Failed to insert WaylandSource: {e}"))?;

    info!("vikey-daemon started and ready for Niri Wayland input!");

    let mut repeat_started_at: Option<(u32, Instant)> = None;
    let mut last_repeat_tick: Option<Instant> = None;

    while !state.should_quit {
        let timeout = compute_repeat_timeout(
            &state,
            &mut repeat_started_at,
            &mut last_repeat_tick,
        );

        event_loop
            .dispatch(timeout, &mut state)
            .context("Event loop dispatch error")?;

        // Check if a held key is due for a repeat event
        if let Some((key, base_time_ms)) = state.repeating_key
            && state.repeat_rate > 0
            && let Some((_, started)) = repeat_started_at
        {
            let now = Instant::now();
            let delay = Duration::from_millis(state.repeat_delay as u64);
            if now.duration_since(started) >= delay {
                let interval = Duration::from_micros(1_000_000 / (state.repeat_rate as u64));
                let due = last_repeat_tick.is_none_or(|last| now.duration_since(last) >= interval);
                if due {
                    last_repeat_tick = Some(now);
                    let synth_time =
                        base_time_ms.wrapping_add(now.duration_since(started).as_millis() as u32);
                    state.process_key_event(synth_time, key, KeyState::Pressed, true);
                    let _ = state.conn.flush();
                }
            }
        }
    }

    let _ = std::fs::remove_file(runtime_socket_path());
    let mut gui_sock = runtime_socket_path();
    gui_sock.set_file_name("vikey-gui.sock");
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&gui_sock) {
        use std::io::Write;
        let _ = stream.write_all(b"quit\n");
    }
    Ok(())
}

fn compute_repeat_timeout(
    state: &DaemonState,
    repeat_started_at: &mut Option<(u32, Instant)>,
    last_repeat_tick: &mut Option<Instant>,
) -> Option<Duration> {
    let Some((key, _)) = state.repeating_key else {
        *repeat_started_at = None;
        *last_repeat_tick = None;
        return None;
    };

    if state.repeat_rate <= 0 {
        return None;
    }

    let now = Instant::now();
    if repeat_started_at.is_none_or(|(k, _)| k != key) {
        *repeat_started_at = Some((key, now));
        *last_repeat_tick = None;
    }

    let (_, started) = repeat_started_at.unwrap();
    let delay = Duration::from_millis(state.repeat_delay as u64);
    let elapsed = now.duration_since(started);

    if elapsed < delay {
        return Some(delay - elapsed);
    }

    let interval = Duration::from_micros(1_000_000 / (state.repeat_rate as u64));
    if let Some(last) = *last_repeat_tick {
        let since_last = now.duration_since(last);
        if since_last < interval {
            return Some(interval - since_last);
        }
        return Some(Duration::ZERO);
    }

    Some(Duration::ZERO)
}
