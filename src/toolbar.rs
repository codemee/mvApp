//! 工具列的扁平圖示按鈕與圖示（皆以 painter 繪製，不依賴圖檔）。
//!
//! 風格：單色細線條，無外框，只有滑過時的淺色圓角底。
//! 三態功能只用「一個」按鈕：圖示代表目前狀態，點一下切換到下一個狀態。

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// 圖示繪製函式：painter、圖示區域、前景色、按鈕底色（月亮挖空用）。
pub type IconFn = fn(&Painter, Rect, Color32, Color32);

/// 按鈕尺寸 34×34；圖示畫布 20×20 置中（四邊各留白 7）。
const BTN: Vec2 = Vec2::new(34.0, 34.0);
const CANVAS: f32 = 20.0;
const LINE: f32 = 1.5;

fn radius(r: Rect) -> f32 {
    r.width().min(r.height()) * 0.29
}

fn arc_points(c: Pos2, rad: f32, from_deg: f32, to_deg: f32) -> Vec<Pos2> {
    let steps = 28;
    (0..=steps)
        .map(|i| {
            let a = (from_deg + (to_deg - from_deg) * i as f32 / steps as f32).to_radians();
            c + Vec2::new(a.cos(), a.sin()) * rad
        })
        .collect()
}

/// 半實心圓（◐）：配色隨系統。
pub fn icon_system(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    let c = r.center();
    let rad = radius(r) * 1.0;
    p.add(Shape::convex_polygon(arc_points(c, rad, -90.0, 90.0), fg, Stroke::NONE));
    p.circle_stroke(c, rad, Stroke::new(LINE, fg));
}

/// 太陽：淺色。
pub fn icon_sun(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    let c = r.center();
    let rad = radius(r);
    let s = Stroke::new(LINE, fg);
    p.circle_stroke(c, rad * 0.52, s);
    for i in 0..8 {
        let a = (i as f32 * 45.0).to_radians();
        let d = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + d * rad * 0.82, c + d * rad * 1.22], s);
    }
}

/// 月亮：深色（兩圓相減成新月）。
pub fn icon_moon(p: &Painter, r: Rect, fg: Color32, bg: Color32) {
    let c = r.center();
    let rad = radius(r) * 1.1;
    p.circle_filled(c, rad, fg);
    p.circle_filled(c + Vec2::new(rad * 0.5, -rad * 0.42), rad * 0.84, bg);
}

/// 地球：語言隨系統。
pub fn icon_globe(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    let c = r.center();
    let rad = radius(r) * 1.0;
    let s = Stroke::new(LINE, fg);
    let thin = Stroke::new(1.1, fg);
    p.circle_stroke(c, rad, s);
    p.line_segment([c - Vec2::new(rad, 0.0), c + Vec2::new(rad, 0.0)], thin);
    // 經線（橢圓）
    let pts: Vec<Pos2> = (0..=32)
        .map(|i| {
            let a = (i as f32 / 32.0 * 360.0).to_radians();
            c + Vec2::new(a.cos() * rad * 0.45, a.sin() * rad)
        })
        .collect();
    p.add(Shape::line(pts, thin));
    p.line_segment([c - Vec2::new(0.0, rad), c + Vec2::new(0.0, rad)], thin);
}

pub fn icon_zh(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    p.text(r.center(), Align2::CENTER_CENTER, "中", FontId::proportional(15.0), fg);
}

pub fn icon_en(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    p.text(r.center(), Align2::CENTER_CENTER, "En", FontId::proportional(13.5), fg);
}

/// 重新整理：圓形箭頭。
pub fn icon_refresh(p: &Painter, r: Rect, fg: Color32, _bg: Color32) {
    let c = r.center();
    let rad = radius(r) * 1.0;
    let (from, to) = (-40.0f32, 250.0f32);
    p.add(Shape::line(arc_points(c, rad, from, to), Stroke::new(LINE, fg)));
    // 箭頭（順時針方向的切線）
    let a = to.to_radians();
    let e = c + Vec2::new(a.cos(), a.sin()) * rad;
    let t = Vec2::new(-a.sin(), a.cos());
    let n = Vec2::new(a.cos(), a.sin());
    p.add(Shape::convex_polygon(vec![e + t * 4.0, e - t * 1.8 + n * 3.4, e - t * 1.8 - n * 3.4], fg, Stroke::NONE));
}

/// 一般工具按鈕的圓角。
pub const RADIUS_NORMAL: f32 = 4.0;
/// 狀態（三態切換）按鈕的圓角。
pub const RADIUS_STATE: f32 = 8.0;

/// 扁平的圖示按鈕（無外框線，只有滑過時的圓角底色）。
/// 按鈕 34×34，圖示畫在置中的 20×20 畫布內；`corner` 為圓角（一般 4、狀態 8）。
/// 回傳是否剛被點擊；三態按鈕由呼叫端負責切換到下一個狀態。
pub fn icon_button(
    ui: &mut egui::Ui,
    tip: &str,
    icon: IconFn,
    corner: f32,
    fg: Color32,
    idle_bg: Color32,
    hover_bg: Color32,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(BTN, egui::Sense::click());
    let hovered = resp.hovered();
    let bg = if hovered { hover_bg } else { idle_bg };
    if hovered {
        ui.painter().rect_filled(rect, corner, hover_bg);
    }
    let canvas = Rect::from_center_size(rect.center(), Vec2::splat(CANVAS));
    icon(ui.painter(), canvas, fg, bg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

/// 工具列分隔線：寬 1，上下外距 5；左右外距 4 由相鄰項目的間距（4）提供。
pub fn separator(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, BTN.y), egui::Sense::hover());
    let x = rect.center().x;
    ui.painter().line_segment(
        [Pos2::new(x, rect.top() + 5.0), Pos2::new(x, rect.bottom() - 5.0)],
        Stroke::new(1.0, color),
    );
}
