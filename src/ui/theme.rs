//! The "Werkbank" look: palettes for dark and light, embedded fonts and small styled widgets.
use eframe::egui::{
    self, epaint::Shadow, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Margin, Painter, Pos2, Rect, Response, RichText, Stroke, TextStyle, Theme, Vec2,
    Visuals,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_on: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub warn_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub monitor: Color32,
    pub monitor_edge: Color32,
    pub hatch: Color32,
    pub zone: Color32,
    pub zone_edge: Color32,
}

pub fn dark() -> Palette {
    Palette {
        bg: Color32::from_rgb(0x11, 0x13, 0x15),
        panel: Color32::from_rgb(0x16, 0x19, 0x1C),
        raised: Color32::from_rgb(0x1E, 0x22, 0x26),
        hover: Color32::from_rgb(0x25, 0x2A, 0x2F),
        border: Color32::from_rgb(0x2A, 0x2F, 0x35),
        text: Color32::from_rgb(0xE6, 0xE8, 0xEA),
        muted: Color32::from_rgb(0x9A, 0xA3, 0xAD),
        accent: Color32::from_rgb(0x3F, 0xB8, 0x9A),
        accent_on: Color32::from_rgb(0x0B, 0x1F, 0x1A),
        accent_soft: Color32::from_rgb(0x1C, 0x30, 0x2E),
        ok: Color32::from_rgb(0x3F, 0xB8, 0x9A),
        warn: Color32::from_rgb(0xF0, 0xA3, 0x5E),
        warn_soft: Color32::from_rgb(0x35, 0x2C, 0x25),
        danger: Color32::from_rgb(0xF0, 0x8A, 0x6C),
        danger_soft: Color32::from_rgb(0x35, 0x29, 0x27),
        monitor: Color32::from_rgb(0x1A, 0x1E, 0x22),
        monitor_edge: Color32::from_rgb(0x3A, 0x41, 0x49),
        hatch: Color32::from_rgb(0x34, 0x39, 0x3F),
        zone: Color32::from_rgb(0x24, 0x28, 0x2C),
        zone_edge: Color32::from_rgb(0x4A, 0x52, 0x5B),
    }
}

pub fn light() -> Palette {
    Palette {
        bg: Color32::from_rgb(0xF3, 0xF4, 0xF6),
        panel: Color32::WHITE,
        raised: Color32::from_rgb(0xF5, 0xF6, 0xF8),
        hover: Color32::from_rgb(0xEB, 0xED, 0xF0),
        border: Color32::from_rgb(0xDC, 0xDF, 0xE3),
        text: Color32::from_rgb(0x16, 0x19, 0x1C),
        muted: Color32::from_rgb(0x59, 0x61, 0x6A),
        accent: Color32::from_rgb(0x1A, 0x7F, 0x67),
        accent_on: Color32::WHITE,
        accent_soft: Color32::from_rgb(0xE6, 0xF2, 0xEF),
        ok: Color32::from_rgb(0x1A, 0x7F, 0x67),
        warn: Color32::from_rgb(0xA3, 0x5A, 0x12),
        warn_soft: Color32::from_rgb(0xF6, 0xEF, 0xE7),
        danger: Color32::from_rgb(0xB4, 0x43, 0x2A),
        danger_soft: Color32::from_rgb(0xF9, 0xEF, 0xEC),
        monitor: Color32::WHITE,
        monitor_edge: Color32::from_rgb(0xB9, 0xBF, 0xC6),
        hatch: Color32::from_rgb(0xCF, 0xD3, 0xD8),
        zone: Color32::from_rgb(0xF4, 0xF5, 0xF7),
        zone_edge: Color32::from_rgb(0xA3, 0xAA, 0xB2),
    }
}

/// The palette of the theme the context currently shows.
pub fn palette(ctx: &egui::Context) -> Palette {
    match ctx.theme() {
        Theme::Dark => dark(),
        Theme::Light => light(),
    }
}

const STRONG: &str = "strong";

/// Inter SemiBold, for headings, names and primary buttons.
pub fn strong_family() -> FontFamily {
    FontFamily::Name(STRONG.into())
}

/// Embeds the fonts and installs both themes' visuals and the shared spacing.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Dark, visuals(&dark(), true));
    ctx.set_visuals_of(Theme::Light, visuals(&light(), false));
    install_spacing(ctx);
}

/// Inter and JetBrains Mono are the only fonts: egui's bundled ones (about 1 MB resident)
/// are left out, so every symbol in the UI must exist in these two or be painted.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    for (name, bytes) in [
        (
            "inter",
            &include_bytes!("../../assets/fonts/Inter-Regular.ttf")[..],
        ),
        (
            "inter-semibold",
            &include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")[..],
        ),
        (
            "mono",
            &include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // Each family falls back to the other fonts for glyphs it lacks.
    for (family, order) in [
        (
            FontFamily::Proportional,
            ["inter", "mono", "inter-semibold"],
        ),
        (strong_family(), ["inter-semibold", "inter", "mono"]),
        (FontFamily::Monospace, ["mono", "inter", "inter-semibold"]),
    ] {
        fonts
            .families
            .insert(family, order.iter().map(|name| name.to_string()).collect());
    }
    ctx.set_fonts(fonts);
}

fn install_spacing(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.window_margin = Margin::same(16);
        style.interaction.selectable_labels = false;
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Heading, FontId::new(20.0, strong_family())),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
    });
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(6);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.raised;
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.raised;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    let shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 40 }),
    };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.muted);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.active.bg_fill = p.hover;
    w.active.weak_bg_fill = p.hover;
    w.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.active.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.open.bg_fill = p.raised;
    w.open.weak_bg_fill = p.raised;
    w.open.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    v
}

pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(strong_family())
}

pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).monospace()
}

/// Small uppercase section label.
pub fn caps(ui: &mut egui::Ui, text: &str) -> Response {
    let p = palette(ui.ctx());
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .family(strong_family())
            .extra_letter_spacing(0.6)
            .color(p.muted),
    )
}

pub fn primary(ui: &mut egui::Ui, text: &str) -> Response {
    primary_enabled(ui, true, text)
}

pub fn primary_enabled(ui: &mut egui::Ui, enabled: bool, text: &str) -> Response {
    let p = palette(ui.ctx());
    let (fill, color) = if enabled {
        (p.accent, p.accent_on)
    } else {
        (p.raised, p.muted)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(strong(text).color(color))
            .fill(fill)
            .stroke(Stroke::new(
                1.0_f32,
                if enabled { p.accent } else { p.border },
            ))
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

/// Secondary button with the standard height.
pub fn button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> Response {
    ui.add(egui::Button::new(text).min_size(Vec2::new(0.0, 30.0)))
}

/// Text-only accent button.
pub fn ghost(ui: &mut egui::Ui, text: &str) -> Response {
    let p = palette(ui.ctx());
    ui.add(
        egui::Button::new(RichText::new(text).color(p.accent))
            .frame(false)
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

/// A key cap such as `Ctrl`.
pub fn kbd(ui: &mut egui::Ui, text: &str) {
    let p = palette(ui.ctx());
    Frame::NONE
        .fill(p.raised)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(mono(text).color(p.text));
        });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Ok,
    Warn,
    Danger,
    Muted,
    Accent,
}

impl Tone {
    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Ok => p.ok,
            Self::Warn => p.warn,
            Self::Danger => p.danger,
            Self::Muted => p.muted,
            Self::Accent => p.accent,
        }
    }
}

/// Rounded status pill with a colored dot.
pub fn pill(ui: &mut egui::Ui, text: &str, tone: Tone) -> Response {
    let p = palette(ui.ctx());
    let alert = matches!(tone, Tone::Warn | Tone::Danger | Tone::Accent);
    let (fill, edge) = match tone {
        Tone::Warn => (p.warn_soft, p.warn),
        Tone::Danger => (p.danger_soft, p.danger),
        Tone::Accent => (p.accent_soft, p.accent),
        _ => (Color32::TRANSPARENT, p.border),
    };
    Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, edge))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.horizontal(|ui| {
                dot(ui, tone.color(&p));
                ui.label(RichText::new(text).size(12.0).color(if alert {
                    p.text
                } else {
                    p.muted
                }));
            });
        })
        .response
}

pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

/// Diagonal hatching, used for screen areas reserved by panels and docks.
pub fn hatch(painter: &Painter, rect: Rect, color: Color32) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let step = 6.0;
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom()),
                Pos2::new(x + rect.height(), rect.top()),
            ],
            Stroke::new(1.0_f32, color),
        );
        x += step;
    }
}

/// The HerdCtrl mark: two tiles over a wide one.
pub fn logo(ui: &mut egui::Ui, size: f32) {
    let p = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let unit = size / 24.0;
    let stroke = Stroke::new(2.0 * unit, p.accent);
    for (x, y, w, h) in [
        (3.0, 3.0, 8.0, 8.0),
        (13.0, 3.0, 8.0, 8.0),
        (3.0, 13.0, 18.0, 8.0),
    ] {
        ui.painter().rect_stroke(
            Rect::from_min_size(rect.min + Vec2::new(x, y) * unit, Vec2::new(w, h) * unit),
            1.5 * unit,
            stroke,
            egui::StrokeKind::Middle,
        );
    }
}

/// A painted icon for the appearance toggle: half disc (system), crescent (dark) or sun (light).
pub fn appearance_icon(ui: &mut egui::Ui, appearance: Appearance) -> Response {
    let p = palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.hover);
    }
    let center = rect.center();
    let color = if response.hovered() { p.text } else { p.muted };
    let painter = ui.painter();
    match appearance {
        Appearance::System => {
            painter.circle_stroke(center, 6.5, Stroke::new(1.5_f32, color));
            let half: Vec<Pos2> = (0..=16)
                .map(|step| {
                    let angle =
                        std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / 16.0;
                    center + 6.5 * Vec2::angled(angle)
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Appearance::Dark => {
            painter.circle_filled(center, 6.5, color);
            painter.circle_filled(
                center + Vec2::new(3.5, -3.0),
                5.5,
                if response.hovered() { p.hover } else { p.panel },
            );
        }
        Appearance::Light => {
            painter.circle_filled(center, 3.5, color);
            for step in 0..8 {
                let direction = Vec2::angled(std::f32::consts::TAU * step as f32 / 8.0);
                painter.line_segment(
                    [center + direction * 5.5, center + direction * 8.0],
                    Stroke::new(1.5_f32, color),
                );
            }
        }
    }
    response
}

/// The user's appearance choice; `System` follows the desktop's light or dark style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

impl Appearance {
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Dark,
            Self::Dark => Self::Light,
            Self::Light => Self::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Appearance: system",
            Self::Dark => "Appearance: dark",
            Self::Light => "Appearance: light",
        }
    }

    pub fn theme(self, system: Option<Theme>) -> Theme {
        match self {
            Self::Dark => Theme::Dark,
            Self::Light => Theme::Light,
            Self::System => system.unwrap_or(Theme::Dark),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    appearance: Appearance,
}

fn settings_path() -> Option<PathBuf> {
    crate::core::config::Config::path()
        .ok()
        .and_then(|path| Some(path.parent()?.join("ui.json")))
}

/// The saved appearance; editor-only, so it lives beside the layouts in `ui.json`.
pub fn load_appearance() -> Appearance {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|raw| serde_json::from_slice::<Settings>(&raw).ok())
        .unwrap_or_default()
        .appearance
}

pub fn store_appearance(appearance: Appearance) -> Result<(), String> {
    let path = settings_path().ok_or("Configuration directory unavailable")?;
    std::fs::create_dir_all(path.parent().ok_or("Invalid settings path")?)
        .map_err(|e| e.to_string())?;
    let raw = serde_json::to_vec_pretty(&Settings { appearance }).map_err(|e| e.to_string())?;
    std::fs::write(path, raw).map_err(|e| e.to_string())
}

/// Reads the desktop's light/dark preference from the XDG settings portal (blocking; run on a worker).
pub fn system_theme() -> Option<Theme> {
    let connection = zbus::blocking::Connection::session().ok()?;
    let reply = connection
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "ReadOne",
            &("org.freedesktop.appearance", "color-scheme"),
        )
        .ok()?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
    match u32::try_from(value).ok()? {
        1 => Some(Theme::Dark),
        // 0 means no preference; GNOME's default style is light.
        _ => Some(Theme::Light),
    }
}

/// A painted magnifier for search fields; the embedded fonts have no such glyph.
pub fn search_icon(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
    let stroke = Stroke::new(1.6_f32, color);
    let center = rect.center() - Vec2::splat(1.5);
    ui.painter().circle_stroke(center, 5.0, stroke);
    let handle = Vec2::splat(5.0 / std::f32::consts::SQRT_2);
    ui.painter()
        .line_segment([center + handle, center + handle * 2.0], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_tone_uses_the_accent_role() {
        for p in [dark(), light()] {
            assert_eq!(Tone::Accent.color(&p), p.accent);
        }
    }

    #[test]
    fn appearance_cycles_and_falls_back_to_dark() {
        assert_eq!(Appearance::System.next(), Appearance::Dark);
        assert_eq!(Appearance::Light.next(), Appearance::System);
        assert_eq!(Appearance::System.theme(None), Theme::Dark);
        assert_eq!(Appearance::System.theme(Some(Theme::Light)), Theme::Light);
        assert_eq!(Appearance::Light.theme(Some(Theme::Dark)), Theme::Light);
        let raw = serde_json::to_string(&Settings {
            appearance: Appearance::Light,
        })
        .unwrap();
        assert_eq!(raw, r#"{"appearance":"light"}"#);
    }

    #[test]
    fn text_colors_keep_readable_contrast() {
        // WCAG relative luminance contrast, 4.5:1 for body text.
        fn luminance(c: Color32) -> f64 {
            let channel = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        fn contrast(a: Color32, b: Color32) -> f64 {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }
        for p in [dark(), light()] {
            for ground in [p.bg, p.panel, p.raised] {
                assert!(contrast(p.text, ground) >= 4.5);
                assert!(contrast(p.muted, ground) >= 4.5);
            }
            assert!(contrast(p.accent_on, p.accent) >= 4.5);
        }
    }
}
