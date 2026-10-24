use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, Vec2,
};
use unixkey_core::{
    AppRule, CommitMode, Config, InputMethod, IpcRequest, IpcResponse, NiriWindow, ToggleShortcut,
    ToneStyle, runtime_socket_path, transform_text,
};

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    General,
    AppRules,
    Sandbox,
}

impl Tab {
    fn breadcrumb(self) -> &'static str {
        match self {
            Tab::General => "UniXKey  /  General & Typing",
            Tab::AppRules => "UniXKey  /  Per-App Rules (Niri)",
            Tab::Sandbox => "UniXKey  /  Live Typing Sandbox",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Tab::General => "General & Typing",
            Tab::AppRules => "Per-App Rules",
            Tab::Sandbox => "Live Typing Sandbox",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Tab::General => {
                "Configure Vietnamese input method, orthography, and Wayland commit behavior."
            }
            Tab::AppRules => {
                "Automatically switch VN/EN or override commit modes based on Niri's focused app-id."
            }
            Tab::Sandbox => {
                "Test Telex/VNI keystrokes and English auto-restoration in a Notion-style document block."
            }
        }
    }
}

#[derive(Clone, Copy)]
struct NotionPalette {
    is_dark: bool,
    bg_canvas: Color32,
    bg_sidebar: Color32,
    bg_card: Color32,
    bg_hover: Color32,
    bg_active: Color32,
    bg_code: Color32,
    text_primary: Color32,
    text_secondary: Color32,
    text_muted: Color32,
    border: Color32,
    accent_blue: Color32,
    tag_vn_bg: Color32,
    tag_vn_fg: Color32,
    tag_en_bg: Color32,
    tag_en_fg: Color32,
    tag_green_fg: Color32,
}

impl NotionPalette {
    fn light() -> Self {
        Self {
            is_dark: false,
            bg_canvas: Color32::from_rgb(255, 255, 255),
            bg_sidebar: Color32::from_rgb(247, 246, 243), // #F7F6F3 warm Notion sidebar
            bg_card: Color32::from_rgb(250, 250, 249),    // #FAFAF9
            bg_hover: Color32::from_rgb(239, 239, 238),   // #EFEFEE
            bg_active: Color32::from_rgb(232, 231, 228),  // #E8E7E4
            bg_code: Color32::from_rgb(241, 241, 239),    // #F1F1EF
            text_primary: Color32::from_rgb(55, 53, 47),  // #37352F Notion ink
            text_secondary: Color32::from_rgb(120, 119, 116), // #787774
            text_muted: Color32::from_rgb(155, 154, 151), // #9B9A97
            border: Color32::from_rgb(233, 233, 231),     // #E9E9E7 subtle Notion divider
            accent_blue: Color32::from_rgb(35, 131, 226), // #2383E2 Notion blue
            tag_vn_bg: Color32::from_rgb(253, 235, 236),  // #FDEBEC
            tag_vn_fg: Color32::from_rgb(212, 76, 71),    // #D44C47
            tag_en_bg: Color32::from_rgb(231, 243, 248),  // #E7F3F8
            tag_en_fg: Color32::from_rgb(35, 131, 226),   // #2383E2
            tag_green_fg: Color32::from_rgb(68, 131, 97), // #448361
        }
    }

    fn dark() -> Self {
        Self {
            is_dark: true,
            bg_canvas: Color32::from_rgb(25, 25, 25),  // #191919 Notion dark canvas
            bg_sidebar: Color32::from_rgb(32, 32, 32), // #202020 Notion dark sidebar
            bg_card: Color32::from_rgb(34, 34, 34),    // #222222
            bg_hover: Color32::from_rgb(44, 44, 44),   // #2C2C2C
            bg_active: Color32::from_rgb(52, 52, 52),  // #343434
            bg_code: Color32::from_rgb(42, 42, 42),    // #2A2A2A
            text_primary: Color32::from_rgb(235, 235, 235), // #EBEBEB
            text_secondary: Color32::from_rgb(155, 154, 151), // #9B9A97
            text_muted: Color32::from_rgb(115, 114, 111),
            border: Color32::from_rgb(47, 47, 47),        // #2F2F2F
            accent_blue: Color32::from_rgb(35, 131, 226), // #2383E2
            tag_vn_bg: Color32::from_rgb(73, 41, 40),
            tag_vn_fg: Color32::from_rgb(255, 115, 105),
            tag_en_bg: Color32::from_rgb(30, 58, 76),
            tag_en_fg: Color32::from_rgb(82, 156, 202),
            tag_green_fg: Color32::from_rgb(77, 171, 154),
        }
    }

    fn apply_to_ctx(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let mut visuals = if self.is_dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        visuals.panel_fill = self.bg_canvas;
        visuals.window_fill = self.bg_canvas;
        visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        visuals.extreme_bg_color = self.bg_card;
        visuals.faint_bg_color = self.bg_card;
        visuals.code_bg_color = self.bg_code;
        visuals.override_text_color = Some(self.text_primary);
        visuals.selection.bg_fill = self.accent_blue.gamma_multiply(0.25);
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent_blue);

        let cr = CornerRadius::same(6);
        visuals.widgets.noninteractive.bg_fill = self.bg_card;
        visuals.widgets.noninteractive.weak_bg_fill = self.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.noninteractive.corner_radius = cr;

        visuals.widgets.inactive.bg_fill = self.bg_card;
        visuals.widgets.inactive.weak_bg_fill = self.bg_card;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.inactive.corner_radius = cr;

        visuals.widgets.hovered.bg_fill = self.bg_hover;
        visuals.widgets.hovered.weak_bg_fill = self.bg_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.border);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.hovered.corner_radius = cr;

        visuals.widgets.active.bg_fill = self.bg_active;
        visuals.widgets.active.weak_bg_fill = self.bg_active;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent_blue);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.active.corner_radius = cr;

        visuals.widgets.open.bg_fill = self.bg_active;
        visuals.widgets.open.weak_bg_fill = self.bg_active;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, self.border);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.open.corner_radius = cr;

        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.visuals = visuals;
        ctx.set_style(style);
    }
}

struct ViKeyGuiApp {
    tab: Tab,
    palette: NotionPalette,
    config: Config,
    daemon_online: bool,
    enabled: bool,
    ime_grab_active: bool,
    focused_window: Option<NiriWindow>,
    open_windows: Vec<NiriWindow>,
    last_poll: Instant,

    // New rule editor fields
    new_rule_name: String,
    new_rule_app_id: String,
    new_rule_default_lang: Option<bool>,
    new_rule_commit_mode: Option<CommitMode>,

    // Sandbox fields
    sandbox_raw_input: String,
}

fn setup_vietnamese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let proportional_candidates = [
        "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
        "/usr/share/fonts/google-noto-vf/NotoSans[wght].ttf",
    ];

    for path in proportional_candidates {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(
                "vietnamese_sans".to_string(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            if let Some(vec) = fonts.families.get_mut(&FontFamily::Proportional) {
                vec.insert(0, "vietnamese_sans".to_string());
            }
            break;
        }
    }

    let mono_candidates = [
        "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf",
        "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Regular.ttf",
        "/usr/share/fonts/google-noto-vf/NotoSansMono[wght].ttf",
    ];

    for path in mono_candidates {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(
                "vietnamese_mono".to_string(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            if let Some(vec) = fonts.families.get_mut(&FontFamily::Monospace) {
                vec.insert(0, "vietnamese_mono".to_string());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

impl ViKeyGuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_vietnamese_fonts(&cc.egui_ctx);

        let config = Config::load_or_default();
        let palette = if config.dark_mode {
            NotionPalette::dark()
        } else {
            NotionPalette::light()
        };
        palette.apply_to_ctx(&cc.egui_ctx);

        let mut app = Self {
            tab: Tab::General,
            palette,
            enabled: config.enabled_by_default,
            config,
            daemon_online: false,
            ime_grab_active: false,
            focused_window: None,
            open_windows: Vec::new(),
            last_poll: Instant::now() - Duration::from_secs(10),
            new_rule_name: String::new(),
            new_rule_app_id: String::new(),
            new_rule_default_lang: Some(false),
            new_rule_commit_mode: None,
            sandbox_raw_input:
                "Vieetj Nam ddaats nuowcs con nguowif — thuwr gox tieengs Vieetj treen Niri (window restore rust)".to_string(),
        };
        app.poll_daemon();
        app
    }

    fn send_ipc(req: &IpcRequest) -> Option<IpcResponse> {
        let sock = runtime_socket_path();
        let mut stream = UnixStream::connect(sock).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));

        let mut payload = serde_json::to_string(req).ok()?;
        payload.push('\n');
        stream.write_all(payload.as_bytes()).ok()?;
        stream.flush().ok()?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        serde_json::from_str(line.trim()).ok()
    }

    fn poll_daemon(&mut self) {
        self.last_poll = Instant::now();
        if let Some(IpcResponse::Status(st)) = Self::send_ipc(&IpcRequest::GetStatus) {
            self.daemon_online = true;
            self.enabled = st.enabled;
            self.ime_grab_active = st.ime_grab_active;
            self.focused_window = st.focused_window;
        } else {
            self.daemon_online = false;
            self.ime_grab_active = false;
        }
    }

    fn apply_and_save_config(&mut self) {
        // Always persist directly to ~/.config/vikey/config.toml first
        if let Err(e) = self.config.save() {
            eprintln!("Failed to save config: {e}");
        }

        // Also push live update to running vikey-daemon if online
        if let Some(IpcResponse::Status(st)) =
            Self::send_ipc(&IpcRequest::UpdateConfig(Box::new(self.config.clone())))
        {
            self.daemon_online = true;
            self.enabled = st.enabled;
        }
    }

    fn refresh_niri_windows(&mut self) {
        if let Some(IpcResponse::Windows(wins)) = Self::send_ipc(&IpcRequest::ListNiriWindows) {
            self.open_windows = wins;
        }
    }

    fn toggle_language(&mut self) {
        self.enabled = !self.enabled;
        self.config.enabled_by_default = self.enabled;
        let _ = self.config.save();

        if self.daemon_online {
            if let Some(IpcResponse::Status(st)) =
                Self::send_ipc(&IpcRequest::SetEnabled(self.enabled))
            {
                self.enabled = st.enabled;
                self.config.enabled_by_default = st.enabled;
            }
            let _ = Self::send_ipc(&IpcRequest::UpdateConfig(Box::new(self.config.clone())));
        } else {
            self.apply_and_save_config();
        }
    }

    // --- Notion UI Building Blocks ---

    fn render_sidebar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let p = self.palette;

        // Draggable workspace brand header
        ui.add_space(4.0);
        let brand_resp = ui
            .horizontal(|ui| {
                let badge_rect = ui
                    .allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::hover())
                    .0;
                ui.painter()
                    .rect_filled(badge_rect, CornerRadius::same(6), p.text_primary);
                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "V",
                    FontId::new(15.0, FontFamily::Proportional),
                    p.bg_canvas,
                );

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new("UniXKey Workspace")
                            .size(14.5)
                            .strong()
                            .color(p.text_primary),
                    );
                    ui.label(
                        RichText::new("Wayland IME · Niri")
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });
            })
            .response
            .interact(egui::Sense::click_and_drag());

        if brand_resp.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.add_space(12.0);

        // Active Mode Callout Card inside Sidebar
        egui::Frame::new()
            .fill(p.bg_canvas)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (tag_bg, tag_fg, tag_text) = if self.enabled {
                        (p.tag_vn_bg, p.tag_vn_fg, "VN · Tiếng Việt")
                    } else {
                        (p.tag_en_bg, p.tag_en_fg, "EN · English")
                    };

                    let pill = egui::Button::new(
                        RichText::new(format!(" {tag_text} "))
                            .size(13.0)
                            .strong()
                            .color(tag_fg),
                    )
                    .fill(tag_bg)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(5));

                    if ui
                        .add(pill)
                        .on_hover_text("Click to switch between Tiếng Việt and English")
                        .clicked()
                    {
                        self.toggle_language();
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (dot_color, status_label) = if self.daemon_online {
                        (p.tag_green_fg, "Daemon Active")
                    } else {
                        (p.text_muted, "Daemon Offline")
                    };
                    let (dot_rect, _) =
                        ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, dot_color);
                    ui.label(
                        RichText::new(status_label)
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });

                if !self.daemon_online {
                    ui.add_space(4.0);
                    let start_btn = egui::Button::new(
                        RichText::new("Start unixkey-daemon")
                            .size(12.0)
                            .color(Color32::WHITE),
                    )
                    .fill(p.accent_blue)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(5));

                    if ui.add_sized([ui.available_width(), 26.0], start_btn).clicked() {
                        let _ = Command::new("pkill").args(["-x", "fcitx5"]).status();
                        let _ = Command::new("pkill").args(["-x", "ibus-daemon"]).status();
                        let exe = std::env::current_exe()
                            .ok()
                            .and_then(|path| path.parent().map(|d| d.join("unixkey-daemon")))
                            .filter(|path| path.exists())
                            .unwrap_or_else(|| "unixkey-daemon".into());
                        let _ = Command::new(exe).spawn();
                        std::thread::sleep(Duration::from_millis(350));
                        self.poll_daemon();
                    }
                }
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new("SETTINGS PAGES")
                .size(11.0)
                .strong()
                .color(p.text_muted),
        );
        ui.add_space(2.0);

        self.sidebar_nav_item(ui, Tab::General, "Aa", "General & Typing");
        if self.sidebar_nav_item(ui, Tab::AppRules, "[]", "Per-App Rules") {
            self.refresh_niri_windows();
        }
        self.sidebar_nav_item(ui, Tab::Sandbox, ">_", "Typing Sandbox");

        // Push footer to bottom of sidebar
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(6.0);

            // Explicit Quit button to shut down both GUI and daemon
            let quit_btn = egui::Button::new(
                RichText::new("Quit UniXKey Completely")
                    .size(11.5)
                    .color(p.tag_vn_fg),
            )
            .fill(p.tag_vn_bg)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(5));

            if ui
                .add_sized([ui.available_width(), 24.0], quit_btn)
                .on_hover_text("Stop the background IME daemon and exit UniXKey")
                .clicked()
            {
                let _ = self.config.save();
                let _ = Self::send_ipc(&IpcRequest::Quit);
                std::process::exit(42);
            }

            ui.add_space(4.0);

            // Theme toggle row (Notion Light / Dark) — persisted to config.toml
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Appearance")
                        .size(12.0)
                        .color(p.text_secondary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme_label = if p.is_dark { "Dark" } else { "Light" };
                    let btn = egui::Button::new(
                        RichText::new(theme_label)
                            .size(11.5)
                            .color(p.text_primary),
                    )
                    .fill(p.bg_canvas)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(5));

                    if ui.add(btn).clicked() {
                        self.config.dark_mode = !p.is_dark;
                        self.palette = if self.config.dark_mode {
                            NotionPalette::dark()
                        } else {
                            NotionPalette::light()
                        };
                        self.palette.apply_to_ctx(ctx);
                        self.apply_and_save_config();
                    }
                });
            });

            ui.add_space(4.0);

            // Ko-fi support link
            let kofi_btn = egui::Button::new(
                RichText::new("☕ Support on Ko-fi")
                    .size(11.5)
                    .color(p.text_secondary),
            )
            .fill(p.bg_canvas)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(5));

            if ui
                .add_sized([ui.available_width(), 24.0], kofi_btn)
                .on_hover_text("Open https://ko-fi.com/dang809156 in browser")
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab("https://ko-fi.com/dang809156"));
            }

            ui.add_space(4.0);
            self.thin_divider(ui);
        });
    }

    fn sidebar_nav_item(&mut self, ui: &mut egui::Ui, target: Tab, icon: &str, label: &str) -> bool {
        let p = self.palette;
        let selected = self.tab == target;
        let fill = if selected {
            p.bg_active
        } else {
            Color32::TRANSPARENT
        };
        let text_color = if selected {
            p.text_primary
        } else {
            p.text_secondary
        };

        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::click());

        let bg = if resp.hovered() && !selected {
            p.bg_hover
        } else {
            fill
        };
        if bg != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), bg);
        }

        // Icon badge
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            FontId::new(12.0, FontFamily::Monospace),
            p.text_muted,
        );

        // Label
        ui.painter().text(
            egui::pos2(rect.left() + 34.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(13.5, FontFamily::Proportional),
            text_color,
        );

        let clicked = resp.clicked();
        if clicked {
            self.tab = target;
        }
        clicked
    }

    fn render_page_header(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.horizontal(|ui| {
            // Draggable breadcrumb bar
            let (drag_rect, drag_resp) = ui.allocate_exact_size(
                Vec2::new((ui.available_width() - 36.0).max(100.0), 26.0),
                egui::Sense::click_and_drag(),
            );
            ui.painter().text(
                egui::pos2(drag_rect.left(), drag_rect.center().y),
                egui::Align2::LEFT_CENTER,
                self.tab.breadcrumb(),
                FontId::new(12.0, FontFamily::Proportional),
                p.text_muted,
            );
            if drag_resp.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            // Vector-painted Minimize ("—") and Close-to-tray ("X") buttons
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Close / Minimize to System Tray ("X")
                let (close_rect, close_resp) =
                    ui.allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::click());
                let hovered = close_resp.hovered();
                if hovered {
                    ui.painter()
                        .rect_filled(close_rect, CornerRadius::same(6), p.bg_hover);
                }
                let stroke_color = if hovered {
                    p.text_primary
                } else {
                    p.text_secondary
                };
                let c = close_rect.center();
                let d = 4.5_f32;
                ui.painter().line_segment(
                    [egui::pos2(c.x - d, c.y - d), egui::pos2(c.x + d, c.y + d)],
                    Stroke::new(1.7_f32, stroke_color),
                );
                ui.painter().line_segment(
                    [egui::pos2(c.x + d, c.y - d), egui::pos2(c.x - d, c.y + d)],
                    Stroke::new(1.7_f32, stroke_color),
                );

                if close_resp
                    .on_hover_text("Minimize to system tray (UniXKey keeps running in background)")
                    .clicked()
                {
                    let _ = self.config.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                // Minimize button ("—")
                let (min_rect, min_resp) =
                    ui.allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::click());
                if min_resp.hovered() {
                    ui.painter()
                        .rect_filled(min_rect, CornerRadius::same(6), p.bg_hover);
                }
                let mc = min_rect.center();
                ui.painter().line_segment(
                    [egui::pos2(mc.x - 5.0, mc.y), egui::pos2(mc.x + 5.0, mc.y)],
                    Stroke::new(
                        1.7_f32,
                        if min_resp.hovered() {
                            p.text_primary
                        } else {
                            p.text_secondary
                        },
                    ),
                );
                if min_resp
                    .on_hover_text("Minimize to system tray")
                    .clicked()
                {
                    let _ = self.config.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(self.tab.title())
                .size(24.0)
                .strong()
                .color(p.text_primary),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new(self.tab.subtitle())
                .size(13.0)
                .color(p.text_secondary),
        );
        ui.add_space(12.0);
        self.thin_divider(ui);
        ui.add_space(10.0);
    }

    fn thin_divider(&self, ui: &mut egui::Ui) {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, self.palette.border);
    }

    fn notion_toggle_switch(&self, ui: &mut egui::Ui, on: &mut bool) -> bool {
        let desired_size = Vec2::new(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let bg_color = if *on {
            self.palette.accent_blue
        } else if self.palette.is_dark {
            Color32::from_rgb(65, 65, 65)
        } else {
            Color32::from_rgb(211, 209, 203) // Notion warm gray switch track
        };

        let radius = 0.5 * rect.height();
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius as u8), bg_color);

        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle_filled(center, radius - 2.5, Color32::WHITE);

        response.changed()
    }

    fn property_toggle_row(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        desc: &str,
        value: &mut bool,
    ) -> bool {
        let p = self.palette;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 56.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).size(14.0).color(p.text_primary));
                ui.label(RichText::new(desc).size(12.0).color(p.text_secondary));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                changed = self.notion_toggle_switch(ui, value);
            });
        });
        ui.add_space(6.0);
        self.thin_divider(ui);
        ui.add_space(6.0);
        changed
    }

    fn render_general_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let mut changed = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Section 1: Core Properties Table
                ui.label(
                    RichText::new("TYPING PROPERTIES")
                        .size(11.5)
                        .strong()
                        .color(p.text_muted),
                );
                ui.add_space(6.0);

                // Property 1: Input Method
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(290.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Input Method (Kiểu gõ)")
                                .size(14.0)
                                .color(p.text_primary),
                        );
                        ui.label(
                            RichText::new("Choose between letter-based Telex or number-based VNI.")
                                .size(12.0)
                                .color(p.text_secondary),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (method, label) in [
                            (InputMethod::Vni, "VNI (1–9)"),
                            (InputMethod::Telex, "Telex"),
                        ] {
                            let selected = self.config.engine.input_method == method;
                            let btn = egui::Button::new(
                                RichText::new(label).size(12.5).color(if selected {
                                    Color32::WHITE
                                } else {
                                    p.text_primary
                                }),
                            )
                            .fill(if selected { p.accent_blue } else { p.bg_card })
                            .stroke(Stroke::new(
                                1.0_f32,
                                if selected { p.accent_blue } else { p.border },
                            ))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add(btn).clicked() && !selected {
                                self.config.engine.input_method = method;
                                changed = true;
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                self.thin_divider(ui);
                ui.add_space(6.0);

                // Property 2: Tone Placement
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(250.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Tone Placement (Quy tắc bỏ dấu)")
                                .size(14.0)
                                .color(p.text_primary),
                        );
                        ui.label(
                            RichText::new("Modern places tone on the 2nd vowel in oa, oe, uy.")
                                .size(12.0)
                                .color(p.text_secondary),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (style, label) in [
                            (ToneStyle::Traditional, "Traditional (hòa, thúy)"),
                            (ToneStyle::Modern, "Modern (hoà, thuý)"),
                        ] {
                            let selected = self.config.engine.tone_style == style;
                            let btn = egui::Button::new(
                                RichText::new(label).size(12.5).color(if selected {
                                    Color32::WHITE
                                } else {
                                    p.text_primary
                                }),
                            )
                            .fill(if selected { p.accent_blue } else { p.bg_card })
                            .stroke(Stroke::new(
                                1.0_f32,
                                if selected { p.accent_blue } else { p.border },
                            ))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add(btn).clicked() && !selected {
                                self.config.engine.tone_style = style;
                                changed = true;
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                self.thin_divider(ui);
                ui.add_space(6.0);

                // Property 3: Toggle Shortcut
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(280.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Toggle Shortcut (Phím tắt chuyển ngôn ngữ)")
                                .size(14.0)
                                .color(p.text_primary),
                        );
                        ui.label(
                            RichText::new("Chord to switch between Tiếng Việt (VN) and English (EN).")
                                .size(12.0)
                                .color(p.text_secondary),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("toggle_shortcut_combo")
                            .selected_text(self.config.toggle_shortcut.label())
                            .show_ui(ui, |ui| {
                                for sc in [
                                    ToggleShortcut::CtrlShift,
                                    ToggleShortcut::AltZ,
                                    ToggleShortcut::CtrlSpace,
                                    ToggleShortcut::None,
                                ] {
                                    changed |= ui
                                        .selectable_value(
                                            &mut self.config.toggle_shortcut,
                                            sc,
                                            sc.label(),
                                        )
                                        .changed();
                                }
                            });
                    });
                });
                ui.add_space(6.0);
                self.thin_divider(ui);
                ui.add_space(6.0);

                // Property 4: Commit Mode (Chế độ gõ: Pre-edit Buffer vs Direct Commit)
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(280.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new("Commit Mode (Chế độ gõ)")
                                .size(14.0)
                                .color(p.text_primary),
                        );
                        ui.label(
                            RichText::new(self.config.default_commit_mode.description())
                                .size(11.5)
                                .color(p.text_secondary),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (mode, label) in [
                            (CommitMode::Direct, "Direct (Fast Autocomplete)"),
                            (CommitMode::Preedit, "Pre-edit (Underlined)"),
                        ] {
                            let selected = self.config.default_commit_mode == mode;
                            let btn = egui::Button::new(
                                RichText::new(label).size(12.0).color(if selected {
                                    Color32::WHITE
                                } else {
                                    p.text_primary
                                }),
                            )
                            .fill(if selected { p.accent_blue } else { p.bg_card })
                            .stroke(Stroke::new(
                                1.0_f32,
                                if selected { p.accent_blue } else { p.border },
                            ))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add(btn).clicked() && !selected {
                                self.config.default_commit_mode = mode;
                                changed = true;
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                self.thin_divider(ui);
                ui.add_space(6.0);

                ui.add_space(16.0);

                // Section 2: Smart Features
                ui.label(
                    RichText::new("SMART ASSISTANCE & NIRI")
                        .size(11.5)
                        .strong()
                        .color(p.text_muted),
                );
                ui.add_space(6.0);

                let mut spellcheck = self.config.engine.spellcheck;
                if self.property_toggle_row(
                    ui,
                    "Vietnamese Phonotactic Spellcheck (Kiểm tra chính tả)",
                    "Validate consonant, vowel nucleus, and stop-coda tone rules while typing.",
                    &mut spellcheck,
                ) {
                    self.config.engine.spellcheck = spellcheck;
                    changed = true;
                }

                let mut auto_restore = self.config.engine.auto_restore_english;
                if self.property_toggle_row(
                    ui,
                    "Auto-Restore English Words (Tự động khôi phục từ tiếng Anh)",
                    "Typing words like 'window', 'restore', 'support', or 'rust' automatically stays English.",
                    &mut auto_restore,
                ) {
                    self.config.engine.auto_restore_english = auto_restore;
                    changed = true;
                }

                let mut w_horn = self.config.engine.w_at_word_start_as_u_horn;
                if self.property_toggle_row(
                    ui,
                    "Quick 'W' as 'Ư' at Word Start (Gõ 'w' thành 'ư')",
                    "Allows typing 'w' or 'nhw', 'tw' for 'ư', 'như', 'tư' while preserving English 'w' words.",
                    &mut w_horn,
                ) {
                    self.config.engine.w_at_word_start_as_u_horn = w_horn;
                    changed = true;
                }

                let mut per_win = self.config.remember_per_window;
                if self.property_toggle_row(
                    ui,
                    "Remember Language State per Niri Window",
                    "Uses $NIRI_SOCKET to restore each window's VN/EN state when switching focus.",
                    &mut per_win,
                ) {
                    self.config.remember_per_window = per_win;
                    changed = true;
                }

                let mut tray = self.config.enable_tray;
                if self.property_toggle_row(
                    ui,
                    "System Tray Indicator (StatusNotifierItem)",
                    "Exports a live VN/EN badge and quick menu to Waybar / Noctalia.",
                    &mut tray,
                ) {
                    self.config.enable_tray = tray;
                    changed = true;
                }
            });

        if changed {
            self.apply_and_save_config();
        }
    }

    fn render_app_rules_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let mut changed = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let content_width = ui.available_width();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("CONFIGURED RULES")
                            .size(11.5)
                            .strong()
                            .color(p.text_muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let refresh_btn = egui::Button::new(
                            RichText::new("Refresh Niri Windows")
                                .size(12.0)
                                .color(p.text_primary),
                        )
                        .fill(p.bg_card)
                        .stroke(Stroke::new(1.0_f32, p.border))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add(refresh_btn).clicked() {
                            self.refresh_niri_windows();
                        }
                    });
                });
                ui.add_space(6.0);

                let mut remove_idx = None;
                for (idx, rule) in self.config.app_rules.iter_mut().enumerate() {
                    egui::Frame::new()
                        .fill(p.bg_card)
                        .stroke(Stroke::new(1.0_f32, p.border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_max_width(content_width - 26.0);

                            // Row 1: Rule Name + Remove button
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&rule.name)
                                        .size(13.5)
                                        .strong()
                                        .color(p.text_primary),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("Remove")
                                                        .size(11.5)
                                                        .color(p.tag_vn_fg),
                                                )
                                                .fill(p.tag_vn_bg)
                                                .stroke(Stroke::NONE)
                                                .corner_radius(CornerRadius::same(5)),
                                            )
                                            .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                    },
                                );
                            });

                            // Row 2: Truncated monospace regex pill (never overflows horizontally)
                            ui.add_space(2.0);
                            egui::Frame::new()
                                .fill(p.bg_code)
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.set_max_width(ui.available_width());
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&rule.app_id_regex)
                                                .family(FontFamily::Monospace)
                                                .size(11.5)
                                                .color(p.text_secondary),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&rule.app_id_regex);
                                });

                            // Row 3: Compact dropdown selectors for Language & Commit Mode
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Language:")
                                        .size(12.0)
                                        .color(p.text_secondary),
                                );
                                let lang_label = match rule.default_enabled {
                                    None => "Keep Current",
                                    Some(true) => "Force VN",
                                    Some(false) => "Auto EN",
                                };
                                egui::ComboBox::from_id_salt(format!("rule_lang_{idx}"))
                                    .width(120.0)
                                    .selected_text(lang_label)
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.default_enabled,
                                                None,
                                                "Keep Current",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.default_enabled,
                                                Some(true),
                                                "Force VN",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.default_enabled,
                                                Some(false),
                                                "Auto EN",
                                            )
                                            .changed();
                                    });

                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Commit:")
                                        .size(12.0)
                                        .color(p.text_secondary),
                                );
                                let cm_label = match rule.commit_mode {
                                    None => "Default",
                                    Some(CommitMode::Direct) => "Direct",
                                    Some(CommitMode::Preedit) => "Pre-edit",
                                };
                                egui::ComboBox::from_id_salt(format!("rule_cm_{idx}"))
                                    .width(100.0)
                                    .selected_text(cm_label)
                                    .show_ui(ui, |ui| {
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.commit_mode,
                                                None,
                                                "Default",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.commit_mode,
                                                Some(CommitMode::Direct),
                                                "Direct",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut rule.commit_mode,
                                                Some(CommitMode::Preedit),
                                                "Pre-edit",
                                            )
                                            .changed();
                                    });
                            });
                        });
                    ui.add_space(6.0);
                }

                if let Some(i) = remove_idx {
                    self.config.app_rules.remove(i);
                    changed = true;
                }

                ui.add_space(8.0);
                self.thin_divider(ui);
                ui.add_space(10.0);

                ui.label(
                    RichText::new("NEW APPLICATION RULE")
                        .size(11.5)
                        .strong()
                        .color(p.text_muted),
                );
                ui.add_space(4.0);

                egui::Frame::new()
                    .fill(p.bg_card)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_max_width(content_width - 26.0);

                        if !self.open_windows.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Click a running Niri app-id:")
                                        .size(12.0)
                                        .color(p.text_secondary),
                                );
                                for w in &self.open_windows {
                                    if let Some(app_id) = &w.app_id
                                        && !app_id.is_empty()
                                    {
                                        let tag = egui::Button::new(
                                            RichText::new(app_id)
                                                .size(11.5)
                                                .family(FontFamily::Monospace)
                                                .color(p.tag_en_fg),
                                        )
                                        .fill(p.tag_en_bg)
                                        .stroke(Stroke::NONE)
                                        .corner_radius(CornerRadius::same(4));

                                        if ui.add(tag).clicked() {
                                            self.new_rule_name = format!("App: {app_id}");
                                            self.new_rule_app_id =
                                                format!("^{}$", regex_escape(app_id));
                                        }
                                    }
                                }
                            });
                            ui.add_space(6.0);
                        }

                        // Row 1 of form: Name + App-ID Regex
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Name:").size(12.5).color(p.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_rule_name)
                                    .hint_text("e.g. Ghostty Terminal")
                                    .desired_width(160.0),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("App-ID Regex:")
                                    .size(12.5)
                                    .color(p.text_secondary),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_rule_app_id)
                                    .hint_text("^com\\.mitchellh\\.ghostty$")
                                    .desired_width((ui.available_width() - 8.0).max(120.0)),
                            );
                        });

                        ui.add_space(6.0);

                        // Row 2 of form: Default Language + Commit Mode + Add Rule button
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Language:")
                                    .size(12.0)
                                    .color(p.text_secondary),
                            );
                            let new_lang_label = match self.new_rule_default_lang {
                                None => "Keep Current",
                                Some(true) => "Force VN",
                                Some(false) => "Auto EN",
                            };
                            egui::ComboBox::from_id_salt("new_rule_lang")
                                .width(115.0)
                                .selected_text(new_lang_label)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.new_rule_default_lang,
                                        None,
                                        "Keep Current",
                                    );
                                    ui.selectable_value(
                                        &mut self.new_rule_default_lang,
                                        Some(true),
                                        "Force VN",
                                    );
                                    ui.selectable_value(
                                        &mut self.new_rule_default_lang,
                                        Some(false),
                                        "Auto EN",
                                    );
                                });

                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Commit:")
                                    .size(12.0)
                                    .color(p.text_secondary),
                            );
                            let new_cm_label = match self.new_rule_commit_mode {
                                None => "Default",
                                Some(CommitMode::Direct) => "Direct",
                                Some(CommitMode::Preedit) => "Pre-edit",
                            };
                            egui::ComboBox::from_id_salt("new_rule_cm")
                                .width(105.0)
                                .selected_text(new_cm_label)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.new_rule_commit_mode,
                                        None,
                                        "Default",
                                    );
                                    ui.selectable_value(
                                        &mut self.new_rule_commit_mode,
                                        Some(CommitMode::Direct),
                                        "Direct",
                                    );
                                    ui.selectable_value(
                                        &mut self.new_rule_commit_mode,
                                        Some(CommitMode::Preedit),
                                        "Pre-edit",
                                    );
                                });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let add_btn = egui::Button::new(
                                        RichText::new("+ Add Rule")
                                            .size(12.5)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(p.accent_blue)
                                    .stroke(Stroke::NONE)
                                    .corner_radius(CornerRadius::same(6));

                                    if ui.add(add_btn).clicked()
                                        && !self.new_rule_app_id.trim().is_empty()
                                    {
                                        let name = if self.new_rule_name.trim().is_empty() {
                                            self.new_rule_app_id.clone()
                                        } else {
                                            self.new_rule_name.clone()
                                        };
                                        self.config.app_rules.push(AppRule {
                                            name,
                                            app_id_regex: self.new_rule_app_id.trim().to_string(),
                                            title_regex: None,
                                            default_enabled: self.new_rule_default_lang,
                                            commit_mode: self.new_rule_commit_mode,
                                        });
                                        self.new_rule_name.clear();
                                        self.new_rule_app_id.clear();
                                        changed = true;
                                    }
                                },
                            );
                        });
                    });
            });

        if changed {
            self.apply_and_save_config();
        }
    }

    fn render_sandbox_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Notion Callout Box
        egui::Frame::new()
            .fill(p.bg_card)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("TIP")
                            .size(10.5)
                            .strong()
                            .color(p.tag_en_fg),
                    );
                    ui.label(
                        RichText::new(
                            "Type raw Telex or VNI keystrokes below to preview live Vietnamese syllable composition and English word auto-restoration.",
                        )
                        .size(12.5)
                        .color(p.text_secondary),
                    );
                });
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new("RAW KEYSTROKE INPUT (CHUỖI PHÍM THÔ)")
                .size(11.5)
                .strong()
                .color(p.text_muted),
        );
        ui.add_space(4.0);

        ui.add(
            egui::TextEdit::multiline(&mut self.sandbox_raw_input)
                .font(FontId::new(14.0, FontFamily::Monospace))
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(14.0);
        let transformed = transform_text(&self.sandbox_raw_input, &self.config.engine);

        ui.label(
            RichText::new("RENDERED DOCUMENT PREVIEW (KẾT QUẢ HIỂN THỊ)")
                .size(11.5)
                .strong()
                .color(p.text_muted),
        );
        ui.add_space(4.0);

        egui::Frame::new()
            .fill(p.bg_card)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new(&transformed)
                        .size(18.0)
                        .color(p.text_primary),
                );
            });
    }
}

impl eframe::App for ViKeyGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Re-assert Notion palette every frame so winit's Wayland ThemeChanged events
        // cannot overwrite Light Mode visuals with dark separator lines or dark widgets.
        self.palette.apply_to_ctx(ctx);

        if self.last_poll.elapsed() >= Duration::from_secs(2) {
            self.poll_daemon();
        }

        let p = self.palette;

        let sidebar_resp = egui::SidePanel::left("notion_sidebar")
            .resizable(false)
            .exact_width(215.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.bg_sidebar)
                    .inner_margin(Margin::symmetric(14, 14)),
            )
            .show(ctx, |ui| {
                self.render_sidebar(ctx, ui);
            });

        // Paint a single subtle 1px Notion divider along the right edge of the sidebar
        let sb_rect = sidebar_resp.response.rect;
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("sidebar_border"),
        ))
        .line_segment(
            [
                egui::pos2(sb_rect.right(), sb_rect.top()),
                egui::pos2(sb_rect.right(), sb_rect.bottom()),
            ],
            Stroke::new(1.0_f32, p.border),
        );

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg_canvas)
                    .inner_margin(Margin::symmetric(32, 24)),
            )
            .show(ctx, |ui| {
                self.render_page_header(ctx, ui);
                match self.tab {
                    Tab::General => self.render_general_tab(ui),
                    Tab::AppRules => self.render_app_rules_tab(ui),
                    Tab::Sandbox => self.render_sandbox_tab(ui),
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.config.save();
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn gui_socket_path() -> std::path::PathBuf {
    let mut p = runtime_socket_path();
    p.set_file_name("unixkey-gui.sock");
    p
}

fn ensure_daemon_running() {
    if ViKeyGuiApp::send_ipc(&IpcRequest::GetStatus).is_some() {
        return;
    }
    let _ = Command::new("pkill").args(["-x", "fcitx5"]).status();
    let _ = Command::new("pkill").args(["-x", "ibus-daemon"]).status();
    let exe = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|d| d.join("unixkey-daemon")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| "unixkey-daemon".into());
    let _ = Command::new(exe).spawn();
    std::thread::sleep(Duration::from_millis(300));
}

fn run_window_worker() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("UniXKey — Vietnamese Input for Niri")
            .with_app_id("unixkey-gui")
            .with_decorations(false)
            .with_inner_size([840.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "UniXKey",
        options,
        Box::new(|cc| Ok(Box::new(ViKeyGuiApp::new(cc)))),
    )
}

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--window-worker") {
        return run_window_worker();
    }

    ensure_daemon_running();

    let sock_path = gui_socket_path();
    // If a resident unixkey-gui supervisor is already running minimized to tray, wake it up!
    if let Ok(mut stream) = UnixStream::connect(&sock_path) {
        let _ = stream.write_all(b"show\n");
        let _ = stream.flush();
        return Ok(());
    }

    let _ = fs::remove_file(&sock_path);
    let listener = std::os::unix::net::UnixListener::bind(&sock_path)
        .expect("Failed to bind unixkey-gui control socket");
    let _ = listener.set_nonblocking(true);

    let self_exe = std::env::current_exe().unwrap_or_else(|_| "unixkey-gui".into());
    let mut should_open_window = true;

    loop {
        if should_open_window {
            should_open_window = false;
            ensure_daemon_running();
            if let Ok(status) = Command::new(&self_exe).arg("--window-worker").status()
                && status.code() == Some(42)
            {
                // User clicked "Quit UniXKey Completely"
                break;
            }
            println!(
                "UniXKey minimized to system tray (VN/EN icon in top bar). Use tray menu 'Open UniXKey Settings...' to restore."
            );
        }

        // Wait for tray "Open UniXKey Settings..." or another `unixkey-gui` invocation
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream);
            let mut cmd = String::new();
            let _ = reader.read_line(&mut cmd);
            match cmd.trim() {
                "show" => {
                    should_open_window = true;
                }
                "quit" => {
                    break;
                }
                _ => {
                    should_open_window = true;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    let _ = fs::remove_file(&sock_path);
    Ok(())
}
