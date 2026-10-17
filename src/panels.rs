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
        let rows = self.profile(r, true);
        let cols = self.profile(r, false);
        let row_noise = noise(r.width());
        let col_noise = noise(r.height());
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
    let framed = panels.iter().filter(|r| is_framed(ink.edges(**r))).count();
    if panels.len() <= 1 || framed * 2 < panels.len() {
        return vec![content];
    }
    panels
}

fn split(ink: &Ink, r: Rect, min_gap: usize, rtl: bool, depth: usize, out: &mut Vec<Rect>) {
    // Gouttières nettes d'abord ; sinon on accepte celles que mordent une
    // bulle ou une onomatopée (« ZAM ! ») qui déborde de sa case.
    // À chaque fois : en bandes horizontales, sinon en colonnes.
    for loose in [false, true] {
        for rows in [true, false] {
            let parts = cut(ink, r, rows, min_gap, loose);
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

/// Coupe `r` le long des gouttières (lignes si `rows`, colonnes sinon).
fn cut(ink: &Ink, r: Rect, rows: bool, min_gap: usize, loose: bool) -> Vec<Rect> {
    let profile = ink.profile(r, rows);
    let across = if rows { r.width() } else { r.height() };
    let along = if rows { r.height() } else { r.width() };
    let (tolerance, min_gap) = if loose {
        (across * 3 / 10, min_gap)
    } else {
        (noise(across), min_gap)
    };
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
