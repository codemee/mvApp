//! egui UI：工具列＋每個螢幕一個群組窗格，以圖示列出視窗，支援拖放搬移。
//!
//! 風格：Windows 11 Fluent 式的輕量扁平——中性底色、細框群組、標題嵌在框線上、
//! 內部白色清單區、灰色細線條圖示。鋼彈（淺色）／EVA 初號機（深色）的色彩只用在強調色。

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Stroke, ThemePreference, Vec2};
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow};

use crate::i18n::{self, Lang, LangPref, Tr};
use crate::icons::IconCache;
use crate::monitors::{self, Monitor};
use crate::mover::{self, MoveError};
use crate::settings::Settings;
use crate::toolbar::{self, IconFn};
use crate::tray::{self, Tray};
use crate::windows_list::{self, WinInfo, hwnd_of};

const REFRESH: Duration = Duration::from_millis(600);
const ICON_SIZE: f32 = 40.0;
const CELL_W: f32 = 84.0;
const CELL_H: f32 = 84.0;

/// 窗格最小寬度；容不下所有螢幕並排時，改成上下排列。
const MIN_PANE_W: f32 = 300.0;
/// 上下排列時每個窗格的最小高度。
const MIN_PANE_H: f32 = 230.0;

/// 群組框上緣預留給「嵌在框線上的標題」的高度。
const TITLE_GAP: f32 = 10.0;
/// 群組框內距（上方較大，讓出標題位置）。
const GROUP_PAD: f32 = 10.0;
const GROUP_PAD_TOP: f32 = 16.0;
/// 內部白色清單區的內距。
const LIST_PAD: f32 = 6.0;
/// 窗格內除了清單內容區以外的固定高度（標題空隙＋兩層框的內距與框線）。
const CHROME_H: f32 = TITLE_GAP + GROUP_PAD_TOP + GROUP_PAD + LIST_PAD * 2.0 + 6.0;

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// 配色。
struct Palette {
    /// 視窗底色（工具列、狀態列與其相同）。
    bg: Color32,
    /// 群組框底色。
    group: Color32,
    /// 細框線。
    border: Color32,
    /// 內部清單區底色。
    inner: Color32,
    text: Color32,
    /// 次要文字與工具列圖示。
    muted: Color32,
    /// 滑過時的底色。
    hover: Color32,
    /// 各螢幕的強調色（用在螢幕圖示）。
    accents: [Color32; 5],
    /// 拖放目標高亮。
    accent: Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            // 新世紀福音戰士 初號機：暗紫機體 + 螢光綠，搭配二號機橘紅、零號機黃
            Palette {
                bg: rgb(0x16121F),
                group: rgb(0x1E1830),
                border: rgb(0x3B2F58),
                inner: rgb(0x120E1B),
                text: rgb(0xE8E4F2),
                muted: rgb(0xA89FC0),
                hover: rgb(0x2D2446),
                accents: [rgb(0x7CFF3A), rgb(0xFF7A1A), rgb(0xB57CFF), rgb(0xFF3B4E), rgb(0xFFD60A)],
                accent: rgb(0x7CFF3A),
            }
        } else {
            // 鋼彈 RX-78-2：機體白 + 藍，搭配紅、黃與膝甲灰
            Palette {
                bg: rgb(0xF4F5F7),
                group: rgb(0xEDEFF3),
                border: rgb(0xD3D8E0),
                inner: rgb(0xFFFFFF),
                text: rgb(0x1F2937),
                muted: rgb(0x5F6B7A),
                hover: rgb(0xE1E5EB),
                accents: [rgb(0x1C4E9C), rgb(0xC8102E), rgb(0xE0A100), rgb(0x5B6573), rgb(0x2F7AC9)],
                accent: rgb(0x1C4E9C),
            }
        }
    }

    fn monitor_accent(&self, i: usize) -> Color32 {
        self.accents[i % self.accents.len()]
    }
}

/// 套用樣式：細框線、圓角、依配色設定底色（淺色、深色各設定一次）。
fn apply_styles(ctx: &egui::Context) {
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        let pal = Palette::new(dark);
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            v.override_text_color = Some(pal.text);
            v.panel_fill = pal.bg;
            v.extreme_bg_color = pal.inner;
            v.window_fill = pal.group;
            v.selection.bg_fill = mix(pal.group, pal.accent, 0.30);
            v.selection.stroke = Stroke::new(1.0, pal.accent);
            let w = &mut v.widgets;
            // 面板之間的分隔線
            w.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
            for ws in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                ws.expansion = 0.0;
                ws.corner_radius = egui::CornerRadius::same(6);
            }
            // 拖曳時，放置區會改用這兩組樣式重畫：一般狀態與靜止時相同，
            // 可放置且游標在上方時，換成強調色外框與淡淡的強調色底，作為回饋。
            w.inactive.bg_fill = pal.group;
            w.inactive.weak_bg_fill = pal.group;
            w.inactive.bg_stroke = Stroke::new(1.0, pal.border);
            w.hovered.bg_stroke = Stroke::new(1.0, pal.border);
            w.active.bg_fill = mix(pal.group, pal.accent, 0.12);
            w.active.weak_bg_fill = w.active.bg_fill;
            w.active.bg_stroke = Stroke::new(1.5, pal.accent);
        });
    }
}

/// 拖放的 payload：視窗 HWND。
struct DragWin(isize);

/// 狀態列顯示的內容（以資料保存，語言切換時才能即時重新翻譯）。
enum Status {
    Hint,
    Moved { title: String, to: usize },
    Failed(MoveError),
}

pub struct App {
    monitors: Vec<Monitor>,
    windows: Vec<WinInfo>,
    icons: IconCache,
    last: Instant,
    status: Status,
    settings: Settings,
    system_lang: Lang,
    title_lang: Option<Lang>,
    /// 系統匣；建立失敗時為 None（此時直接顯示主視窗，且關閉視窗＝結束程式）。
    tray: Option<Tray>,
    /// 已執行的啟動影格數（用來在 eframe 顯示視窗之後把它藏起來）。
    startup_frames: u8,
}

impl App {
    pub fn new(ctx: &egui::Context, icon_png: &[u8]) -> Self {
        apply_styles(ctx);
        let settings = Settings::load();
        ctx.set_theme(settings.theme);
        let system_lang = i18n::system_lang();
        let tr = Tr(settings.lang.resolve(system_lang));
        let tray = match Tray::new(ctx, icon_png, tr) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("無法建立系統匣圖示：{e}");
                tray::show_main(); // 沒有系統匣就直接顯示視窗，避免使用者找不到程式
                None
            }
        };
        let mut a = App {
            monitors: Vec::new(),
            windows: Vec::new(),
            icons: IconCache::default(),
            last: Instant::now(),
            status: Status::Hint,
            settings,
            system_lang,
            title_lang: None,
            tray,
            startup_frames: 0,
        };
        a.refresh();
        a
    }

    fn lang(&self) -> Lang {
        self.settings.lang.resolve(self.system_lang)
    }

    fn refresh(&mut self) {
        self.monitors = monitors::list();
        self.windows = windows_list::list();
        self.system_lang = i18n::system_lang();
        let alive: std::collections::HashSet<isize> = self.windows.iter().map(|w| w.hwnd).collect();
        self.icons.retain(|h| alive.contains(&h));
        self.last = Instant::now();
    }
}

/// 畫小螢幕圖示（細線外框＋底座）於 `rect`。
fn paint_monitor_glyph(p: &egui::Painter, rect: egui::Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let body = egui::Rect::from_min_max(rect.min + Vec2::new(1.0, 1.0), egui::pos2(rect.right() - 1.0, rect.bottom() - 4.0));
    p.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
    let cx = rect.center().x;
    p.line_segment([egui::pos2(cx, body.bottom()), egui::pos2(cx, rect.bottom() - 1.5)], stroke);
    p.line_segment([egui::pos2(cx - 4.0, rect.bottom() - 1.0), egui::pos2(cx + 4.0, rect.bottom() - 1.0)], stroke);
}

/// 畫一個螢幕窗格：細框群組（標題嵌在框線上）＋內部白色清單區（內含視窗圖示）。
#[allow(clippy::too_many_arguments)]
fn monitor_pane(
    ui: &mut egui::Ui,
    index: usize,
    m: &Monitor,
    pane_w: f32,
    pane_h: f32,
    windows: &[WinInfo],
    icons: &mut IconCache,
    pal: &Palette,
    tr: Tr,
    focus: &mut Option<isize>,
    moves: &mut Vec<(isize, usize)>,
) {
    let accent = pal.monitor_accent(index);
    let layout = egui::Layout::top_down(egui::Align::Min);
    ui.allocate_ui_with_layout(Vec2::new(pane_w, pane_h), layout, |ui| {
        ui.set_width(pane_w);
        ui.add_space(TITLE_GAP);

        let zone_h = (pane_h - CHROME_H).max(60.0);
        let group_w = pane_w - GROUP_PAD * 2.0 - 2.0;
        let list_w = group_w - LIST_PAD * 2.0 - 2.0;

        // 群組框：細框線、圓角；上方內距較大，讓出標題的位置
        let group = egui::Frame::new()
            .fill(pal.group)
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(8)
            .inner_margin(egui::Margin {
                left: GROUP_PAD as i8,
                right: GROUP_PAD as i8,
                top: GROUP_PAD_TOP as i8,
                bottom: GROUP_PAD as i8,
            });
        let (inner, dropped) = ui.dnd_drop_zone::<DragWin, ()>(group, |ui| {
            ui.set_width(group_w);
            // 內部白色清單區
            egui::Frame::new()
                .fill(pal.inner)
                .stroke(Stroke::new(1.0, pal.border))
                .corner_radius(6)
                .inner_margin(LIST_PAD)
                .show(ui, |ui| {
                    ui.set_width(list_w);
                    ui.set_min_height(zone_h);
                    egui::ScrollArea::vertical()
                        .id_salt(("pane", index))
                        .auto_shrink([false, false])
                        .max_width(list_w)
                        .max_height(zone_h)
                        .show(ui, |ui| {
                            let wins: Vec<&WinInfo> = windows.iter().filter(|w| w.monitor == m.handle).collect();
                            let step = CELL_W + ui.spacing().item_spacing.x;
                            let cols = (((list_w - 20.0) / step).floor() as usize).max(1);
                            for row in wins.chunks(cols) {
                                ui.horizontal(|ui| {
                                    for &w in row {
                                        window_cell(ui, w, icons, pal, tr, focus);
                                    }
                                });
                            }
                        });
                });
        });

        // 標題嵌在上緣框線上：先用視窗底色蓋住那一段框線，再畫圖示與文字
        let r = inner.response.rect;
        let title = tr.monitor_title(index + 1, m.primary, m.rect.width(), m.rect.height());
        let galley = ui.painter().layout_no_wrap(title, egui::FontId::proportional(13.0), pal.text);
        let glyph = Vec2::new(18.0, 14.0);
        let pad = 6.0;
        let total = Vec2::new(glyph.x + 6.0 + galley.size().x, galley.size().y.max(glyph.y));
        let origin = egui::pos2(r.left() + 14.0, r.top() - total.y / 2.0);
        let title_rect = egui::Rect::from_min_size(origin, total);
        let p = ui.painter();
        p.rect_filled(title_rect.expand2(Vec2::new(pad, 0.0)), 0.0, pal.bg);
        let glyph_rect = egui::Rect::from_min_size(
            egui::pos2(origin.x, origin.y + (total.y - glyph.y) / 2.0),
            glyph,
        );
        paint_monitor_glyph(p, glyph_rect, accent);
        let text_pos = egui::pos2(origin.x + glyph.x + 6.0, origin.y + (total.y - galley.size().y) / 2.0);
        p.galley(text_pos, galley, pal.text);
        ui.interact(title_rect, ui.scope_id().with(("monitor-title", index)), egui::Sense::hover())
            .on_hover_text(tr.monitor_tip(&m.name, m.dpi));

        if let Some(p) = dropped {
            let from = windows.iter().find(|w| w.hwnd == p.0).map(|w| w.monitor);
            if from != Some(m.handle) {
                moves.push((p.0, index));
            }
        }
    });
}

/// 一個可拖曳的視窗圖示格（滑過時有淺色圓角底）。
fn window_cell(ui: &mut egui::Ui, w: &WinInfo, icons: &mut IconCache, pal: &Palette, tr: Tr, focus: &mut Option<isize>) {
    let id = ui.scope_id().with(("win", w.hwnd));
    let r = ui.dnd_drag_source(id, DragWin(w.hwnd), |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(CELL_W, CELL_H), egui::Sense::hover());
        if ui.rect_contains_pointer(rect) {
            ui.painter().rect_filled(rect, 8.0, pal.hover);
        }
        let painter = ui.painter_at(rect);
        let icon_rect = egui::Rect::from_center_size(
            rect.center_top() + Vec2::new(0.0, 6.0 + ICON_SIZE / 2.0),
            Vec2::splat(ICON_SIZE),
        );
        match icons.get(ui.ctx(), w.hwnd, &w.exe_path) {
            Some(t) => {
                painter.image(
                    t.id(),
                    icon_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(icon_rect, 6.0, Color32::from_gray(120));
            }
        }
        let color = if w.minimized { mix(pal.text, pal.inner, 0.55) } else { pal.text };
        // 標題最多兩行，超出以「…」結尾
        let mut job = egui::text::LayoutJob::simple(
            w.title.clone(),
            egui::FontId::proportional(11.0),
            color,
            CELL_W - 8.0,
        );
        job.wrap.max_rows = 2;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        painter.galley(egui::pos2(rect.left() + 4.0, icon_rect.bottom() + 4.0), galley, color);
    });
    let resp = r.response.on_hover_text(format!(
        "{}\n{}{}",
        w.title,
        w.exe_name(),
        if w.minimized { tr.minimized() } else { "" }
    ));
    if resp.clicked() {
        *focus = Some(w.hwnd);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // eframe 會在第一幀畫完後強制顯示視窗，所以要等到第二幀才能縮回系統匣；
        // 立刻要求重繪，讓視窗閃現的時間降到最短。
        if self.startup_frames < 2 {
            self.startup_frames += 1;
            if self.startup_frames == 2 && self.tray.is_some() {
                tray::hide_main();
            } else {
                ctx.request_repaint();
            }
        }
        // 拖曳期間不重新整理，避免清單抖動
        if !egui::DragAndDrop::has_any_payload(ctx) && self.last.elapsed() >= REFRESH {
            self.refresh();
        }
        // 按下關閉：有系統匣時改為縮回系統匣（從系統匣選單結束時則放行）
        if self.tray.is_some()
            && !tray::quit_requested()
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_main();
        }
        // 語言改變時更新視窗標題與系統匣文字
        let lang = self.lang();
        if self.title_lang != Some(lang) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(Tr(lang).app_title().to_owned()));
            if let Some(t) = &self.tray {
                t.set_lang(Tr(lang));
            }
            self.title_lang = Some(lang);
        }
        ctx.request_repaint_after(REFRESH);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut moves: Vec<(isize, usize)> = Vec::new();
        let mut focus: Option<isize> = None;
        let mut refresh_now = false;
        let tr = Tr(self.lang());
        let pal = Palette::new(ui.visuals().dark_mode);

        // 工具列：左側為重新整理；右側為兩個扁平三態單鍵（語言、配色），點一下切換到下一個狀態
        let mut new_settings = self.settings;
        egui::Panel::top("toolbar")
            // 工具列：內距四邊 4、按鈕 34×34 → 高度 42；同組按鈕間距 4
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::same(4)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    let rn = toolbar::RADIUS_NORMAL;
                    if toolbar::icon_button(ui, tr.refresh(), toolbar::icon_refresh, rn, pal.muted, pal.bg, pal.hover) {
                        refresh_now = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 右到左：先放配色（最右），再放語言
                        let (icon, tip, next): (IconFn, &str, ThemePreference) = match self.settings.theme {
                            ThemePreference::System => (toolbar::icon_system, tr.theme_system(), ThemePreference::Light),
                            ThemePreference::Light => (toolbar::icon_sun, tr.theme_light(), ThemePreference::Dark),
                            ThemePreference::Dark => (toolbar::icon_moon, tr.theme_dark(), ThemePreference::System),
                        };
                        let tip = format!("{tip}{}", tr.cycle_hint());
                        let rs = toolbar::RADIUS_STATE;
                        if toolbar::icon_button(ui, &tip, icon, rs, pal.muted, pal.bg, pal.hover) {
                            new_settings.theme = next;
                        }

                        let (icon, tip, next): (IconFn, &str, LangPref) = match self.settings.lang {
                            LangPref::System => (toolbar::icon_globe, tr.lang_system(), LangPref::Zh),
                            LangPref::Zh => (toolbar::icon_zh, tr.lang_zh(), LangPref::En),
                            LangPref::En => (toolbar::icon_en, tr.lang_en(), LangPref::System),
                        };
                        let tip = format!("{tip}{}", tr.cycle_hint());
                        if toolbar::icon_button(ui, &tip, icon, rs, pal.muted, pal.bg, pal.hover) {
                            new_settings.lang = next;
                        }
                        // 與左側（重新整理）區隔的分隔線
                        toolbar::separator(ui, pal.border);
                    });
                });
            });
        if refresh_now {
            self.last = Instant::now() - REFRESH;
        }
        if new_settings != self.settings {
            if new_settings.theme != self.settings.theme {
                ui.ctx().set_theme(new_settings.theme);
            }
            self.settings = new_settings;
            self.settings.save();
        }

        let status_text = match &self.status {
            Status::Hint => tr.hint().to_owned(),
            Status::Moved { title, to } => tr.moved(title, *to),
            Status::Failed(e) => tr.move_failed(e),
        };
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(12, 5)))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(status_text).size(12.0).color(pal.muted));
            });

        let Self { monitors, windows, icons, .. } = self;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| {
                if monitors.is_empty() {
                    ui.label(tr.no_monitor());
                    return;
                }
                let n = monitors.len();
                let gap = ui.spacing().item_spacing.x;
                let avail_w = ui.available_width();
                let avail_h = ui.available_height();
                let side_by_side = avail_w >= n as f32 * MIN_PANE_W + gap * (n as f32 - 1.0);

                if side_by_side {
                    // 左右並排，窗格等寬
                    let pane_w = (avail_w - gap * (n as f32 - 1.0)) / n as f32;
                    ui.horizontal_top(|ui| {
                        for (i, m) in monitors.iter().enumerate() {
                            monitor_pane(ui, i, m, pane_w, avail_h, windows, icons, &pal, tr, &mut focus, &mut moves);
                        }
                    });
                } else {
                    // 視窗太窄：上下排列，整體可捲動
                    let gap_v = ui.spacing().item_spacing.y;
                    let pane_h = ((avail_h - gap_v * (n as f32 - 1.0)) / n as f32).max(MIN_PANE_H);
                    egui::ScrollArea::vertical().id_salt("panes").auto_shrink([false, false]).show(ui, |ui| {
                        let pane_w = (ui.available_width() - 4.0).max(160.0);
                        for (i, m) in monitors.iter().enumerate() {
                            monitor_pane(ui, i, m, pane_w, pane_h, windows, icons, &pal, tr, &mut focus, &mut moves);
                        }
                    });
                }
            });

        let mut changed = false;
        if let Some(h) = focus {
            unsafe {
                let hw = hwnd_of(h);
                if IsIconic(hw).as_bool() {
                    let _ = ShowWindow(hw, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hw);
            }
        }
        for (hwnd, idx) in moves {
            let name = self.windows.iter().find(|w| w.hwnd == hwnd).map(|w| w.title.clone());
            self.status = match mover::move_window(hwnd, &self.monitors[idx], &self.monitors) {
                Ok(()) => Status::Moved { title: name.unwrap_or_default(), to: idx + 1 },
                Err(e) => Status::Failed(e),
            };
            changed = true;
        }
        if changed {
            self.refresh();
        }
    }
}
