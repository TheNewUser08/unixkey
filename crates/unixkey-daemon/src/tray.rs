use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use calloop::channel::Sender;
use log::{debug, info, warn};
use unixkey_core::{InputMethod, ToneStyle};
use zbus::blocking::Connection;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

#[derive(Debug, Clone)]
pub enum TrayAction {
    ToggleEnabled,
    SetInputMethod(InputMethod),
    SetToneStyle(ToneStyle),
    OpenSettings,
    Quit,
}

#[derive(Debug, Clone)]
pub struct TrayState {
    pub enabled: bool,
    pub input_method: InputMethod,
    pub tone_style: ToneStyle,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            enabled: true,
            input_method: InputMethod::Telex,
            tone_style: ToneStyle::Modern,
        }
    }
}

pub struct TrayHandle {
    tx: mpsc::Sender<TrayState>,
}

impl TrayHandle {
    pub fn update(&self, state: TrayState) {
        let _ = self.tx.send(state);
    }
}

struct SniService<E: Send + 'static> {
    state: Arc<Mutex<TrayState>>,
    tx: Sender<E>,
    map_action: Arc<dyn Fn(TrayAction) -> E + Send + Sync>,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl<E: Send + 'static> SniService<E> {
    #[zbus(property)]
    fn category(&self) -> &str {
        "TextServices"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "unixkey"
    }

    #[zbus(property)]
    fn title(&self) -> String {
        let st = self.state.lock().unwrap();
        if st.enabled {
            "UniXKey (VN)".to_string()
        } else {
            "UniXKey (EN)".to_string()
        }
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let st = self.state.lock().unwrap();
        vec![render_badge_pixmap(st.enabled)]
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str("/MenuBar").unwrap()
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        let st = self.state.lock().unwrap();
        let title = if st.enabled {
            "UniXKey — Tiếng Việt (VN)"
        } else {
            "UniXKey — English (EN)"
        };
        let desc = format!(
            "Method: {:?} | Tone: {:?}\nLeft-click: Toggle VN/EN | Right-click: Menu",
            st.input_method, st.tone_style
        );
        (String::new(), Vec::new(), title.to_string(), desc)
    }

    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.tx.send((self.map_action)(TrayAction::ToggleEnabled));
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        spawn_gui_process();
        let _ = self.tx.send((self.map_action)(TrayAction::OpenSettings));
    }

    fn context_menu(&self, _x: i32, _y: i32) {
        spawn_gui_process();
    }

    fn scroll(&self, _delta: i32, _orientation: &str) {
        let _ = self.tx.send((self.map_action)(TrayAction::ToggleEnabled));
    }

    #[zbus(signal)]
    async fn new_icon(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_title(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

struct DbusMenuService<E: Send + 'static> {
    state: Arc<Mutex<TrayState>>,
    tx: Sender<E>,
    map_action: Arc<dyn Fn(TrayAction) -> E + Send + Sync>,
}

type MenuItemTuple = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

impl<E: Send + 'static> DbusMenuService<E> {
    fn build_items(&self) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let st = self.state.lock().unwrap().clone();
        let toggle_label = if st.enabled {
            "✓ Tiếng Việt (VN) — Switch to EN"
        } else {
            "  English (EN) — Switch to VN"
        };
        let telex_label = if st.input_method == InputMethod::Telex {
            "• Input: Telex"
        } else {
            "  Input: Telex"
        };
        let vni_label = if st.input_method == InputMethod::Vni {
            "• Input: VNI"
        } else {
            "  Input: VNI"
        };
        let tone_label = match st.tone_style {
            ToneStyle::Modern => "Tone: Modern (hoà, thuý)",
            ToneStyle::Traditional => "Tone: Traditional (hòa, thúy)",
        };

        vec![
            (1, item_props(toggle_label, true, false)),
            (2, item_props("", true, true)),
            (3, item_props(telex_label, true, false)),
            (4, item_props(vni_label, true, false)),
            (5, item_props(tone_label, true, false)),
            (6, item_props("", true, true)),
            (7, item_props("Open ViKey Settings...", true, false)),
            (8, item_props("Quit ViKey", true, false)),
        ]
    }

    fn handle_menu_click(&self, id: i32) {
        match id {
            1 => {
                let _ = self.tx.send((self.map_action)(TrayAction::ToggleEnabled));
            }
            3 => {
                let _ = self
                    .tx
                    .send((self.map_action)(TrayAction::SetInputMethod(InputMethod::Telex)));
            }
            4 => {
                let _ = self
                    .tx
                    .send((self.map_action)(TrayAction::SetInputMethod(InputMethod::Vni)));
            }
            5 => {
                let next_tone = {
                    let st = self.state.lock().unwrap();
                    match st.tone_style {
                        ToneStyle::Modern => ToneStyle::Traditional,
                        ToneStyle::Traditional => ToneStyle::Modern,
                    }
                };
                let _ = self
                    .tx
                    .send((self.map_action)(TrayAction::SetToneStyle(next_tone)));
            }
            7 => {
                spawn_gui_process();
                let _ = self.tx.send((self.map_action)(TrayAction::OpenSettings));
            }
            8 => {
                let _ = self.tx.send((self.map_action)(TrayAction::Quit));
            }
            _ => {}
        }
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl<E: Send + 'static> DbusMenuService<E> {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, MenuItemTuple) {
        let items = self.build_items();
        if parent_id == 0 {
            let mut root_props = HashMap::new();
            root_props.insert(
                "children-display".to_string(),
                Value::from("submenu").try_to_owned().unwrap(),
            );
            let children: Vec<OwnedValue> = items
                .into_iter()
                .map(|(id, props)| {
                    let tuple: MenuItemTuple = (id, props, Vec::new());
                    Value::from(tuple).try_to_owned().unwrap()
                })
                .collect();
            (1, (0, root_props, children))
        } else {
            let props = items
                .into_iter()
                .find(|(id, _)| *id == parent_id)
                .map(|(_, p)| p)
                .unwrap_or_default();
            (1, (parent_id, props, Vec::new()))
        }
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let items = self.build_items();
        if ids.is_empty() {
            return items;
        }
        items
            .into_iter()
            .filter(|(id, _)| ids.contains(id))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> OwnedValue {
        for (item_id, props) in self.build_items() {
            if item_id == id
                && let Some(val) = props.get(&name)
            {
                return val.clone();
            }
        }
        Value::from("").try_to_owned().unwrap()
    }

    fn event(&self, id: i32, event_id: &str, _data: Value<'_>, _timestamp: u32) {
        if event_id.eq_ignore_ascii_case("clicked") {
            self.handle_menu_click(id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, Value<'_>, u32)>) -> Vec<i32> {
        for (id, event_id, _, _) in events {
            if event_id.eq_ignore_ascii_case("clicked") {
                self.handle_menu_click(id);
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }
}

fn item_props(label: &str, enabled: bool, is_separator: bool) -> HashMap<String, OwnedValue> {
    let mut props: HashMap<String, OwnedValue> = HashMap::new();
    if is_separator {
        props.insert(
            "type".to_string(),
            Value::from("separator").try_to_owned().unwrap(),
        );
    } else {
        props.insert(
            "type".to_string(),
            Value::from("standard").try_to_owned().unwrap(),
        );
        props.insert(
            "label".to_string(),
            Value::from(label).try_to_owned().unwrap(),
        );
        props.insert(
            "enabled".to_string(),
            Value::from(enabled).try_to_owned().unwrap(),
        );
    }
    props.insert(
        "visible".to_string(),
        Value::from(true).try_to_owned().unwrap(),
    );
    props
}

fn gui_socket_path() -> std::path::PathBuf {
    let mut p = unixkey_core::runtime_socket_path();
    p.set_file_name("unixkey-gui.sock");
    p
}

fn spawn_gui_process() {
    // If a unixkey-gui supervisor is already running minimized in the background, wake it up!
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(gui_socket_path()) {
        use std::io::Write;
        let _ = stream.write_all(b"show\n");
        let _ = stream.flush();
        return;
    }

    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("unixkey-gui")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "unixkey-gui".into());

    let _ = Command::new(exe).spawn();
}

pub fn spawn_system_tray<F, E>(
    initial: TrayState,
    tx: Sender<E>,
    map_action: F,
) -> TrayHandle
where
    F: Fn(TrayAction) -> E + Send + Sync + 'static,
    E: Send + 'static,
{
    let (state_tx, state_rx): (mpsc::Sender<TrayState>, Receiver<TrayState>) = mpsc::channel();
    let shared_state = Arc::new(Mutex::new(initial));
    let map_arc: Arc<dyn Fn(TrayAction) -> E + Send + Sync> = Arc::new(map_action);

    thread::Builder::new()
        .name("unixkey-sni-tray".to_string())
        .spawn(move || {
            let Ok(conn) = Connection::session() else {
                warn!("Could not connect to DBus session bus for StatusNotifierItem tray");
                return;
            };

            let sni = SniService {
                state: Arc::clone(&shared_state),
                tx: tx.clone(),
                map_action: Arc::clone(&map_arc),
            };
            let menu = DbusMenuService {
                state: Arc::clone(&shared_state),
                tx,
                map_action: map_arc,
            };

            let Ok(obj_server) = conn.object_server().at("/StatusNotifierItem", sni) else {
                warn!("Failed to export /StatusNotifierItem on DBus");
                return;
            };
            let _ = obj_server;
            let _ = conn.object_server().at("/MenuBar", menu);

            let bus_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
            if let Err(e) = conn.request_name(bus_name.as_str()) {
                warn!("Failed to request SNI DBus name {bus_name}: {e}");
                return;
            }

            let register_res = conn.call_method(
                Some("org.kde.StatusNotifierWatcher"),
                "/StatusNotifierWatcher",
                Some("org.kde.StatusNotifierWatcher"),
                "RegisterStatusNotifierItem",
                &("/StatusNotifierItem",),
            );
            match register_res {
                Ok(_) => info!("Registered StatusNotifierItem tray icon as {bus_name}"),
                Err(e) => debug!("StatusNotifierWatcher not active right now ({e}); tray will still be exported"),
            }

            while let Ok(new_state) = state_rx.recv() {
                {
                    let mut guard = shared_state.lock().unwrap();
                    *guard = new_state;
                }
                if let Ok(iface_ref) = conn
                    .object_server()
                    .interface::<_, SniService<E>>("/StatusNotifierItem")
                {
                    let emitter = iface_ref.signal_emitter();
                    let _ = zbus::block_on(SniService::<E>::new_icon(emitter));
                    let _ = zbus::block_on(SniService::<E>::new_title(emitter));
                    let _ = zbus::block_on(SniService::<E>::new_tool_tip(emitter));
                }
            }
        })
        .expect("Failed to spawn unixkey-sni-tray thread");

    TrayHandle { tx: state_tx }
}

/// Renders a crisp 24x24 ARGB32 (network byte order: A, R, G, B) badge with "VN" or "EN".
fn render_badge_pixmap(vn: bool) -> (i32, i32, Vec<u8>) {
    const W: usize = 24;
    const H: usize = 24;
    let mut buf = vec![0u8; W * H * 4];

    // Background rounded badge color: Crimson Red (#E11D48) for VN, Slate (#334155) for EN
    let bg: [u8; 4] = if vn {
        [0xFF, 0xE1, 0x1D, 0x48]
    } else {
        [0xFF, 0x33, 0x41, 0x55]
    };
    let fg: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

    for y in 2..(H - 2) {
        for x in 1..(W - 1) {
            // Simple rounded corners
            if (x == 1 || x == W - 2) && (y == 2 || y == H - 3) {
                continue;
            }
            let idx = (y * W + x) * 4;
            buf[idx..idx + 4].copy_from_slice(&bg);
        }
    }

    // 5x7 bitmap glyphs for 'V', 'E', 'N'
    const GLYPH_V: [u8; 7] = [
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b01010,
        0b00100,
    ];
    const GLYPH_E: [u8; 7] = [
        0b11111,
        0b10000,
        0b10000,
        0b11110,
        0b10000,
        0b10000,
        0b11111,
    ];
    const GLYPH_N: [u8; 7] = [
        0b10001,
        0b11001,
        0b10101,
        0b10011,
        0b10001,
        0b10001,
        0b10001,
    ];

    let left_glyph = if vn { &GLYPH_V } else { &GLYPH_E };
    draw_glyph(&mut buf, W, 6, 8, left_glyph, &fg);
    draw_glyph(&mut buf, W, 13, 8, &GLYPH_N, &fg);

    (W as i32, H as i32, buf)
}

fn draw_glyph(buf: &mut [u8], stride: usize, ox: usize, oy: usize, rows: &[u8; 7], color: &[u8; 4]) {
    for (ry, &bits) in rows.iter().enumerate() {
        for rx in 0..5 {
            if (bits >> (4 - rx)) & 1 == 1 {
                let x = ox + rx;
                let y = oy + ry;
                let idx = (y * stride + x) * 4;
                buf[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
}
