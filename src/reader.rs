//! La lecture d'un livre en texte (EPUB, MOBI, FB2, TXT) : Serj le met en page
//! à la taille de la fenêtre, sur une ou deux colonnes, et on tourne les pages.
//! Une page qui tourne glisse un peu et se fond dans la suivante.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontFamily, FontId, Galley, Key, Painter, Pos2, Rect, Sense, TextureHandle,
    Vec2, pos2, vec2,
};

use serj::ebook::{Align, Block, Ebook, Span};

use crate::state::{Paper, Position, Typography};
use crate::widgets::{self, Icon, Tone, theme};

/// Durée d'une page qui tourne, et de combien elle glisse.
const TURN_SECS: f64 = 0.34;
const TURN_SHIFT: f32 = 26.0;
/// Combien de temps les commandes restent visibles.
const HUD_SECS: f64 = 2.2;
/// Tailles de texte proposées.
const SIZES: [f32; 11] = [
    14.0, 15.0, 16.0, 17.0, 19.0, 21.0, 23.0, 25.0, 28.0, 32.0, 36.0,
];

/// Le livre lu en tâche de fond, avec la taille de ses images.
type Loaded = Result<(Ebook, HashMap<String, Vec2>), String>;

/// Le livre ouvert.
pub struct TextBook {
    pub path: PathBuf,
    loading: Option<Receiver<Loaded>>,
    book: Option<Arc<Ebook>>,
    /// La taille de chaque image, en pixels.
    image_sizes: HashMap<String, Vec2>,
    pub error: Option<String>,
    pub position: Position,
    /// Caractères avant chaque bloc ; le dernier : le total.
    before: Vec<usize>,
    layout: Layout,
    textures: HashMap<String, Option<TextureHandle>>,
    /// La double page (ou page) montrée : chapitre, et sa première page.
    shown: Option<(usize, usize)>,
    turn: Option<Turn>,
    contents_open: bool,
    /// Le sommaire vient de s'ouvrir (ou de se fermer) : le clic qui l'a fait
    /// ne le referme pas.
    contents_toggled: bool,
    scroll: f32,
    /// Tourner la page à la prochaine image (captures de test).
    pub advance: bool,
}

/// Une page qui tourne : celle qu'on quitte, et dans quel sens.
struct Turn {
    from: (usize, usize),
    forward: bool,
    start: Option<f64>,
}

/// Ce que la lecture demande à l'appli.
#[derive(Default)]
pub struct Out {
    pub close: bool,
    pub fullscreen: bool,
    /// La position ou les réglages ont changé : à enregistrer.
    pub changed: bool,
}

/// Les dimensions de la mise en page : la changer refait les pages.
#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    /// Largeur d'une colonne de texte et hauteur d'une page, en points.
    width: f32,
    height: f32,
    /// Une page, ou deux côte à côte.
    columns: usize,
    size: f32,
    sans: bool,
}

#[derive(Default)]
struct Layout {
    geometry: Option<Geometry>,
    /// Les pages de chaque chapitre déjà mis en page.
    chapters: HashMap<usize, Vec<Page>>,
}

#[derive(Default)]
struct Page {
    pieces: Vec<Piece>,
}

enum Piece {
    /// Des lignes d'un paragraphe : `galley` posé en `at` (par rapport au
    /// coin de la page), montré seulement dans `clip`.
    Text {
        galley: Arc<Galley>,
        at: Pos2,
        clip: Rect,
        block: usize,
        /// Le caractère du bloc où commence la première ligne.
        offset: usize,
        muted: bool,
    },
    Image {
        key: String,
        rect: Rect,
        block: usize,
    },
}

impl Piece {
    fn anchor(&self) -> (usize, usize) {
        match self {
            Piece::Text { block, offset, .. } => (*block, *offset),
            Piece::Image { block, .. } => (*block, 0),
        }
    }
}

impl Page {
    /// Le bloc et le caractère où commence la page.
    fn anchor(&self) -> Option<(usize, usize)> {
        self.pieces.first().map(Piece::anchor)
    }
}

impl TextBook {
    pub fn open(path: PathBuf, position: Position, repaint: egui::Context) -> Self {
        let (outbox, inbox) = channel();
        let file = path.clone();
        std::thread::Builder::new()
            .name("serj-livre".into())
            .spawn(move || {
                let result = serj::ebook::open(&file).map(|book| {
                    let sizes = book
                        .images
                        .iter()
                        .filter_map(|(key, bytes)| {
                            let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
                                .with_guessed_format()
                                .ok()?
                                .into_dimensions()
                                .ok()?;
                            Some((key.clone(), vec2(w as f32, h as f32)))
                        })
                        .collect();
                    (book, sizes)
                });
                let _ = outbox.send(result);
                repaint.request_repaint();
            })
            .expect("thread du livre");
        Self {
            path,
            loading: Some(inbox),
            book: None,
            image_sizes: HashMap::new(),
            error: None,
            position,
            before: Vec::new(),
            layout: Layout::default(),
            textures: HashMap::new(),
            shown: None,
            turn: None,
            contents_open: false,
            contents_toggled: false,
            scroll: 0.0,
            advance: false,
        }
    }

    /// Le livre est lu et prêt à montrer.
    pub fn ready(&self) -> bool {
        self.book.is_some()
    }

    /// Rien ne bouge : prêt pour une capture.
    pub fn idle(&self) -> bool {
        (self.book.is_some() && self.turn.is_none() && self.shown.is_some()) || self.error.is_some()
    }

    fn receive(&mut self) {
        let Some(inbox) = &self.loading else { return };
        let Ok(result) = inbox.try_recv() else { return };
        self.loading = None;
        match result {
            Ok((book, sizes)) => {
                let mut total = 0;
                self.before = book
                    .blocks
                    .iter()
                    .map(|b| {
                        let at = total;
                        total += b.len();
                        at
                    })
                    .collect();
                self.before.push(total);
                if self.position.pages != book.blocks.len()
                    || self.position.page >= book.blocks.len()
                {
                    // Un livre jamais ouvert (ou qui a changé) commence au début.
                    if self.position.page >= book.blocks.len() {
                        self.position.page = 0;
                        self.position.panel = 0;
                    }
                    self.position.pages = book.blocks.len();
                }
                self.image_sizes = sizes;
                self.book = Some(Arc::new(book));
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Le chapitre du bloc `block`.
    fn chapter_of(&self, block: usize) -> usize {
        let book = self.book.as_ref().expect("livre lu");
        book.chapters
            .iter()
            .rposition(|c| c.start <= block)
            .unwrap_or(0)
    }

    fn chapter_blocks(&self, chapter: usize) -> Range<usize> {
        let book = self.book.as_ref().expect("livre lu");
        let start = book.chapters[chapter].start;
        let end = book
            .chapters
            .get(chapter + 1)
            .map_or(book.blocks.len(), |c| c.start);
        start..end
    }

    /// Les pages du chapitre, mises en page au besoin.
    fn pages(&mut self, painter: &Painter, chapter: usize) -> &Vec<Page> {
        if !self.layout.chapters.contains_key(&chapter) {
            let geometry = self.layout.geometry.expect("mise en page choisie");
            let pages = self.paginate(painter, chapter, geometry);
            self.layout.chapters.insert(chapter, pages);
        }
        &self.layout.chapters[&chapter]
    }

    /// La page du chapitre où se trouve `(block, offset)`.
    fn page_at(&mut self, painter: &Painter, chapter: usize, at: (usize, usize)) -> usize {
        let pages = self.pages(painter, chapter);
        pages
            .iter()
            .rposition(|p| p.anchor().is_some_and(|a| a <= at))
            .unwrap_or(0)
    }

    /// Le premier rang de la double page qui contient la page `page`.
    fn view_of(&self, page: usize) -> usize {
        let columns = self.layout.geometry.map_or(1, |g| g.columns);
        page - page % columns
    }

    /// Va à `(chapter, page)`, en tournant la page si `animate`.
    fn go(&mut self, painter: &Painter, chapter: usize, page: usize, animate: bool, now: f64) {
        let page = self.view_of(page);
        let from = self.shown;
        if from == Some((chapter, page)) {
            return;
        }
        self.shown = Some((chapter, page));
        if let Some(anchor) = self
            .pages(painter, chapter)
            .get(page)
            .and_then(Page::anchor)
        {
            self.position.page = anchor.0;
            self.position.panel = anchor.1;
        }
        self.turn = match from {
            Some(from) if animate => Some(Turn {
                forward: (chapter, page) > from,
                from,
                start: Some(now),
            }),
            _ => None,
        };
    }

    fn next(&mut self, painter: &Painter, now: f64) {
        let Some((chapter, page)) = self.shown else {
            return;
        };
        let columns = self.layout.geometry.map_or(1, |g| g.columns);
        let count = self.pages(painter, chapter).len();
        let chapters = self.book.as_ref().map_or(0, |b| b.chapters.len());
        if page + columns < count {
            self.go(painter, chapter, page + columns, true, now);
        } else if chapter + 1 < chapters {
            self.go(painter, chapter + 1, 0, true, now);
        }
    }

    fn previous(&mut self, painter: &Painter, now: f64) {
        let Some((chapter, page)) = self.shown else {
            return;
        };
        if page > 0 {
            let columns = self.layout.geometry.map_or(1, |g| g.columns);
            self.go(painter, chapter, page.saturating_sub(columns), true, now);
        } else if chapter > 0 {
            let last = self.pages(painter, chapter - 1).len().saturating_sub(1);
            self.go(painter, chapter - 1, last, true, now);
        }
    }

    /// Va à la part `t` (0 à 1) du livre.
    fn jump(&mut self, painter: &Painter, t: f32, now: f64) {
        let Some(&total) = self.before.last() else {
            return;
        };
        let target = (t.clamp(0.0, 1.0) * total as f32) as usize;
        let block = self
            .before
            .partition_point(|&b| b <= target)
            .saturating_sub(1);
        let chapter = self.chapter_of(block);
        let page = self.page_at(painter, chapter, (block, 0));
        self.go(painter, chapter, page, false, now);
    }

    /// Part du livre lue, de 0 à 1 : jusqu'au bas de la page montrée.
    pub fn progress(&mut self, painter: &Painter) -> Option<f32> {
        let (chapter, page) = self.shown?;
        let total = *self.before.last()?;
        if total == 0 {
            return None;
        }
        let columns = self.layout.geometry.map_or(1, |g| g.columns);
        let chapters = self.book.as_ref()?.chapters.len();
        let pages = self.pages(painter, chapter);
        let next = pages.get(page + columns).and_then(Page::anchor);
        let last = chapter + 1 == chapters && next.is_none();
        if last {
            return Some(1.0);
        }
        let end = match next {
            Some((block, offset)) => self.before[block] + offset,
            None => self.before[self.chapter_blocks(chapter).end],
        };
        Some((end as f32 / total as f32).clamp(0.0, 1.0))
    }

    /// Met en page le chapitre : chaque bloc en lignes, les lignes en pages.
    fn paginate(&self, painter: &Painter, chapter: usize, g: Geometry) -> Vec<Page> {
        let book = self.book.as_ref().expect("livre lu").clone();
        let blocks = self.chapter_blocks(chapter);
        let fs = g.size;
        let mut pages = vec![Page::default()];
        let mut y = 0.0;
        let mut previous: Option<&Block> = None;
        for index in blocks.clone() {
            let block = &book.blocks[index];
            let first = index == blocks.start;
            match block {
                Block::Image(key) => {
                    let Some(&px) = self.image_sizes.get(key) else {
                        continue;
                    };
                    let natural = px / 1.5;
                    let inline = (g.width / natural.x)
                        .min(g.height * 0.85 / natural.y)
                        .min(1.0);
                    // Une grande image (la couverture, une illustration) : seule
                    // sur sa page, aussi grande que la page.
                    let alone = natural.y * inline >= g.height * 0.55;
                    let scale = if alone {
                        (g.width / natural.x).min(g.height / natural.y).min(2.0)
                    } else {
                        inline
                    };
                    let size = natural * scale;
                    let gap = fs * 0.8;
                    if y > 0.0 && (alone || y + gap + size.y > g.height) {
                        pages.push(Page::default());
                        y = 0.0;
                    }
                    let top = if alone {
                        (g.height - size.y) / 2.0
                    } else if y > 0.0 {
                        y + gap
                    } else {
                        0.0
                    };
                    let rect = Rect::from_min_size(pos2((g.width - size.x) / 2.0, top), size);
                    pages
                        .last_mut()
                        .expect("une page")
                        .pieces
                        .push(Piece::Image {
                            key: key.clone(),
                            rect,
                            block: index,
                        });
                    if alone {
                        pages.push(Page::default());
                        y = 0.0;
                    } else {
                        y = top + size.y + gap;
                    }
                }
                _ => {
                    let (job, inset, anchor_x, gap_before, gap_after, muted) =
                        self.job(block, previous, first, g);
                    let galley = justified(painter, job);
                    // Un titre de chapitre descend un peu dans la page.
                    let mut top = if y > 0.0 { y + gap_before } else { 0.0 };
                    if first && matches!(block, Block::Heading { .. }) {
                        top = g.height * 0.12;
                    }
                    let rows = &galley.rows;
                    let mut i = 0;
                    let mut offset = 0;
                    while i < rows.len() {
                        let base = rows[i].min_y();
                        let mut j = i;
                        while j < rows.len() && top + rows[j].max_y() - base <= g.height {
                            j += 1;
                        }
                        // Pas même une ligne : page suivante. Une seule ligne d'un
                        // paragraphe en bas de page : il commence sur la suivante.
                        let lonely = i == 0 && j == 1 && rows.len() > 1;
                        if (j == i || lonely) && top > 0.0 {
                            pages.push(Page::default());
                            top = 0.0;
                            continue;
                        }
                        j = j.max(i + 1);
                        let slack = fs * 0.15;
                        let clip = Rect::from_min_max(
                            pos2(-fs * 2.0, top - slack),
                            pos2(g.width + fs * 2.0, top + rows[j - 1].max_y() - base + slack),
                        );
                        pages
                            .last_mut()
                            .expect("une page")
                            .pieces
                            .push(Piece::Text {
                                galley: galley.clone(),
                                at: pos2(anchor_x + inset, top - base),
                                clip,
                                block: index,
                                offset,
                                muted,
                            });
                        offset += rows[i..j]
                            .iter()
                            .map(|r| r.char_count_including_newline().0)
                            .sum::<usize>();
                        y = top + rows[j - 1].max_y() - base;
                        if j < rows.len() {
                            pages.push(Page::default());
                            top = 0.0;
                        }
                        i = j;
                    }
                    y += gap_after;
                }
            }
            previous = Some(block);
        }
        pages.retain(|p| !p.pieces.is_empty());
        if pages.is_empty() {
            pages.push(Page::default());
        }
        pages
    }

    /// Comment poser un bloc : son texte mis en forme, son retrait à gauche,
    /// où l'ancrer, l'espace avant et après, s'il est en gris.
    fn job(
        &self,
        block: &Block,
        previous: Option<&Block>,
        first: bool,
        g: Geometry,
    ) -> (LayoutJob, f32, f32, f32, f32, bool) {
        let fs = g.size;
        let family = |bold: bool, italic: bool| {
            let base = if g.sans { "sans" } else { "serif" };
            let name = match (bold, italic) {
                (false, false) => base.to_string(),
                (false, true) => format!("{base}-italic"),
                (true, false) => format!("{base}-bold"),
                (true, true) => format!("{base}-bold-italic"),
            };
            FontFamily::Name(name.into())
        };
        let mut job = LayoutJob::default();
        let add = |job: &mut LayoutJob, spans: &[Span], size: f32, line: f32, bold: bool| {
            for span in spans {
                job.append(
                    &span.text,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(
                            size,
                            family(bold || span.style.bold, span.style.italic),
                        ),
                        line_height: Some(line),
                        color: Color32::PLACEHOLDER,
                        ..TextFormat::default()
                    },
                );
            }
        };
        match block {
            Block::Heading { level, spans } => {
                let (size, centered) = match level {
                    1 | 2 => (fs * 1.5, true),
                    3 => (fs * 1.25, true),
                    _ => (fs * 1.08, false),
                };
                add(&mut job, spans, size, size * 1.3, true);
                job.wrap.max_width = g.width;
                job.halign = if centered {
                    egui::Align::Center
                } else {
                    egui::Align::LEFT
                };
                let x = if centered { g.width / 2.0 } else { 0.0 };
                let before = if first { 0.0 } else { fs * 1.6 };
                (job, 0.0, x, before, fs * 1.4, false)
            }
            Block::Break => {
                add(
                    &mut job,
                    &[Span {
                        text: "*   *   *".into(),
                        style: Default::default(),
                    }],
                    fs,
                    fs * 1.5,
                    false,
                );
                job.wrap.max_width = g.width;
                job.halign = egui::Align::Center;
                (job, 0.0, g.width / 2.0, fs * 0.7, fs * 0.7, true)
            }
            Block::Paragraph { spans, align } => {
                let line = fs * 1.55;
                let (inset, width) = match align {
                    Align::Quote => (fs * 1.6, g.width - fs * 3.2),
                    _ => (0.0, g.width),
                };
                // Alinéa : seulement pour un paragraphe qui en suit un autre.
                let indent = *align == Align::Normal
                    && matches!(
                        previous,
                        Some(Block::Paragraph {
                            align: Align::Normal,
                            ..
                        })
                    );
                if indent {
                    // Un signe invisible plutôt qu'un espace : la justification
                    // garde les espaces du début de ligne, pas les blancs.
                    job.append(
                        "M",
                        0.0,
                        TextFormat {
                            font_id: FontId::new(fs, family(false, false)),
                            extra_letter_spacing: fs * 0.45,
                            line_height: Some(line),
                            color: Color32::TRANSPARENT,
                            ..TextFormat::default()
                        },
                    );
                }
                add(&mut job, spans, fs, line, false);
                job.wrap.max_width = width;
                let (halign, x) = match align {
                    Align::Center => (egui::Align::Center, width / 2.0),
                    Align::Right => (egui::Align::RIGHT, width),
                    _ => (egui::Align::LEFT, 0.0),
                };
                job.halign = halign;
                job.justify = matches!(align, Align::Normal | Align::Quote);
                let (before, after) = match align {
                    Align::Normal => (fs * 0.18, 0.0),
                    _ => (fs * 0.5, fs * 0.3),
                };
                (job, inset, x, before, after, false)
            }
            Block::Image(_) => unreachable!("les images ne passent pas par ici"),
        }
    }

    fn texture(&mut self, ctx: &egui::Context, key: &str) -> Option<TextureHandle> {
        if !self.textures.contains_key(key) {
            let texture = self
                .book
                .as_ref()
                .and_then(|b| b.images.get(key))
                .and_then(|bytes| image::load_from_memory(bytes).ok())
                .map(|image| {
                    let image = image.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                    ctx.load_texture(format!("livre-{key}"), color, egui::TextureOptions::LINEAR)
                });
            self.textures.insert(key.to_string(), texture);
        }
        self.textures[key].clone()
    }

    /// Dessine la page `page` du chapitre `chapter`, son coin en haut à gauche
    /// en `origin`.
    fn paint_page(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        chapter: usize,
        page: usize,
        origin: Pos2,
        colors: &Colors,
    ) {
        let Some(pieces) = self.layout.chapters.get(&chapter).and_then(|p| p.get(page)) else {
            return;
        };
        let mut images = Vec::new();
        for piece in &pieces.pieces {
            match piece {
                Piece::Text {
                    galley,
                    at,
                    clip,
                    muted,
                    ..
                } => {
                    let clip = clip
                        .translate(origin.to_vec2())
                        .intersect(painter.clip_rect());
                    let color = if *muted { colors.muted } else { colors.text };
                    painter.with_clip_rect(clip).galley(
                        origin + at.to_vec2(),
                        galley.clone(),
                        color,
                    );
                }
                Piece::Image { key, rect, .. } => images.push((key.clone(), *rect)),
            }
        }
        for (key, rect) in images {
            if let Some(t) = self.texture(ctx, &key) {
                let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                painter.image(t.id(), rect.translate(origin.to_vec2()), uv, Color32::WHITE);
            }
        }
    }

    /// La lecture : la page (ou la double page), les commandes, le sommaire.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        now: f64,
        typography: &mut Typography,
        hud_until: &mut f64,
        title: &str,
    ) -> Out {
        let ctx = ui.ctx().clone();
        self.receive();
        let mut out = Out::default();
        let colors = Colors::of(typography.paper);
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0.0, colors.paper);

        // Clavier, souris, molette.
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let typing = ctx.memory(|m| m.focused().is_some());
        let mut next = false;
        let mut previous = false;
        let mut first = false;
        let mut last = false;
        let mut bigger = false;
        let mut smaller = false;
        if !typing {
            ctx.input_mut(|i| {
                let mut any = |keys: &[Key]| {
                    keys.iter().fold(false, |hit, k| {
                        i.consume_key(egui::Modifiers::NONE, *k) | hit
                    })
                };
                next = any(&[
                    Key::ArrowRight,
                    Key::ArrowDown,
                    Key::Space,
                    Key::PageDown,
                    Key::Enter,
                ]);
                previous = any(&[Key::ArrowLeft, Key::ArrowUp, Key::PageUp, Key::Backspace]);
                first = any(&[Key::Home]);
                last = any(&[Key::End]);
                bigger = any(&[Key::Plus, Key::Equals]);
                smaller = any(&[Key::Minus]);
                out.fullscreen = any(&[Key::F]);
                let escape = any(&[Key::Escape]);
                if escape {
                    if self.contents_open {
                        self.contents_open = false;
                    } else if is_fullscreen {
                        out.fullscreen = true;
                    } else {
                        out.close = true;
                    }
                }
                previous |= i.consume_key(egui::Modifiers::SHIFT, Key::Space);
                let cmd = egui::Modifiers::COMMAND;
                bigger |= i.consume_key(cmd, Key::Plus) | i.consume_key(cmd, Key::Equals);
                smaller |= i.consume_key(cmd, Key::Minus);
                out.close |= i.consume_key(cmd, Key::W);
            });
        }
        // Clic à droite pour avancer, à gauche pour reculer ; glisser la page
        // du doigt (ou de la souris) la tourne aussi.
        let response = ui.interact(area, ui.id().with("livre"), Sense::click_and_drag());
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            if p.x > area.left() + area.width() / 3.0 {
                next = true;
            } else {
                previous = true;
            }
        }
        if response.drag_stopped() {
            let dx = response.drag_delta().x + ctx.input(|i| i.pointer.delta().x);
            let total = ctx.input(|i| {
                i.pointer
                    .press_origin()
                    .zip(i.pointer.latest_pos())
                    .map(|(a, b)| b.x - a.x)
            });
            let dx = total.unwrap_or(dx);
            if dx < -60.0 {
                next = true;
            } else if dx > 60.0 {
                previous = true;
            }
        }
        let wheel = ctx.input(|i| i.smooth_scroll_delta);
        let wheel = if wheel.x.abs() > wheel.y.abs() {
            wheel.x
        } else {
            wheel.y
        };
        if !self.contents_open {
            self.scroll += wheel;
        }
        if self.scroll.abs() > 90.0 {
            if self.scroll < 0.0 {
                next = true;
            } else {
                previous = true;
            }
            self.scroll = 0.0;
        }
        if wheel == 0.0 {
            self.scroll *= 0.8;
        }
        next |= std::mem::take(&mut self.advance);
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO) {
            *hud_until = hud_until.max(now + 1.0);
        }

        if let Some(error) = &self.error {
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                error,
                FontId::proportional(18.0),
                colors.muted,
            );
            self.toolbar(ui, area, 1.0, is_fullscreen, title, typography, &mut out);
            return out;
        }
        if self.book.is_none() {
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                "Ouverture…",
                FontId::proportional(16.0),
                colors.muted,
            );
            ctx.request_repaint_after(Duration::from_millis(100));
            self.toolbar(ui, area, 1.0, is_fullscreen, title, typography, &mut out);
            return out;
        }

        // La mise en page : une colonne de lecture confortable (une soixantaine
        // de signes par ligne), deux côte à côte s'il y a la place.
        let size = typography.size;
        let margin_y = (area.height() * 0.09).clamp(44.0, 84.0);
        let height = (area.height() - 2.0 * margin_y).max(size * 4.0);
        let ideal = (size * 31.0).clamp(260.0, 760.0);
        let gutter = (size * 4.0).max(56.0);
        let side = if area.width() > 700.0 { 48.0 } else { 20.0 };
        let columns = if area.width() - 2.0 * side >= 2.0 * ideal * 0.82 + gutter
            && area.width() > area.height() * 1.15
        {
            2
        } else {
            1
        };
        let width = ((area.width() - 2.0 * side - (columns - 1) as f32 * gutter) / columns as f32)
            .min(ideal);
        let geometry = Geometry {
            width: width.round(),
            height: height.round(),
            columns,
            size,
            sans: typography.sans,
        };
        if self.layout.geometry != Some(geometry) {
            self.layout = Layout {
                geometry: Some(geometry),
                chapters: HashMap::new(),
            };
            self.shown = None;
            self.turn = None;
        }
        if self.shown.is_none() {
            let at = (self.position.page, self.position.panel);
            let chapter = self.chapter_of(at.0);
            let page = self.page_at(&painter, chapter, at);
            self.go(&painter, chapter, page, false, now);
        }

        // Les commandes du bas et du haut, avant de bouger.
        let mut jump = None;
        if first {
            jump = Some(0.0);
        }
        if last {
            jump = Some(1.0);
        }
        if bigger || smaller {
            let at = SIZES
                .iter()
                .position(|s| *s >= typography.size)
                .unwrap_or(4);
            let at = if bigger {
                (at + 1).min(SIZES.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            typography.size = SIZES[at];
            out.changed = true;
            *hud_until = now + HUD_SECS;
        }
        if next && !self.contents_open {
            self.next(&painter, now);
        }
        if previous && !self.contents_open {
            self.previous(&painter, now);
        }
        if let Some(t) = jump {
            self.jump(&painter, t, now);
        }

        // Les pages, et celles qu'on quitte en fondu.
        let block_w = columns as f32 * width + (columns - 1) as f32 * gutter;
        let origin = pos2(area.center().x - block_w / 2.0, area.top() + margin_y);
        let (chapter, page) = self.shown.expect("page montrée");
        let pages_count = self.pages(&painter, chapter).len();
        let t = match &mut self.turn {
            Some(turn) => {
                let start = *turn.start.get_or_insert(now);
                ((now - start) / TURN_SECS).clamp(0.0, 1.0) as f32
            }
            None => 1.0,
        };
        if t >= 1.0 {
            self.turn = None;
        }
        let ease = 1.0 - (1.0 - t).powi(3);
        let paint_view =
            |this: &mut Self, chapter: usize, page: usize, shift: f32, opacity: f32| {
                let mut painter = painter.clone();
                painter.set_opacity(opacity);
                for k in 0..columns {
                    let at = origin + vec2(k as f32 * (width + gutter) + shift, 0.0);
                    this.paint_page(&ctx, &painter, chapter, page + k, at, &colors);
                }
            };
        if let Some(turn) = &self.turn {
            let dir = if turn.forward { 1.0 } else { -1.0 };
            let from = turn.from;
            // Celle qu'on quitte s'efface vite en reculant un peu ; la nouvelle
            // arrive de l'autre côté.
            let fade = (1.0 - ease * 1.6).max(0.0);
            paint_view(self, from.0, from.1, -dir * TURN_SHIFT * ease, fade);
            paint_view(self, chapter, page, dir * TURN_SHIFT * (1.0 - ease), ease);
            ctx.request_repaint();
        } else {
            paint_view(self, chapter, page, 0.0, 1.0);
        }

        // En marge : le chapitre en haut, l'avancement en bas, discrets.
        let progress = self.progress(&painter);
        if let Some(p) = progress {
            self.position.progress = p;
        }
        let book = self.book.clone().expect("livre lu");
        let chapter_title = book.chapters[chapter].title.clone();
        let running = if chapter_title.is_empty() {
            book.metadata.title.clone().unwrap_or_default()
        } else {
            chapter_title.clone()
        };
        let small = FontId::new(12.5, FontFamily::Proportional);
        // Les commandes, un instant après chaque geste, en fondu ; le titre
        // courant s'efface dessous.
        let hud_shown = now < *hud_until || self.contents_open;
        let opacity = ctx.animate_bool_with_time(ui.id().with("hud-livre"), hud_shown, 0.2);
        if !running.is_empty() && opacity < 1.0 {
            let mut job = LayoutJob::simple(running, small.clone(), colors.muted, block_w);
            job.wrap.max_rows = 1;
            job.halign = egui::Align::Center;
            let galley = painter.layout_job(job);
            painter.galley(
                pos2(area.center().x, area.top() + margin_y * 0.42),
                galley,
                colors.muted.gamma_multiply(1.0 - opacity),
            );
        }
        let footer = match (columns, progress) {
            (_, Some(p)) => {
                let shown = if columns == 2 && page + 1 < pages_count {
                    format!("{}–{} / {pages_count}", page + 1, page + 2)
                } else {
                    format!("{} / {pages_count}", page + 1)
                };
                format!("{shown}   ·   {} %", (p * 100.0).floor() as u32)
            }
            _ => String::new(),
        };
        painter.text(
            pos2(area.center().x, area.bottom() - margin_y * 0.45),
            Align2::CENTER_CENTER,
            footer,
            small,
            colors.muted,
        );

        // La barre d'avancement, tout en bas : on y clique pour sauter.
        let mut bar_jump = None;
        if let Some(p) = progress {
            let zone = Rect::from_min_max(pos2(area.left(), area.bottom() - 18.0), area.max);
            let bar = ui.interact(
                zone,
                ui.id().with("avancement-livre"),
                Sense::click_and_drag(),
            );
            let active = bar.hovered() || bar.dragged();
            let h = if active { 8.0 } else { 3.0 };
            let track = Rect::from_min_max(pos2(area.left(), area.bottom() - h), area.max);
            painter.rect_filled(
                track,
                0.0,
                colors.text.gamma_multiply(if active { 0.16 } else { 0.07 }),
            );
            let mut done = track;
            done.max.x = track.left() + track.width() * p;
            painter.rect_filled(
                done,
                0.0,
                theme()
                    .accent
                    .gamma_multiply(if active { 1.0 } else { 0.7 }),
            );
            if active {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                *hud_until = hud_until.max(now + 1.0);
            }
            if let Some(pos) = bar.hover_pos().or(bar.interact_pointer_pos()) {
                let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
                let label = painter.layout_no_wrap(
                    format!("{} %", (t * 100.0).floor() as u32),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                let x = pos.x.clamp(area.left() + 40.0, area.right() - 40.0);
                let tip = Rect::from_center_size(
                    pos2(x, track.top() - 20.0),
                    label.size() + vec2(16.0, 8.0),
                );
                painter.rect_filled(tip, 6.0, Color32::from_black_alpha(220));
                painter.galley(tip.center() - label.size() / 2.0, label, Color32::WHITE);
                if bar.clicked() || bar.drag_stopped() {
                    bar_jump = Some(t);
                }
            }
        }

        if hud_shown {
            ctx.request_repaint_after(Duration::from_secs_f64((*hud_until - now).max(0.05)));
        } else if is_fullscreen {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        if opacity > 0.0 {
            let hovered = self.toolbar(
                ui,
                area,
                opacity,
                is_fullscreen,
                title,
                typography,
                &mut out,
            );
            if hovered {
                *hud_until = hud_until.max(now + 1.0);
            }
        }
        let mut go_chapter = None;
        if self.contents_open {
            go_chapter = self.contents(&ctx, &book, chapter, &colors);
        }
        self.contents_toggled = false;
        if let Some(c) = go_chapter {
            self.contents_open = false;
            self.go(&painter, c, 0, false, now);
        }
        if let Some(t) = bar_jump {
            self.jump(&painter, t, now);
        }
        // Tourner les pages ne fait pas apparaître les commandes : rien ne
        // vient entre le texte et soi.
        if self.shown != Some((chapter, page)) || jump.is_some() {
            out.changed = true;
        }
        out
    }

    /// La barre d'outils en haut ; renvoie si la souris est dessus.
    #[allow(clippy::too_many_arguments)]
    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        opacity: f32,
        fullscreen: bool,
        title: &str,
        typography: &mut Typography,
        out: &mut Out,
    ) -> bool {
        let mut painter = ui.painter_at(area);
        painter.set_opacity(opacity);
        let band = Rect::from_min_size(area.min, vec2(area.width(), 110.0));
        let [r, g, b, _] = Colors::of(typography.paper).paper.to_array();
        widgets::shade(
            &painter,
            band,
            Color32::from_rgba_unmultiplied(r, g, b, 250),
            Color32::TRANSPARENT,
        );
        let inner = Rect::from_min_max(
            area.min + vec2(14.0, 14.0),
            pos2(area.right() - 14.0, area.top() + 48.0),
        );
        let compact = area.width() < 900.0;
        let mut hovered = false;
        let mut right_edge = inner.right();
        let before = *typography;
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.multiply_opacity(opacity);
                ui.spacing_mut().item_spacing.x = 6.0;
                let icon = if fullscreen {
                    Icon::ExitFullscreen
                } else {
                    Icon::Fullscreen
                };
                out.fullscreen |= widgets::button(ui, Some(icon), "", Tone::Plain)
                    .on_hover_text("Plein écran (F)")
                    .clicked();
                ui.add_space(10.0);
                for paper in [Paper::Night, Paper::Sepia, Paper::White, Paper::Theme] {
                    if paper_swatch(ui, paper, typography.paper == paper) {
                        typography.paper = paper;
                    }
                }
                ui.add_space(10.0);
                let font = if typography.sans { "Sans" } else { "Serif" };
                if widgets::button(ui, None, font, Tone::Plain)
                    .on_hover_text("Police avec ou sans empattements")
                    .clicked()
                {
                    typography.sans = !typography.sans;
                }
                if widgets::button(ui, Some(Icon::Plus), "", Tone::Plain)
                    .on_hover_text("Texte plus grand (+)")
                    .clicked()
                {
                    let at = SIZES
                        .iter()
                        .position(|s| *s > typography.size)
                        .unwrap_or(SIZES.len() - 1);
                    typography.size = SIZES[at];
                }
                ui.label(
                    egui::RichText::new(format!("{}", typography.size as u32))
                        .size(13.0)
                        .color(theme().muted),
                );
                if widgets::button(ui, Some(Icon::Minus), "", Tone::Plain)
                    .on_hover_text("Texte plus petit (−)")
                    .clicked()
                {
                    let at = SIZES
                        .iter()
                        .rposition(|s| *s < typography.size)
                        .unwrap_or(0);
                    typography.size = SIZES[at];
                }
                ui.add_space(10.0);
                let label = if compact { "" } else { "Sommaire" };
                let tone = if self.contents_open {
                    Tone::Active
                } else {
                    Tone::Plain
                };
                if widgets::button(ui, Some(Icon::Contents), label, tone)
                    .on_hover_text("Les chapitres du livre")
                    .clicked()
                {
                    self.contents_open = !self.contents_open;
                    self.contents_toggled = true;
                }
                right_edge = ui.min_rect().left();
                hovered |= ui.rect_contains_pointer(ui.min_rect());
            },
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.multiply_opacity(opacity);
                out.close |= widgets::button(
                    ui,
                    Some(Icon::Back),
                    if compact { "" } else { "Accueil" },
                    Tone::Plain,
                )
                .on_hover_text("Revenir à la bibliothèque (Échap)")
                .clicked();
                hovered |= ui.rect_contains_pointer(ui.min_rect());
                let left = ui.min_rect().right() + 16.0;
                let room = right_edge - left - 16.0;
                if room > 60.0 {
                    let mut job = LayoutJob::simple(
                        title.to_owned(),
                        FontId::proportional(14.0),
                        Color32::WHITE,
                        room,
                    );
                    job.wrap.max_rows = 1;
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    painter.galley(
                        pos2(left, inner.center().y - galley.size().y / 2.0),
                        galley,
                        Colors::of(typography.paper).text,
                    );
                }
            },
        );
        if *typography != before {
            out.changed = true;
        }
        hovered
    }

    /// Le sommaire, en panneau à gauche ; renvoie le chapitre choisi.
    fn contents(
        &mut self,
        ctx: &egui::Context,
        book: &Ebook,
        current: usize,
        colors: &Colors,
    ) -> Option<usize> {
        let mut chosen = None;
        let screen = ctx.content_rect();
        let width = 340.0_f32.min(screen.width() - 40.0);
        let frame = egui::Frame::new()
            .fill(colors.paper.lerp_to_gamma(colors.text, 0.05))
            .stroke(egui::Stroke::new(1.0, colors.text.gamma_multiply(0.1)))
            .corner_radius(14)
            .inner_margin(18)
            .shadow(egui::epaint::Shadow {
                offset: [0, 10],
                blur: 30,
                spread: 0,
                color: Color32::from_black_alpha(90),
            });
        let area = egui::Area::new(egui::Id::new("sommaire"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.left() + 16.0, screen.top() + 60.0));
        let response = area.show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                ui.label(
                    egui::RichText::new("Sommaire")
                        .size(17.0)
                        .strong()
                        .color(colors.text),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(screen.height() - 160.0)
                    .show(ui, |ui| {
                        for (i, c) in book.chapters.iter().enumerate() {
                            let name = if c.title.is_empty() {
                                if i == 0 {
                                    "Début".to_string()
                                } else {
                                    format!("Chapitre {i}")
                                }
                            } else {
                                c.title.clone()
                            };
                            let here = i == current;
                            let text = egui::RichText::new(name).size(14.5).color(if here {
                                theme().accent
                            } else {
                                colors.text
                            });
                            let r = ui.add(
                                egui::Button::new(if here { text.strong() } else { text })
                                    .frame(false)
                                    .wrap(),
                            );
                            if here && self.contents_open {
                                r.scroll_to_me(Some(egui::Align::Center));
                            }
                            if r.clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
            });
        });
        // Un clic à côté le referme.
        if !std::mem::take(&mut self.contents_toggled)
            && ctx.input(|i| i.pointer.any_click())
            && !response
                .response
                .rect
                .contains(ctx.input(|i| i.pointer.interact_pos().unwrap_or_default()))
            && chosen.is_none()
        {
            self.contents_open = false;
        }
        chosen
    }
}

/// Met en page un paragraphe justifié en n'élargissant que les espaces
/// (egui écarte aussi les lettres) : les lignes sont coupées une première
/// fois, puis chaque espace d'une ligne pleine reçoit sa part du blanc qui
/// reste. Le texte ne change pas, sauf le blanc de chaque coupure, qui devient
/// un retour à la ligne.
fn justified(painter: &Painter, job: LayoutJob) -> Arc<Galley> {
    let width = job.wrap.max_width;
    let mut plain = job.clone();
    plain.justify = false;
    let galley = painter.layout_job(plain);
    if !job.justify || galley.rows.len() < 2 || !width.is_finite() {
        return galley;
    }
    let chars: Vec<(usize, char)> = job.text.char_indices().collect();
    let format_at = |byte: usize| {
        job.sections
            .iter()
            .find(|s| s.byte_range.start.0 <= byte && byte < s.byte_range.end.0)
            .or(job.sections.last())
            .map(|s| s.format.clone())
            .unwrap_or_default()
    };
    let mut out = LayoutJob {
        text: String::new(),
        sections: Vec::new(),
        wrap: egui::text::TextWrapping::no_max_width(),
        justify: false,
        ..job.clone()
    };
    // Un morceau de texte, avec un blanc avant lui.
    let mut run = String::new();
    let mut run_format: Option<TextFormat> = None;
    let mut run_lead = 0.0;
    let flush = |out: &mut LayoutJob, run: &mut String, format: &Option<TextFormat>, lead: f32| {
        if let Some(f) = format
            && !run.is_empty()
        {
            out.append(run, lead, f.clone());
        }
        run.clear();
    };
    let mut c = 0;
    for (r, row) in galley.rows.iter().enumerate() {
        let glyphs = &row.row.glyphs;
        let last = r + 1 == galley.rows.len();
        let wrapped = !last && !row.ends_with_newline;
        let end = glyphs
            .iter()
            .rposition(|g| !g.chr.is_whitespace())
            .map_or(0, |i| i + 1);
        let spaces = glyphs[..end].iter().filter(|g| g.chr == ' ').count();
        let used = if end > 0 {
            glyphs[end - 1].max_x() - glyphs[0].pos.x
        } else {
            0.0
        };
        let extra = if wrapped && spaces > 0 {
            ((width - used) / spaces as f32).max(0.0)
        } else {
            0.0
        };
        let mut lead_next = 0.0;
        for (i, glyph) in glyphs.iter().enumerate() {
            let Some(&(byte, _)) = chars.get(c + i) else {
                break;
            };
            if wrapped && i >= end {
                // Le blanc de la coupure : un retour à la ligne à sa place.
                if i == end {
                    flush(&mut out, &mut run, &run_format, run_lead);
                    run_lead = 0.0;
                    run.push('\n');
                    run_format = Some(format_at(byte));
                }
                continue;
            }
            let format = format_at(byte);
            if lead_next > 0.0 || run_format.as_ref() != Some(&format) {
                flush(&mut out, &mut run, &run_format, run_lead);
                run_lead = std::mem::take(&mut lead_next);
                run_format = Some(format);
            }
            run.push(glyph.chr);
            if glyph.chr == ' ' && i < end {
                lead_next = extra;
            }
        }
        if wrapped && end == glyphs.len() {
            // Un mot coupé au milieu (trop long pour la ligne).
            run.push('\n');
        }
        if row.ends_with_newline {
            run.push('\n');
        }
        c += row.char_count_including_newline().0;
    }
    flush(&mut out, &mut run, &run_format, run_lead);
    painter.layout_job(out)
}

/// Les couleurs d'une page.
struct Colors {
    paper: Color32,
    text: Color32,
    muted: Color32,
}

impl Colors {
    fn of(paper: Paper) -> Self {
        let (paper, text) = match paper {
            Paper::Theme => (theme().background, theme().text.gamma_multiply(0.92)),
            Paper::White => (
                Color32::from_rgb(0xFB, 0xFA, 0xF6),
                Color32::from_rgb(0x22, 0x20, 0x1C),
            ),
            Paper::Sepia => (
                Color32::from_rgb(0xF3, 0xE9, 0xD2),
                Color32::from_rgb(0x4E, 0x3B, 0x2A),
            ),
            Paper::Night => (
                Color32::from_rgb(0x10, 0x10, 0x12),
                Color32::from_rgb(0xC4, 0xBE, 0xB3),
            ),
        };
        Self {
            paper,
            text,
            muted: text.gamma_multiply(0.5),
        }
    }
}

/// Une pastille pour choisir le fond des pages ; renvoie si on l'a choisie.
fn paper_swatch(ui: &mut egui::Ui, paper: Paper, current: bool) -> bool {
    let (r, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let colors = Colors::of(paper);
    let c = r.center();
    let painter = ui.painter();
    if current {
        painter.circle_stroke(c, 11.5, egui::Stroke::new(2.0, theme().accent));
    } else if response.hovered() {
        painter.circle_stroke(
            c,
            11.5,
            egui::Stroke::new(1.5, theme().text.gamma_multiply(0.4)),
        );
    }
    painter.circle_filled(c, 8.5, colors.paper);
    painter.circle_stroke(
        c,
        8.5,
        egui::Stroke::new(1.0, theme().text.gamma_multiply(0.2)),
    );
    painter.text(
        c,
        Align2::CENTER_CENTER,
        "a",
        FontId::new(11.0, FontFamily::Name("serif".into())),
        colors.text,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let name = match paper {
        Paper::Theme => "Pages aux couleurs du thème",
        Paper::White => "Pages blanches",
        Paper::Sepia => "Pages sépia",
        Paper::Night => "Pages de nuit",
    };
    response.on_hover_text(name).clicked() && !current
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SERJ_LIVRE=livre.epub cargo test --release --bin serj met_en_page -- --nocapture`
    #[test]
    fn met_en_page() {
        let Some(path) = std::env::var_os("SERJ_LIVRE") else {
            return;
        };
        let ctx = egui::Context::default();
        crate::widgets::install_fonts(&ctx);
        let mut book = TextBook::open(path.into(), Position::default(), ctx.clone());
        while !book.ready() && book.error.is_none() {
            std::thread::sleep(Duration::from_millis(20));
            book.receive();
        }
        assert!(book.error.is_none(), "{:?}", book.error);
        let input = |t: f64| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            time: Some(t),
            ..Default::default()
        };
        let mut typography = Typography::default();
        let mut hud = 0.0;
        for step in 0..40 {
            let started = std::time::Instant::now();
            let _ = ctx.run_ui(input(step as f64), |ui| {
                let area = ui.max_rect();
                book.advance = step > 0;
                book.show(ui, area, step as f64, &mut typography, &mut hud, "Livre");
            });
            let (chapter, page) = book.shown.unwrap();
            println!(
                "pas {step}: chapitre {chapter} page {page}/{} position {:?} {:.1} % en {:?}",
                book.layout.chapters[&chapter].len(),
                (book.position.page, book.position.panel),
                book.position.progress * 100.0,
                started.elapsed()
            );
        }
    }

    /// Rend des pages du livre en images, sans fenêtre :
    /// `SERJ_LIVRE=livre.epub SERJ_RENDU=dossier cargo test --release --bin serj rend_des_pages`.
    #[test]
    fn rend_des_pages() {
        let (Some(path), Some(out)) = (
            std::env::var_os("SERJ_LIVRE"),
            std::env::var_os("SERJ_RENDU"),
        ) else {
            return;
        };
        let out = PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let width: f32 = std::env::var("SERJ_LARGEUR")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(1400.0);
        let mut typography = Typography::default();
        if let Ok(paper) = std::env::var("SERJ_PAPIER") {
            typography.paper = match paper.as_str() {
                "sepia" => Paper::Sepia,
                "blanc" => Paper::White,
                "nuit" => Paper::Night,
                _ => Paper::Theme,
            };
        }
        if let Some(size) = std::env::var("SERJ_TAILLE")
            .ok()
            .and_then(|t| t.parse().ok())
        {
            typography.size = size;
        }
        type State = (Option<TextBook>, Typography, f64);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(width, 900.0))
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(400)
            .wgpu()
            .build_ui_state(
                |ui, (book, typography, hud): &mut State| {
                    let area = ui.max_rect();
                    let now = ui.input(|i| i.time);
                    if let Some(book) = book {
                        book.show(
                            ui,
                            area,
                            now,
                            typography,
                            hud,
                            "Harry Potter à l'École des Sorciers",
                        );
                    }
                },
                (
                    None,
                    typography,
                    if std::env::var_os("SERJ_HUD").is_some() {
                        1e9
                    } else {
                        0.0
                    },
                ),
            );
        // Les polices d'abord : le livre ne se met en page qu'ensuite.
        crate::widgets::install_fonts(&harness.ctx);
        crate::widgets::set_theme(&harness.ctx, "nuit");
        harness.step();
        harness.state_mut().0 = Some(TextBook::open(
            path.into(),
            Position::default(),
            harness.ctx.clone(),
        ));
        while harness
            .state()
            .0
            .as_ref()
            .is_some_and(|b| !b.ready() && b.error.is_none())
        {
            std::thread::sleep(Duration::from_millis(20));
            harness.step();
        }
        let steps: usize = std::env::var("SERJ_PAS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(4);
        for n in 0..steps {
            harness.run_steps(40);
            harness
                .render()
                .unwrap()
                .save(out.join(format!("page{n:02}.png")))
                .unwrap();
            harness.key_press(Key::ArrowRight);
            // Au milieu de la page qui tourne.
            harness.run_steps(9);
            harness
                .render()
                .unwrap()
                .save(out.join(format!("page{n:02}-tourne.png")))
                .unwrap();
        }
    }
}
