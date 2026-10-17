//! Le thème et les petits éléments de l'interface. Les icônes sont dessinées à
//! la main : la police par défaut n'a ni flèches, ni lecture, ni plein écran.

use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2,
};

pub const BACKGROUND: Color32 = Color32::from_rgb(16, 16, 19);
/// Le reste de la planche, autour de la case lue : à peine visible.
pub const DIM: Color32 = Color32::from_rgba_premultiplied(16, 16, 19, 232);
pub const SURFACE: Color32 = Color32::from_rgb(30, 30, 35);
pub const ACCENT: Color32 = Color32::from_rgb(240, 196, 92);
/// Le texte posé sur l'accent.
pub const ON_ACCENT: Color32 = Color32::from_rgb(28, 22, 8);
pub const TEXT: Color32 = Color32::from_rgb(232, 232, 236);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 152);

/// Le style de toute l'appli : sombre, arrondi, aéré.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = Color32::from_rgb(26, 26, 31);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(18));
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        visuals.hyperlink_color = ACCENT;
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
        visuals.widgets.inactive.weak_bg_fill = SURFACE;
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 44, 51);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(54, 54, 62);
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
    /// Manga : de droite à gauche.
    RightToLeft,
    Minus,
    Plus,
    Folder,
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
    let (fill, color) = match tone {
        Tone::Primary => (
            if hovered {
                ACCENT.gamma_multiply(1.08)
            } else {
                ACCENT
            },
            ON_ACCENT,
        ),
        Tone::Active => (
            Color32::from_rgba_unmultiplied(240, 196, 92, if hovered { 64 } else { 44 }),
            ACCENT,
        ),
        Tone::Plain => (
            if pressed {
                Color32::from_white_alpha(40)
            } else if hovered {
                Color32::from_white_alpha(28)
            } else {
                Color32::from_white_alpha(14)
            },
            TEXT,
        ),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, height / 2.0, fill);
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
