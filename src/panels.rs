//! Détection des cases d'une planche de BD.
//!
//! Les cases sont séparées par des gouttières claires : on coupe la page le long
//! des lignes (puis des colonnes) presque vides d'encre, récursivement. Cette
//! découpe donne aussi l'ordre de lecture : de haut en bas, puis de gauche à
//! droite (ou de droite à gauche pour un manga).

/// Rectangle en pixels de la page : [x0, x1) × [y0, y1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Rect {
    pub fn width(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn height(&self) -> usize {
        self.y1 - self.y0
    }
    fn area(&self) -> usize {
        self.width() * self.height()
    }
}

/// Masque d'encre : vrai là où le pixel n'est pas du blanc de gouttière.
struct Ink {
    width: usize,
    ink: Vec<bool>,
}

impl Ink {
    fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Self {
        let mut ink = Vec::with_capacity(width * height);
        for px in rgba.chunks_exact(4).take(width * height) {
            let (r, g, b) = (px[0] as u32, px[1] as u32, px[2] as u32);
            // Un pixel clair et peu coloré compte comme du blanc (papier scanné compris).
            let light = r.min(g).min(b) > 205;
            let grey = r.max(g).max(b) - r.min(g).min(b) < 40;
            ink.push(!(light && grey));
        }
        Self { width, ink }
    }

    fn at(&self, x: usize, y: usize) -> bool {
        self.ink[y * self.width + x]
    }

    /// Pour chaque ligne (`rows`) ou colonne de `r` : son encre, et si elle en
    /// touche aux deux bouts, comme une ligne qui traverse le cadre d'une case.
    fn profile(&self, r: Rect, rows: bool) -> Vec<Line> {
        let (along, across) = if rows {
            (r.y0..r.y1, r.x0..r.x1)
        } else {
            (r.x0..r.x1, r.y0..r.y1)
        };
        let edge = (across.len() / 50).max(2);
        let at = |a: usize, b: usize| if rows { self.at(b, a) } else { self.at(a, b) };
        along
            .map(|a| {
                let ink = across.clone().filter(|&b| at(a, b)).count();
                let head = across.clone().take(edge).any(|b| at(a, b));
                let tail = across.clone().rev().take(edge).any(|b| at(a, b));
                Line {
                    ink,
                    framed: head && tail,
                }
            })
            .collect()
    }

    /// Part de chaque bord de `r` (haut, bas, gauche, droite) couverte d'encre,
    /// en pour cent. Une case a des bords pleins : un cadre, ou un dessin qui
    /// va jusqu'au bord. Un personnage détouré sur du blanc n'en a pas.
    fn edges(&self, r: Rect) -> [usize; 4] {
        let band = (r.width().min(r.height()) / 100).max(3);
        let (top, bottom) = (
            r.y0..(r.y0 + band).min(r.y1),
            r.y1.saturating_sub(band).max(r.y0)..r.y1,
        );
        let (left, right) = (
            r.x0..(r.x0 + band).min(r.x1),
            r.x1.saturating_sub(band).max(r.x0)..r.x1,
        );
        let across = |ys: std::ops::Range<usize>| {
            (r.x0..r.x1)
                .filter(|&x| ys.clone().any(|y| self.at(x, y)))
                .count()
                * 100
                / r.width()
        };
        let down = |xs: std::ops::Range<usize>| {
            (r.y0..r.y1)
                .filter(|&y| xs.clone().any(|x| self.at(x, y)))
                .count()
                * 100
                / r.height()
        };
        [across(top), across(bottom), down(left), down(right)]
    }

    /// Resserre `r` sur son encre ; None s'il n'y en a presque pas.
    fn trim(&self, r: Rect) -> Option<Rect> {
        self.trim_to(r, noise(r.width()), noise(r.height()))
    }

    /// Resserre `r` au pixel près, sans rien rogner : pour du texte, dont
    /// les traits fins compteraient sinon comme des poussières.
    fn trim_exact(&self, r: Rect) -> Option<Rect> {
        self.trim_to(r, 0, 0)
    }

    fn trim_to(&self, r: Rect, row_noise: usize, col_noise: usize) -> Option<Rect> {
        let rows = self.profile(r, true);
        let cols = self.profile(r, false);
        let top = rows.iter().position(|l| l.ink > row_noise)?;
        let bottom = rows.iter().rposition(|l| l.ink > row_noise)?;
        let left = cols.iter().position(|l| l.ink > col_noise)?;
        let right = cols.iter().rposition(|l| l.ink > col_noise)?;
        Some(Rect {
            x0: r.x0 + left,
            y0: r.y0 + top,
            x1: r.x0 + right + 1,
            y1: r.y0 + bottom + 1,
        })
    }
}

struct Line {
    ink: usize,
    framed: bool,
}

fn is_framed(edges: [usize; 4]) -> bool {
    edges.iter().filter(|&&e| e >= 90).count() >= 2
}

/// Encre tolérée dans une gouttière (poussières de scan, traits qui débordent).
fn noise(len: usize) -> usize {
    len / 150
}

/// Les cases de la page, dans l'ordre de lecture.
pub fn detect(width: usize, height: usize, rgba: &[u8], right_to_left: bool) -> Vec<Rect> {
    let ink = Ink::from_rgba(width, height, rgba);
    let page = Rect {
        x0: 0,
        y0: 0,
        x1: width,
        y1: height,
    };
    let Some(content) = ink.trim(page) else {
        return Vec::new();
    };
    // Gouttière minimale : environ 0,4 % de la hauteur de page, 2 px au moins.
    let min_gap = (height / 250).max(2);
    let mut found = Vec::new();
    split(&ink, content, min_gap, right_to_left, 0, &mut found);

    // Numéro de page, petites mentions : trop petits pour être des cases.
    let min_area = content.area() / 60;
    let panels: Vec<Rect> = found.into_iter().filter(|r| r.area() >= min_area).collect();

    // Une planche, ce sont des cases cadrées. Une couverture, une page de titre
    // ou une galerie de personnages détourés sur du blanc n'en a pas : la
    // découper trancherait dans les dessins, on la montre entière.
    // Une bulle ou une onomatopée qui déborde peut ronger deux bords d'une
    // case : on se contente de deux bords pleins, et de la moitié des cases.
    let page = Pixels {
        width,
        height,
        rgba,
    };
    let framed = panels.iter().filter(|r| is_framed(ink.edges(**r))).count();
    if panels.len() <= 1 || framed * 2 < panels.len() {
        // Une page de texte (crédits, préface) entière serait illisible : on
        // la lit par morceaux.
        return text(&page, &ink, right_to_left).unwrap_or(vec![content]);
    }
    panels
        .iter()
        .map(|&p| grow(&page, &ink, p, &panels))
        .collect()
}

/// Une page de texte noir sur blanc, en morceaux lisibles : colonne par
/// colonne, des paquets de lignes assez courts pour qu'une fois le morceau à
/// la taille de l'écran, le texte se lise. None si ce n'est pas du texte, ou
/// si la page tient en un morceau.
fn text(page: &Pixels, ink: &Ink, rtl: bool) -> Option<Vec<Rect>> {
    let content = ink.trim_exact(Rect {
        x0: 0,
        y0: 0,
        x1: page.width,
        y1: page.height,
    })?;
    // Du texte : peu d'encre, et presque pas de couleur (un dessin en a).
    let (mut seen, mut inked, mut colored) = (0usize, 0usize, 0usize);
    for y in (content.y0..content.y1).step_by(3) {
        for x in (content.x0..content.x1).step_by(3) {
            seen += 1;
            if ink.at(x, y) {
                inked += 1;
                let c = page.at(x, y);
                colored += (c.iter().max().unwrap() - c.iter().min().unwrap() > 60) as usize;
            }
        }
    }
    if inked * 100 > seen * 15 || colored * 100 > inked * 25 {
        return None;
    }
    // Les colonnes sont séparées par plus qu'un espace entre deux mots.
    let mut columns = cut(ink, content, false, content.width() / 40, Gutter::Exact);
    if rtl {
        columns.reverse();
    }
    let lines: Vec<Vec<Rect>> = columns
        .into_iter()
        .filter_map(|c| ink.trim_exact(c))
        .map(|c| {
            cut(ink, c, true, 2, Gutter::Exact)
                .into_iter()
                .filter_map(|l| ink.trim_exact(l))
                .collect()
        })
        .collect();
    let mut heights: Vec<usize> = lines.iter().flatten().map(Rect::height).collect();
    if heights.len() < 4 {
        return None;
    }
    heights.sort_unstable();
    let line = heights[heights.len() / 2];
    // Des lignes qui se lisent sur la page entière (une page de titre) : rien
    // à découper. Sinon, une trentaine de lignes au plus par morceau
    // (interligne compris).
    if line * 70 > page.height {
        return None;
    }
    let tallest = line * 40;
    // Une poussière, plus petite qu'une lettre, n'est pas une ligne.
    let lines = lines.into_iter().map(|c| {
        c.into_iter()
            .filter(|l| l.area() >= line * line)
            .collect::<Vec<_>>()
    });
    let mut chunks = Vec::new();
    for column in lines {
        let mut lines = column.into_iter();
        let Some(mut chunk) = lines.next() else {
            continue;
        };
        for line in lines {
            if union(chunk, line).height() <= tallest {
                chunk = union(chunk, line);
            } else {
                chunks.push(chunk);
                chunk = line;
            }
        }
        chunks.push(chunk);
    }
    (chunks.len() > 1).then_some(chunks)
}

struct Pixels<'a> {
    width: usize,
    height: usize,
    rgba: &'a [u8],
}

impl Pixels<'_> {
    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.width + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

/// Un cartouche ou une bulle à cheval sur une gouttière est coupé par la
/// découpe : chaque case qui en porte au moins le tiers le reçoit en entier,
/// même s'il mord sur la voisine. Celle qu'il ne fait qu'effleurer, non.
fn grow(page: &Pixels, ink: &Ink, p: Rect, panels: &[Rect]) -> Rect {
    let outside = |x: usize, y: usize| !panels.iter().any(|r| inside(r, x, y));
    // Jusqu'où un débordement peut aller : au-delà, c'est le remplissage qui a fui.
    let bound = Rect {
        x0: p.x0.saturating_sub(page.width / 5),
        y0: p.y0.saturating_sub(page.height / 6),
        x1: (p.x1 + page.width / 5).min(page.width),
        y1: (p.y1 + page.height / 6).min(page.height),
    };
    // Les quatre lignes de pixels qui longent la case, juste à l'extérieur.
    let mut sides: Vec<Vec<(usize, usize)>> = Vec::new();
    if p.y0 >= 2 {
        sides.push((p.x0..p.x1).map(|x| (x, p.y0 - 2)).collect());
    }
    if p.y1 + 1 < page.height {
        sides.push((p.x0..p.x1).map(|x| (x, p.y1 + 1)).collect());
    }
    if p.x0 >= 2 {
        sides.push((p.y0..p.y1).map(|y| (p.x0 - 2, y)).collect());
    }
    if p.x1 + 1 < page.width {
        sides.push((p.y0..p.y1).map(|y| (p.x1 + 1, y)).collect());
    }
    let mut grown = p;
    let mut seen = vec![false; bound.area()];
    for line in sides {
        // Ce qui déborde dans la gouttière : des tronçons d'encre hors de toute
        // case. Les blancs courts (lettres, intérieur d'une bulle) les relient.
        let link = page.height / 100;
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, &(x, y)) in line.iter().enumerate() {
            if ink.at(x, y) && outside(x, y) {
                match runs.last_mut() {
                    Some(run) if i - run.1 <= link => run.1 = i,
                    _ => runs.push((i, i)),
                }
            }
        }
        for (a, b) in runs {
            if b - a < page.height / 100 {
                continue;
            }
            for &(x, y) in line[a..=b].iter().step_by(3) {
                if let Some(blob) = fill(page, bound, &mut seen, x, y)
                    && blob.width().min(blob.height()) >= page.height / 100
                    && overlap(&blob, &p) * 3 >= blob.area()
                {
                    grown = union(grown, blob);
                }
            }
        }
    }
    grown
}

fn inside(r: &Rect, x: usize, y: usize) -> bool {
    (r.x0..r.x1).contains(&x) && (r.y0..r.y1).contains(&y)
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// Surface commune de deux rectangles.
fn overlap(a: &Rect, b: &Rect) -> usize {
    let w = a.x1.min(b.x1).saturating_sub(a.x0.max(b.x0));
    let h = a.y1.min(b.y1).saturating_sub(a.y0.max(b.y0));
    w * h
}

/// Remplit, depuis (x, y), la zone de même couleur (le fond d'un cartouche ou
/// d'une bulle, que son contour arrête) et rend son rectangle ; None si le
/// pixel est sombre (contour, lettre), déjà rempli, ou si la zone est trop
/// grande pour une bulle ou fuit hors de `bound` (la gouttière elle-même, un
/// décor).
fn fill(page: &Pixels, bound: Rect, seen: &mut [bool], x: usize, y: usize) -> Option<Rect> {
    let index = |x: usize, y: usize| (y - bound.y0) * bound.width() + (x - bound.x0);
    if !inside(&bound, x, y) || seen[index(x, y)] {
        return None;
    }
    let seed = page.at(x, y);
    if seed.iter().map(|&c| c as u32).sum::<u32>() < 3 * 90 {
        return None;
    }
    let like = |c: [u8; 3]| (0..3).all(|k| c[k].abs_diff(seed[k]) <= 36);
    let mut blob = Rect {
        x0: x,
        y0: y,
        x1: x + 1,
        y1: y + 1,
    };
    let mut leaked = false;
    let mut stack = vec![(x, y)];
    seen[index(x, y)] = true;
    while let Some((x, y)) = stack.pop() {
        blob = union(
            blob,
            Rect {
                x0: x,
                y0: y,
                x1: x + 1,
                y1: y + 1,
            },
        );
        // Le bord de la page arrête le remplissage ; celui de `bound`, non.
        let touches = (x == bound.x0 && x > 0)
            || (y == bound.y0 && y > 0)
            || (x + 1 == bound.x1 && x + 1 < page.width)
            || (y + 1 == bound.y1 && y + 1 < page.height);
        leaked |= touches;
        let next = [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ];
        for (nx, ny) in next {
            if inside(&bound, nx, ny) && !seen[index(nx, ny)] && like(page.at(nx, ny)) {
                seen[index(nx, ny)] = true;
                stack.push((nx, ny));
            }
        }
    }
    let small = blob.width() <= page.width / 3 && blob.height() <= page.height / 4;
    (!leaked && small).then_some(blob)
}

fn split(ink: &Ink, r: Rect, min_gap: usize, rtl: bool, depth: usize, out: &mut Vec<Rect>) {
    // Gouttières nettes d'abord ; sinon on accepte celles que mordent une
    // bulle ou une onomatopée (« ZAM ! ») qui déborde de sa case.
    // À chaque fois : en bandes horizontales, sinon en colonnes.
    for loose in [false, true] {
        for rows in [true, false] {
            let gutter = if loose { Gutter::Loose } else { Gutter::Clean };
            let parts = cut(ink, r, rows, min_gap, gutter);
            if parts.len() > 1 && depth < 12 {
                let ordered: Vec<Rect> = if !rows && rtl {
                    parts.into_iter().rev().collect()
                } else {
                    parts
                };
                for part in ordered {
                    if let Some(part) = ink.trim(part) {
                        split(ink, part, min_gap, rtl, depth + 1, out);
                    }
                }
                return;
            }
        }
    }
    out.push(r);
}

/// Ce qu'une gouttière peut contenir d'encre.
#[derive(Clone, Copy, PartialEq)]
enum Gutter {
    /// Des poussières, des traits qui débordent.
    Clean,
    /// Une bulle ou une onomatopée qui mord dessus.
    Loose,
    /// Rien du tout : entre deux lignes ou deux colonnes de texte.
    Exact,
}

/// Coupe `r` le long des gouttières (lignes si `rows`, colonnes sinon).
fn cut(ink: &Ink, r: Rect, rows: bool, min_gap: usize, gutter: Gutter) -> Vec<Rect> {
    let profile = ink.profile(r, rows);
    let across = if rows { r.width() } else { r.height() };
    let along = if rows { r.height() } else { r.width() };
    let tolerance = match gutter {
        Gutter::Clean => noise(across),
        Gutter::Loose => across * 3 / 10,
        Gutter::Exact => 0,
    };
    let loose = gutter == Gutter::Loose;
    let mut parts = Vec::new();
    let mut start = 0;
    let mut gap_start: Option<usize> = None;
    for (i, line) in profile.iter().enumerate() {
        // Un blanc bordé d'encre des deux côtés est l'intérieur d'une case
        // (sous un cartouche, par exemple), pas une gouttière.
        if line.ink <= tolerance && !line.framed {
            gap_start.get_or_insert(i);
        } else {
            if let Some(g) = gap_start.take()
                && i - g >= min_gap
                && g > start
            {
                parts.push((start, g));
                start = i;
            }
        }
    }
    let end = gap_start.unwrap_or(profile.len());
    if end > start {
        parts.push((start, end));
    }
    // En mode tolérant, un blanc dans un décor peut ressembler à une gouttière :
    // on refuse les découpes qui laissent des morceaux trop fins.
    if loose && parts.iter().any(|&(a, b)| b - a < along / 8) {
        return vec![r];
    }
    parts
        .into_iter()
        .map(|(a, b)| {
            if rows {
                Rect {
                    x0: r.x0,
                    y0: r.y0 + a,
                    x1: r.x1,
                    y1: r.y0 + b,
                }
            } else {
                Rect {
                    x0: r.x0 + a,
                    y0: r.y0,
                    x1: r.x0 + b,
                    y1: r.y1,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une page blanche de `w`×`h` où chaque rectangle est une case cadrée de
    /// noir, remplie de gris.
    fn page(w: usize, h: usize, boxes: &[Rect]) -> Vec<u8> {
        let mut rgba = vec![255u8; w * h * 4];
        for b in boxes {
            for y in b.y0..b.y1 {
                for x in b.x0..b.x1 {
                    let edge = x < b.x0 + 3 || x + 3 >= b.x1 || y < b.y0 + 3 || y + 3 >= b.y1;
                    let v = if edge { 0 } else { 120 };
                    rgba[(y * w + x) * 4..][..3].copy_from_slice(&[v, v, v]);
                }
            }
        }
        rgba
    }

    fn r(x0: usize, y0: usize, x1: usize, y1: usize) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    #[test]
    fn finds_panels_in_reading_order() {
        // Une grande case en haut, deux en bas.
        let boxes = [
            r(40, 40, 760, 500),
            r(40, 540, 380, 1060),
            r(420, 540, 760, 1060),
        ];
        let found = detect(800, 1100, &page(800, 1100, &boxes), false);
        assert_eq!(found, boxes);
    }

    #[test]
    fn manga_reads_right_to_left() {
        let boxes = [r(40, 40, 380, 500), r(420, 40, 760, 500)];
        let found = detect(800, 540, &page(800, 540, &boxes), true);
        assert_eq!(found, vec![boxes[1], boxes[0]]);
    }

    #[test]
    fn whole_page_without_frames() {
        // Deux personnages détourés (sans cadre) : la page est montrée entière.
        let (w, h) = (800, 1100);
        let mut rgba = vec![255u8; w * h * 4];
        for (cx, cy) in [(220usize, 550usize), (580, 550)] {
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 - cx as f32, (y as f32 - cy as f32) / 3.0);
                    if dx * dx + dy * dy < 120.0 * 120.0 {
                        rgba[(y * w + x) * 4..][..3].copy_from_slice(&[90, 60, 140]);
                    }
                }
            }
        }
        let found = detect(w, h, &rgba, false);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn caption_across_gutter_stays_whole() {
        // Un cartouche cerné de noir, à cheval sur la gouttière : la case de
        // gauche, qui en porte presque la moitié, le reçoit en entier ; celle de
        // droite, qu'il ne fait que mordre, ne change pas.
        let (w, h) = (800, 600);
        let boxes = [r(40, 40, 380, 560), r(420, 40, 760, 560)];
        let mut rgba = page(w, h, &boxes);
        for y in 100..200 {
            for x in 300..470 {
                let edge = !(303..467).contains(&x) || !(103..197).contains(&y);
                let c = if edge { [0, 0, 0] } else { [235, 220, 180] };
                rgba[(y * w + x) * 4..][..3].copy_from_slice(&c);
            }
        }
        let found = detect(w, h, &rgba, false);
        assert_eq!(found.len(), 2);
        assert!(found[0].x1 >= 467, "{:?}", found[0]);
        assert_eq!(found[1], boxes[1]);
    }

    #[test]
    fn text_page_is_read_in_pieces() {
        // Deux colonnes de petites lignes de texte : illisible en entier, la
        // page se lit par paquets de lignes, colonne par colonne.
        let (w, h) = (800, 1100);
        let mut rgba = vec![255u8; w * h * 4];
        for column in [60usize, 440] {
            for top in (60..1040).step_by(16) {
                for y in top..top + 8 {
                    for x in (column..column + 300).filter(|x| x % 12 < 3) {
                        rgba[(y * w + x) * 4..][..3].copy_from_slice(&[0, 0, 0]);
                    }
                }
            }
        }
        let found = detect(w, h, &rgba, false);
        assert!(found.len() > 2, "{found:?}");
        for r in &found {
            assert!(r.height() <= 8 * 40, "{r:?}");
            assert!(r.x1 <= 360 || r.x0 >= 440, "{r:?}");
        }
        assert!(found[0].x0 < 360, "la colonne de gauche d'abord");
    }

    #[test]
    fn blank_page_has_no_panel() {
        assert!(detect(100, 100, &vec![255u8; 100 * 100 * 4], false).is_empty());
    }
}
