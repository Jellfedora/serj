//! L'écran de démarrage, `t` secondes après le lancement : une planche se
//! dessine (celle de l'icône), sa dernière case s'allume et la caméra plonge
//! dedans ; « SERJ » y éclate comme une onomatopée, sur des traits de vitesse,
//! une bulle dit la devise ; puis l'image se ferme en iris, comme à la fin d'un
//! vieux dessin animé, et l'appli apparaît. Un clic ou une touche l'abrège.

use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::widgets::{ACCENT, BACKGROUND};

/// La planche se dessine, puis sa dernière case s'allume.
const BUILD: f32 = 0.15;
const LIGHT: f32 = 0.75;
/// La caméra plonge dans la case allumée.
const DIVE: f32 = 1.05;
const DIVE_SECS: f32 = 0.5;
/// Les lettres de « SERJ » éclatent l'une après l'autre.
const LETTERS: f32 = 1.55;
const LETTER_GAP: f32 = 0.1;
const POP: f32 = 0.32;
/// La bulle, puis la devise tapée dedans.
const BUBBLE: f32 = 2.2;
/// L'iris se ferme, puis l'appli apparaît en fondu.
const IRIS: f32 = 3.6;
const IRIS_SECS: f32 = 0.45;
const FADE_SECS: f32 = 0.3;
pub const END: f32 = IRIS + IRIS_SECS + FADE_SECS;

const INK: Color32 = Color32::from_rgb(20, 18, 22);
const PAPER: Color32 = Color32::from_rgb(250, 248, 242);
const PANEL: Color32 = Color32::from_rgb(84, 84, 96);
const SHEET: Color32 = Color32::from_rgb(56, 56, 66);
const MOTTO: &str = "Une case à la fois.";

/// L'appli est encore cachée : on ne dessine qu'elle.
pub fn covers_app(t: f32) -> bool {
    t < IRIS + IRIS_SECS
}

/// Dessine l'écran à `t` secondes ; faux une fois fini.
pub fn paint(painter: &egui::Painter, screen: Rect, t: f32) -> bool {
    if t >= END {
        return false;
    }
    let iris_end = IRIS + IRIS_SECS;
    if t >= iris_end {
        // L'appli apparaît sous le noir qui s'efface.
        let fade = 1.0 - (t - iris_end) / FADE_SECS;
        painter.rect_filled(screen, 0.0, BACKGROUND.gamma_multiply(fade.clamp(0.0, 1.0)));
        return true;
    }
    painter.rect_filled(screen, 0.0, BACKGROUND);

    // La planche de l'icône : une case en haut, deux en bas.
    let h = (screen.height() * 0.46).min(420.0);
    let page = Rect::from_center_size(screen.center(), vec2(h * 0.74, h));
    let m = h * 0.07;
    let inner = page.shrink(m);
    let mid = inner.top() + inner.height() * 0.44;
    let split = inner.left() + inner.width() * 0.4;
    let gap = m * 0.75;
    let panels = [
        Rect::from_min_max(inner.min, pos2(inner.right(), mid - gap / 2.0)),
        Rect::from_min_max(
            pos2(inner.left(), mid + gap / 2.0),
            pos2(split - gap / 2.0, inner.bottom()),
        ),
        Rect::from_min_max(pos2(split + gap / 2.0, mid + gap / 2.0), inner.max),
    ];
    let lit = panels[2];

    // La plongée : la case allumée grandit jusqu'à remplir l'écran.
    let dive = ease_in_out(((t - DIVE) / DIVE_SECS).clamp(0.0, 1.0));
    let full = (screen.width() / lit.width()).max(screen.height() / lit.height()) * 1.08;
    let scale = full.powf(dive);
    let pivot = screen.center().lerp(lit.center(), dive);
    let view = |p: Pos2| screen.center() + (p - pivot) * scale;
    let view_rect = |r: Rect| Rect::from_min_max(view(r.min), view(r.max));

    if dive < 1.0 {
        // La planche, puis ses cases l'une après l'autre.
        let grow = ease_out_back(((t - BUILD) / 0.35).clamp(0.0, 1.0));
        if grow > 0.0 {
            let sheet = Rect::from_center_size(view(page.center()), page.size() * scale * grow);
            painter.rect_filled(sheet, 14.0 * scale * grow, SHEET);
        }
        for (i, panel) in panels.iter().enumerate() {
            let p = ease_out_back(((t - BUILD - 0.2 - i as f32 * 0.1) / 0.3).clamp(0.0, 1.0));
            if p <= 0.0 {
                continue;
            }
            let r = view_rect(*panel);
            let r = Rect::from_center_size(r.center(), r.size() * p);
            painter.rect_filled(r, 6.0 * scale, PANEL);
        }
        // La dernière case s'allume, avec un halo, et une bulle y apparaît.
        let light = ((t - LIGHT) / 0.25).clamp(0.0, 1.0);
        if light > 0.0 {
            let r = view_rect(lit);
            for k in 1..=8 {
                let glow = r.expand(k as f32 * 6.0 * scale.min(3.0));
                painter.rect_filled(glow, 10.0 * scale, ACCENT.gamma_multiply(0.035 * light));
            }
            painter.rect_filled(r, 6.0 * scale, PANEL.lerp_to_gamma(ACCENT, light));
            let pop = ease_out_back(((t - LIGHT - 0.1) / 0.3).clamp(0.0, 1.0));
            if pop > 0.0 {
                let c = r.center() - vec2(0.0, r.height() * 0.08);
                bubble(
                    painter,
                    c,
                    vec2(r.width() * 0.32, r.width() * 0.17) * pop,
                    2.5 * scale.min(4.0),
                );
                for k in [-1.0, 0.0, 1.0] {
                    let dot = c + vec2(k * r.width() * 0.13, 0.0) * pop;
                    painter.circle_filled(dot, r.width() * 0.045 * pop, INK);
                }
            }
        }
    } else {
        painter.rect_filled(screen, 0.0, ACCENT);
    }

    // Dans la case : le décor d'une onomatopée, des traits de vitesse et une
    // trame de points.
    let inside = ((t - DIVE - DIVE_SECS * 0.6) / 0.3).clamp(0.0, 1.0);
    let center = screen.center() - vec2(0.0, screen.height() * 0.06);
    if inside > 0.0 {
        halftone(painter, screen, inside);
        speed_lines(painter, screen, center, t, inside);
    }

    // « SERJ », lettre par lettre : chacune éclate, penchée, et l'image tremble.
    let size = (screen.width() / 5.5).clamp(90.0, 230.0);
    let font = FontId::monospace(size);
    let letters = ['S', 'E', 'R', 'J'];
    let tilt = [-0.14, 0.07, -0.05, 0.11];
    let lift = [0.0, -0.07, 0.04, -0.04];
    let advance = size * 0.66;
    let landing = |i: usize| LETTERS + i as f32 * LETTER_GAP + POP * 0.5;
    let mut shake = 0.0;
    for i in 0..letters.len() {
        let d = t - landing(i);
        if (0.0..0.18).contains(&d) {
            shake += (1.0 - d / 0.18).powi(2) * size * if i == 3 { 0.05 } else { 0.025 };
        }
    }
    let jolt = vec2((t * 97.0).sin(), (t * 71.0).cos()) * shake;
    let word_left = center.x - advance * (letters.len() as f32 - 1.0) / 2.0;
    for (i, c) in letters.iter().enumerate() {
        let p = ((t - LETTERS - i as f32 * LETTER_GAP) / POP).clamp(0.0, 1.0);
        if p <= 0.0 {
            continue;
        }
        let s = ease_out_back(p);
        let at = pos2(word_left + i as f32 * advance, center.y + lift[i] * size) + jolt;
        sfx(painter, at, *c, size * s, tilt[i] * (2.0 - s), font.clone());
    }

    // L'éclat de la dernière lettre : un cercle qui s'élargit.
    let d = t - landing(3);
    if (0.0..0.5).contains(&d) {
        let ring = size * (0.8 + d * 6.0);
        painter.circle_stroke(
            center,
            ring,
            Stroke::new(5.0 * (1.0 - d / 0.5), PAPER.gamma_multiply(1.0 - d / 0.5)),
        );
    }

    // La bulle, et la devise tapée dedans.
    let pop = ease_out_back(((t - BUBBLE) / 0.3).clamp(0.0, 1.0));
    if pop > 0.0 {
        let text_font = FontId::proportional((size * 0.2).clamp(20.0, 34.0));
        let full = painter.layout_no_wrap(MOTTO.to_owned(), text_font.clone(), INK);
        let c = center + vec2(size * 0.35, size * 0.95);
        let radius = vec2(full.size().x * 0.68 + 30.0, full.size().y * 1.25 + 14.0) * pop;
        // La queue de la bulle, vers les lettres.
        let tail = [
            c + vec2(-radius.x * 0.45, -radius.y * 0.55),
            c + vec2(-radius.x * 0.75, -radius.y * 1.45),
            c + vec2(-radius.x * 0.18, -radius.y * 0.7),
        ];
        painter.add(Shape::convex_polygon(
            tail.to_vec(),
            PAPER,
            Stroke::new(3.0, INK),
        ));
        bubble(painter, c, radius, 3.0);
        painter.add(Shape::convex_polygon(
            vec![
                tail[0] + vec2(2.0, 4.0),
                tail[1] + vec2(4.0, 6.0),
                tail[2] + vec2(-2.0, 4.0),
            ],
            PAPER,
            Stroke::NONE,
        ));
        let typed: String = MOTTO
            .chars()
            .take(((t - BUBBLE - 0.2) / 0.035).max(0.0) as usize)
            .collect();
        if !typed.is_empty() {
            let galley = painter.layout_no_wrap(typed, text_font, INK);
            painter.galley(c - full.size() / 2.0, galley, INK);
        }
    }

    // En bas : la version, et qui l'a faite.
    let credit = ((t - BUBBLE - 0.5) / 0.4).clamp(0.0, 1.0);
    if credit > 0.0 {
        let color = INK.gamma_multiply(credit * 0.9);
        let y = screen.bottom() - 36.0;
        let mut format = egui::TextFormat::simple(FontId::proportional(12.0), color);
        format.extra_letter_spacing = 2.5;
        let spaced = |s: &str| {
            painter.layout_job(egui::text::LayoutJob::single_section(
                s.to_owned(),
                format.clone(),
            ))
        };
        let before = spaced("FAIT AVEC");
        let after = spaced("PAR JELLFEDORA");
        let heart = 16.0;
        let total = before.size().x + after.size().x + heart + 20.0;
        let mut x = screen.center().x - total / 2.0;
        painter.galley(pos2(x, y - before.size().y / 2.0), before.clone(), color);
        x += before.size().x + 10.0;
        let beat = 1.0 + 0.18 * (t * 7.0).sin().max(0.0).powi(2);
        heart_shape(
            painter,
            pos2(x + heart / 2.0, y),
            6.5 * beat,
            Color32::from_rgb(200, 40, 50).gamma_multiply(credit),
        );
        x += heart + 10.0;
        painter.galley(pos2(x, y - after.size().y / 2.0), after, color);
        painter.text(
            pos2(screen.center().x, y - 26.0),
            Align2::CENTER_CENTER,
            format!("v{}", env!("CARGO_PKG_VERSION")),
            FontId::monospace(12.0),
            color,
        );
    }

    // L'iris se ferme sur les lettres.
    let iris = ((t - IRIS) / IRIS_SECS).clamp(0.0, 1.0);
    if iris > 0.0 {
        let far = (screen.width().hypot(screen.height())) * 0.6;
        let r = far * (1.0 - ease_in(iris));
        ring(painter, center, r, far * 1.2, BACKGROUND);
        if r > 1.0 {
            painter.circle_stroke(center, r, Stroke::new(4.0, INK));
        }
    }
    true
}

/// Une lettre d'onomatopée : blanche, cerclée de noir, une ombre décalée.
fn sfx(painter: &egui::Painter, at: Pos2, c: char, size: f32, angle: f32, font: FontId) {
    if size < 1.0 {
        return;
    }
    let font = FontId::new(size, font.family);
    let galley = painter.layout_no_wrap(c.to_string(), font, Color32::WHITE);
    let corner = at - galley.size() / 2.0;
    let place = |offset: Vec2, color: Color32| {
        painter.add(
            egui::epaint::TextShape::new(corner + offset, galley.clone(), color)
                .with_override_text_color(color)
                .with_angle_and_anchor(angle, Align2::CENTER_CENTER),
        );
    };
    // L'ombre, le contour épais, puis la lettre (épaissie : la police est fine).
    let shadow = vec2(1.0, 1.0) * size * 0.075;
    let outline = size * 0.055;
    let bold = size * 0.022;
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        place(shadow + Vec2::angled(a) * outline, INK);
    }
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        place(Vec2::angled(a) * outline, INK);
    }
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        place(Vec2::angled(a) * bold, PAPER);
    }
    place(Vec2::ZERO, PAPER);
}

/// Une bulle : une ellipse blanche cerclée d'encre.
fn bubble(painter: &egui::Painter, c: Pos2, radius: Vec2, stroke: f32) {
    if radius.x < 1.0 || radius.y < 1.0 {
        return;
    }
    let points: Vec<Pos2> = (0..64)
        .map(|k| {
            let a = k as f32 / 64.0 * std::f32::consts::TAU;
            c + vec2(a.cos() * radius.x, a.sin() * radius.y)
        })
        .collect();
    painter.add(Shape::convex_polygon(
        points,
        PAPER,
        Stroke::new(stroke, INK),
    ));
}

/// Des traits fins qui rayonnent du centre, en tournant doucement.
fn speed_lines(painter: &egui::Painter, screen: Rect, center: Pos2, t: f32, alpha: f32) {
    let far = screen.width().hypot(screen.height());
    for k in 0..44u32 {
        let a = k as f32 / 44.0 * std::f32::consts::TAU + noise(k) * 0.08 + t * 0.05;
        let start = far * (0.22 + 0.12 * noise(k + 100));
        let dir = Vec2::angled(a);
        let width = 2.0 + 7.0 * noise(k + 200);
        let side = dir.rot90() * width;
        let tip = center + dir * start;
        let end = center + dir * far;
        painter.add(Shape::convex_polygon(
            vec![tip, end + side, end - side],
            INK.gamma_multiply(0.16 * alpha),
            Stroke::NONE,
        ));
    }
}

/// Une trame de points, de plus en plus gros vers les coins.
fn halftone(painter: &egui::Painter, screen: Rect, alpha: f32) {
    let step = 18.0;
    let color = Color32::from_rgb(214, 160, 50).gamma_multiply(alpha);
    let corners = [screen.left_top(), screen.right_bottom()];
    let reach = screen.width().min(screen.height()) * 0.55;
    let mut y = screen.top();
    let mut row = 0;
    while y < screen.bottom() + step {
        let shift = if row % 2 == 0 { 0.0 } else { step / 2.0 };
        let mut x = screen.left() + shift;
        while x < screen.right() + step {
            let p = pos2(x, y);
            let near = corners
                .iter()
                .map(|c| c.distance(p))
                .fold(f32::MAX, f32::min);
            let r = (1.0 - near / reach) * step * 0.42;
            if r > 0.6 {
                painter.circle_filled(p, r, color);
            }
            x += step;
        }
        y += step;
        row += 1;
    }
}

/// Un anneau plein entre les rayons `inner` et `outer`.
fn ring(painter: &egui::Painter, c: Pos2, inner: f32, outer: f32, color: Color32) {
    let n = 96;
    let mut mesh = egui::Mesh::default();
    for k in 0..n {
        let dir = Vec2::angled(k as f32 / n as f32 * std::f32::consts::TAU);
        mesh.colored_vertex(c + dir * inner.max(0.0), color);
        mesh.colored_vertex(c + dir * outer, color);
    }
    for k in 0..n {
        let (a, b) = (2 * k, 2 * ((k + 1) % n));
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    painter.add(mesh);
}

/// ❤, dessiné (les polices ne l'ont pas toujours) : deux lobes sur une pointe.
fn heart_shape(painter: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    let lobe = r * 0.55;
    painter.circle_filled(c + vec2(-lobe * 0.9, -r * 0.25), lobe, color);
    painter.circle_filled(c + vec2(lobe * 0.9, -r * 0.25), lobe, color);
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(-r * 0.98, -r * 0.05),
            c + vec2(r * 0.98, -r * 0.05),
            c + vec2(0.0, r * 0.95),
        ],
        color,
        Stroke::NONE,
    ));
}

fn ease_in(x: f32) -> f32 {
    x * x * x
}

fn ease_in_out(x: f32) -> f32 {
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

/// Dépasse un peu avant de se poser, comme un ressort.
fn ease_out_back(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// Un nombre pseudo-aléatoire stable dans [0, 1) pour `n`.
fn noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}
