//! Le thème et les petits éléments de l'interface. Les icônes sont dessinées à
//! la main : la police par défaut n'a ni flèches, ni lecture, ni plein écran.

use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2,
};

/// Les couleurs de l'appli.
#[derive(Clone, Copy)]
pub struct Theme {
    /// Son nom dans les réglages (`State::theme`).
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    pub background: Color32,
    /// Cartes, champs, boutons.
    pub surface: Color32,
    pub accent: Color32,
    /// Le texte posé sur l'accent.
    pub on_accent: Color32,
    pub text: Color32,
    pub muted: Color32,
}

impl Theme {
    /// Le reste de la planche, autour de la case lue : à peine visible.
    pub fn dim(&self) -> Color32 {
        let [r, g, b, _] = self.background.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, 232)
    }

    /// Une surface un peu plus marquée (survol, champ actif).
    pub fn raised(&self) -> Color32 {
        self.surface.lerp_to_gamma(self.text, 0.08)
    }
}

pub const THEMES: [Theme; 5] = [
    Theme {
        id: "nuit",
        name: "Nuit",
        dark: true,
        background: Color32::from_rgb(16, 16, 19),
        surface: Color32::from_rgb(30, 30, 35),
        accent: Color32::from_rgb(240, 196, 92),
        on_accent: Color32::from_rgb(28, 22, 8),
        text: Color32::from_rgb(232, 232, 236),
        muted: Color32::from_rgb(140, 140, 152),
    },
    Theme {
        id: "encre",
        name: "Encre bleue",
        dark: true,
        background: Color32::from_rgb(11, 16, 30),
        surface: Color32::from_rgb(23, 32, 54),
        accent: Color32::from_rgb(94, 200, 255),
        on_accent: Color32::from_rgb(4, 20, 34),
        text: Color32::from_rgb(226, 234, 246),
        muted: Color32::from_rgb(126, 144, 172),
    },
    Theme {
        id: "manga",
        name: "Manga",
        dark: true,
        background: Color32::from_rgb(10, 10, 10),
        surface: Color32::from_rgb(28, 28, 28),
        accent: Color32::from_rgb(238, 66, 66),
        on_accent: Color32::from_rgb(255, 255, 255),
        text: Color32::from_rgb(242, 242, 242),
        muted: Color32::from_rgb(150, 150, 150),
    },
    Theme {
        id: "pop",
        name: "Pop",
        dark: true,
        background: Color32::from_rgb(22, 13, 33),
        surface: Color32::from_rgb(41, 26, 60),
        accent: Color32::from_rgb(255, 98, 180),
        on_accent: Color32::from_rgb(40, 6, 24),
        text: Color32::from_rgb(242, 232, 252),
        muted: Color32::from_rgb(166, 146, 190),
    },
    Theme {
        id: "papier",
        name: "Papier",
        dark: false,
        background: Color32::from_rgb(243, 238, 227),
        surface: Color32::from_rgb(230, 223, 208),
        accent: Color32::from_rgb(214, 74, 44),
        on_accent: Color32::from_rgb(255, 255, 255),
        text: Color32::from_rgb(34, 30, 26),
        muted: Color32::from_rgb(112, 104, 94),
    },
];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Le thème en cours.
pub fn theme() -> &'static Theme {
    &THEMES[CURRENT.load(Ordering::Relaxed).min(THEMES.len() - 1)]
}

/// Passe au thème `id` (le premier s'il n'existe pas).
pub fn set_theme(ctx: &egui::Context, id: &str) {
    let index = THEMES.iter().position(|t| t.id == id).unwrap_or(0);
    CURRENT.store(index, Ordering::Relaxed);
    install(ctx);
}

/// Les polices des livres : PT Serif et PT Sans (ParaType, licence OFL), en
/// romain, italique, gras et gras italique. Chaque famille se replie sur les
/// polices d'egui pour les caractères qu'elle n'a pas.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (name, bytes) in [
        (
            "serif",
            &include_bytes!("../assets/fonts/PT_Serif-Web-Regular.ttf")[..],
        ),
        (
            "serif-italic",
            &include_bytes!("../assets/fonts/PT_Serif-Web-Italic.ttf")[..],
        ),
        (
            "serif-bold",
            &include_bytes!("../assets/fonts/PT_Serif-Web-Bold.ttf")[..],
        ),
        (
            "serif-bold-italic",
            &include_bytes!("../assets/fonts/PT_Serif-Web-BoldItalic.ttf")[..],
        ),
        (
            "sans",
            &include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf")[..],
        ),
        (
            "sans-italic",
            &include_bytes!("../assets/fonts/PT_Sans-Web-Italic.ttf")[..],
        ),
        (
            "sans-bold",
            &include_bytes!("../assets/fonts/PT_Sans-Web-Bold.ttf")[..],
        ),
        (
            "sans-bold-italic",
            &include_bytes!("../assets/fonts/PT_Sans-Web-BoldItalic.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        let mut family = vec![name.to_string()];
        family.extend(fallback.iter().cloned());
        fonts
            .families
            .insert(egui::FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
}

/// Le style de toute l'appli, aux couleurs du thème : arrondi, aéré.
pub fn install(ctx: &egui::Context) {
    let t = theme();
    ctx.all_styles_mut(|style| {
        let mut visuals = if t.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = t.background;
        visuals.window_fill = t.background.lerp_to_gamma(t.surface, 0.6);
        visuals.window_stroke = Stroke::new(1.0, t.text.gamma_multiply(0.08));
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.selection.bg_fill = t.accent.gamma_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0, t.accent);
        visuals.hyperlink_color = t.accent;
        visuals.override_text_color = Some(t.text);
        visuals.extreme_bg_color = t.surface;
        for w in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(8);
            w.bg_stroke = Stroke::NONE;
        }
        visuals.widgets.inactive.weak_bg_fill = t.surface;
        visuals.widgets.hovered.weak_bg_fill = t.raised();
        visuals.widgets.active.weak_bg_fill = t.surface.lerp_to_gamma(t.text, 0.14);
        visuals.widgets.hovered.expansion = 0.0;
        visuals.widgets.active.expansion = 0.0;
        style.visuals = visuals;
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.item_spacing = vec2(8.0, 6.0);
    });
}

#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    Play,
    Pause,
    Fullscreen,
    ExitFullscreen,
    /// Lecture case par case : une planche découpée.
    Panels,
    /// Lecture page entière.
    Page,
    /// Le voile autour de la case : une planche assombrie, la case claire.
    Dim,
    /// Manga : de droite à gauche.
    RightToLeft,
    Minus,
    Plus,
    Folder,
    Search,
    Download,
    /// Le sommaire d'un livre : des lignes.
    Contents,
    /// Les réglages : une roue dentée.
    Gear,
    /// Envoyer (vers la Kindle) : un avion en papier.
    Send,
}

/// Dessine `icon` centrée sur `c`, dans un carré de côté `s`.
pub fn icon(painter: &egui::Painter, icon: Icon, c: Pos2, s: f32, color: Color32) {
    let h = s / 2.0;
    let stroke = Stroke::new((s / 11.0).max(1.4), color);
    let line = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], stroke);
    };
    match icon {
        Icon::Back => {
            let tip = c - vec2(h * 0.8, 0.0);
            line(tip, c + vec2(h * 0.8, 0.0));
            line(tip, tip + vec2(h * 0.5, -h * 0.5));
            line(tip, tip + vec2(h * 0.5, h * 0.5));
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![
                    c + vec2(-h * 0.5, -h * 0.7),
                    c + vec2(h * 0.75, 0.0),
                    c + vec2(-h * 0.5, h * 0.7),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for dx in [-h * 0.38, h * 0.38] {
                let bar = Rect::from_center_size(c + vec2(dx, 0.0), vec2(s * 0.18, s * 0.66));
                painter.rect_filled(bar, 1.5, color);
            }
        }
        Icon::Fullscreen | Icon::ExitFullscreen => {
            let r = h * 0.75;
            let arm = h * 0.4;
            let inward = matches!(icon, Icon::ExitFullscreen);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = c + vec2(sx * r, sy * r);
                let (dx, dy) = if inward {
                    (sx * arm, sy * arm)
                } else {
                    (-sx * arm, -sy * arm)
                };
                let base = if inward {
                    c + vec2(sx * (r - arm), sy * (r - arm))
                } else {
                    corner
                };
                line(base, base + vec2(dx, 0.0));
                line(base, base + vec2(0.0, dy));
            }
        }
        Icon::Panels => {
            let page = Rect::from_center_size(c, vec2(s * 0.7, s * 0.86));
            let gap = s * 0.08;
            let mid = page.top() + page.height() * 0.45;
            let top = Rect::from_min_max(page.min, pos2(page.right(), mid - gap / 2.0));
            let left = Rect::from_min_max(
                pos2(page.left(), mid + gap / 2.0),
                pos2(page.center().x - gap / 2.0, page.bottom()),
            );
            let right =
                Rect::from_min_max(pos2(page.center().x + gap / 2.0, mid + gap / 2.0), page.max);
            painter.rect_filled(top, 1.5, color);
            for r in [left, right] {
                painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Inside);
            }
        }
        Icon::Dim => {
            let page = Rect::from_center_size(c, vec2(s * 0.7, s * 0.86));
            let hole = Rect::from_center_size(c, vec2(s * 0.34, s * 0.3));
            let veil = color.gamma_multiply(0.45);
            for r in [
                Rect::from_min_max(page.min, pos2(page.max.x, hole.min.y)),
                Rect::from_min_max(pos2(page.min.x, hole.max.y), page.max),
                Rect::from_min_max(pos2(page.min.x, hole.min.y), pos2(hole.min.x, hole.max.y)),
                Rect::from_min_max(pos2(hole.max.x, hole.min.y), pos2(page.max.x, hole.max.y)),
            ] {
                painter.rect_filled(r, 0.0, veil);
            }
            painter.rect_stroke(page, 2.0, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(hole, 1.0, stroke, egui::StrokeKind::Middle);
        }
        Icon::Page => {
            let page = Rect::from_center_size(c, vec2(s * 0.7, s * 0.86));
            painter.rect_stroke(page, 2.0, stroke, egui::StrokeKind::Inside);
            painter.rect_filled(page.shrink(s * 0.16), 1.0, color);
        }
        Icon::RightToLeft => {
            for dx in [-h * 0.15, h * 0.45] {
                let tip = c + vec2(dx - h * 0.35, 0.0);
                line(tip, tip + vec2(h * 0.45, -h * 0.55));
                line(tip, tip + vec2(h * 0.45, h * 0.55));
            }
        }
        Icon::Minus => line(c - vec2(h * 0.6, 0.0), c + vec2(h * 0.6, 0.0)),
        Icon::Plus => {
            line(c - vec2(h * 0.6, 0.0), c + vec2(h * 0.6, 0.0));
            line(c - vec2(0.0, h * 0.6), c + vec2(0.0, h * 0.6));
        }
        Icon::Download => {
            let top = c - vec2(0.0, h * 0.75);
            let tip = c + vec2(0.0, h * 0.35);
            line(top, tip);
            line(tip, tip + vec2(-h * 0.45, -h * 0.45));
            line(tip, tip + vec2(h * 0.45, -h * 0.45));
            line(c + vec2(-h * 0.7, h * 0.75), c + vec2(h * 0.7, h * 0.75));
        }
        Icon::Search => {
            let r = h * 0.48;
            let lens = c - vec2(h * 0.15, h * 0.15);
            painter.circle_stroke(lens, r, stroke);
            let d = std::f32::consts::FRAC_1_SQRT_2;
            line(lens + vec2(r * d, r * d), c + vec2(h * 0.75, h * 0.75));
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let dir = vec2(a.cos(), a.sin());
                line(c + dir * h * 0.5, c + dir * h * 0.85);
            }
            painter.circle_stroke(c, h * 0.55, stroke);
            painter.circle_filled(c, h * 0.2, color);
        }
        Icon::Send => {
            let tip = c + vec2(h * 0.8, -h * 0.55);
            let tail = c + vec2(-h * 0.8, -h * 0.05);
            let fold = c + vec2(-h * 0.1, h * 0.15);
            let bottom = c + vec2(h * 0.05, h * 0.8);
            line(tail, tip);
            line(tip, bottom);
            line(bottom, fold);
            line(fold, tail);
            line(fold, tip);
        }
        Icon::Contents => {
            for k in [-1.0, 0.0, 1.0] {
                let y = c.y + k * h * 0.5;
                painter.circle_filled(pos2(c.x - h * 0.65, y), s / 14.0, color);
                line(pos2(c.x - h * 0.35, y), pos2(c.x + h * 0.75, y));
            }
        }
        Icon::Folder => {
            let body = Rect::from_center_size(c + vec2(0.0, h * 0.1), vec2(s * 0.8, s * 0.56));
            let tab = Rect::from_min_size(
                body.left_top() - vec2(0.0, s * 0.12),
                vec2(s * 0.32, s * 0.16),
            );
            painter.rect_filled(tab, 1.5, color);
            painter.rect_filled(body, 2.0, color);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    /// Bouton discret, sur fond sombre ou sur la planche.
    Plain,
    /// Option active (mode manga, lecture auto en cours…).
    Active,
    /// L'action principale.
    Primary,
}

/// Un bouton en pilule : une icône, un texte (ou les deux).
pub fn button(ui: &mut Ui, glyph: Option<Icon>, text: &str, tone: Tone) -> Response {
    let height = 34.0;
    let galley = (!text.is_empty()).then(|| {
        ui.painter().layout_no_wrap(
            text.to_owned(),
            FontId::proportional(14.0),
            Color32::PLACEHOLDER,
        )
    });
    let icon_w = if glyph.is_some() { 18.0 } else { 0.0 };
    let text_w = galley.as_ref().map_or(0.0, |g| g.size().x);
    let gap = if glyph.is_some() && galley.is_some() {
        8.0
    } else {
        0.0
    };
    let pad = if galley.is_some() { 14.0 } else { 8.0 };
    let width = (icon_w + gap + text_w + 2.0 * pad).max(height);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // Fonds sombres et presque opaques : les boutons se posent aussi sur une
    // planche claire, et doivent s'y lire.
    let t = theme();
    let (fill, color, stroke) = match tone {
        Tone::Primary => (
            if hovered {
                t.accent.gamma_multiply(1.08)
            } else {
                t.accent
            },
            t.on_accent,
            Stroke::NONE,
        ),
        Tone::Active => (
            t.surface
                .lerp_to_gamma(t.accent, if hovered { 0.3 } else { 0.2 }),
            t.accent,
            Stroke::new(1.0, t.accent.gamma_multiply(0.7)),
        ),
        Tone::Plain => (
            if pressed {
                t.surface.lerp_to_gamma(t.text, 0.18)
            } else if hovered {
                t.raised()
            } else {
                t.surface.gamma_multiply(0.94)
            },
            t.text,
            Stroke::new(1.0, t.text.gamma_multiply(0.09)),
        ),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, height / 2.0, fill);
    painter.rect_stroke(rect, height / 2.0, stroke, egui::StrokeKind::Inside);
    let content_w = icon_w + gap + text_w;
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(g) = glyph {
        icon(
            painter,
            g,
            pos2(x + icon_w / 2.0, rect.center().y),
            16.0,
            color,
        );
        x += icon_w + gap;
    }
    if let Some(galley) = galley {
        painter.galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
    response
}

/// Fond en dégradé vertical, pour que la barre d'outils se lise sur la planche.
pub fn shade(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Une trame de points, comme l'impression d'une BD : de plus en plus gros
/// vers `corner`, jusqu'à `reach`.
pub fn halftone(
    painter: &egui::Painter,
    rect: Rect,
    corner: Pos2,
    reach: f32,
    step: f32,
    color: Color32,
) {
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() + step {
        let shift = if row % 2 == 0 { 0.0 } else { step / 2.0 };
        let mut x = rect.left() + shift;
        while x < rect.right() + step {
            let p = pos2(x, y);
            let r = (1.0 - corner.distance(p) / reach) * step * 0.42;
            if r > 0.5 {
                painter.circle_filled(p, r, color);
            }
            x += step;
        }
        y += step;
        row += 1;
    }
}

/// Fond en dégradé horizontal, de `left` à `right`.
pub fn shade_across(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Un anneau qui se remplit de 0 à 1, dans le sens des aiguilles d'une montre.
pub fn ring(painter: &egui::Painter, center: Pos2, radius: f32, part: f32, color: Color32) {
    painter.circle_filled(center, radius + 5.0, Color32::from_black_alpha(170));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(2.5, Color32::from_white_alpha(36)),
    );
    let part = part.clamp(0.0, 1.0);
    if part > 0.0 {
        let steps = (48.0 * part).ceil() as usize + 1;
        let points = (0..=steps)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * part * i as f32 / steps as f32;
                center + radius * vec2(a.cos(), a.sin())
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(2.5, color)));
    }
}

/// Une étiquette en pilule (« Nouveau », « Manga »…), posée en `at` (coin haut gauche).
pub fn badge(painter: &egui::Painter, at: Pos2, text: &str, fill: Color32, color: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), color);
    let rect = Rect::from_min_size(at, galley.size() + vec2(14.0, 6.0));
    painter.rect_filled(rect, rect.height() / 2.0, fill);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    rect
}
