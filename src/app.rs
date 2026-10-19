//! La fenêtre de Serj : l'accueil, puis la lecture, où une caméra glisse de
//! case en case et assombrit le reste de la planche.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{
    Align2, Color32, FontId, Key, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2, pos2,
    vec2,
};

use serj::meta::{self, Guess, Info};

use crate::infos::Infos;
use crate::library;
use crate::loader::{Event, LoadedPage, Loader};
use crate::state::{Position, State};
use crate::widgets::{self, ACCENT, BACKGROUND, DIM, Icon, MUTED, ON_ACCENT, SURFACE, TEXT, Tone};

/// Durée du glissement d'une case à l'autre, et du zoom en arrivant sur une page.
const MOVE_SECS: f64 = 0.38;
const ENTER_SECS: f64 = 0.55;
/// Marge autour de la case à l'écran, en points.
const SCREEN_MARGIN: f32 = 20.0;
/// Pages gardées de part et d'autre de la page lue.
const KEEP_AROUND: usize = 2;
/// Combien de temps les commandes et l'indication « Page · Case » restent visibles.
const HUD_SECS: f64 = 2.2;
/// Zoom maximal, et celui de la touche Z.
const MAX_ZOOM: f32 = 6.0;
const KEY_ZOOM: f32 = 2.5;
/// Délais proposés pour la lecture automatique, en secondes.
const AUTO_STEPS: [f64; 12] = [
    2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 15.0, 20.0, 30.0,
];

struct Page {
    texture: TextureHandle,
    size: Vec2,
    /// Cases dans l'ordre de lecture, en pixels de la page, avec un peu d'air autour.
    panels: Vec<Rect>,
}

impl Page {
    fn whole(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.size)
    }
}

/// D'où part la caméra quand elle bouge.
#[derive(Clone, Copy)]
enum From {
    Rect(Rect),
    /// De la planche entière : on arrive sur une nouvelle page.
    WholePage,
}

struct Move {
    from: From,
    /// Début du mouvement ; None tant que la page n'est pas encore rendue.
    start: Option<f64>,
    secs: f64,
}

struct Book {
    path: PathBuf,
    loader: Loader,
    page_count: Option<usize>,
    pages: HashMap<usize, Page>,
    position: Position,
    /// En reculant sur la page précédente, on veut sa dernière case : on ne
    /// sait laquelle qu'une fois la page rendue.
    to_last_panel: bool,
    camera_move: Option<Move>,
    error: Option<String>,
}

/// Ce que demandent le clavier, la souris, la barre d'outils ou la lecture
/// automatique, appliqué en un seul endroit.
#[derive(Default)]
struct Actions {
    next: bool,
    previous: bool,
    first: bool,
    last: bool,
    jump: Option<usize>,
    toggle_page: bool,
    toggle_manga: bool,
    toggle_auto: bool,
    /// Moins (ou plus) de temps par case en lecture automatique.
    faster: bool,
    slower: bool,
    fullscreen: bool,
    close: bool,
    /// Pas de la lecture automatique : sans montrer les commandes.
    auto: bool,
}

/// Ce que l'on a cliqué dans l'accueil.
#[derive(Default)]
struct Clicks {
    open: Option<PathBuf>,
    pick: bool,
    add_folder: bool,
    remove_folder: Option<PathBuf>,
    forget: Option<PathBuf>,
    restart: Option<PathBuf>,
    manga: Option<PathBuf>,
    infos: Option<PathBuf>,
}

/// La fiche d'un album ouverte dans l'accueil, et la correction en cours.
struct Sheet {
    path: PathBuf,
    draft: Option<Draft>,
}

#[derive(Default)]
struct Draft {
    series: String,
    volume: String,
    title: String,
    authors: String,
}

pub struct App {
    state: State,
    book: Option<Book>,
    hud_until: f64,
    scroll: f32,
    state_dirty_since: Option<f64>,
    shots: Option<crate::shots::Shots>,
    auto_next: bool,
    /// Lecture automatique en cours, et depuis quand la case est à l'écran.
    auto_play: bool,
    auto_since: Option<f64>,
    /// Zoom (1 : la case ou la page tient dans l'écran) et décalage du centre
    /// de la vue, en pixels de la page. Remis à zéro à chaque mouvement.
    zoom: f32,
    pan: Vec2,
    library: library::Library,
    infos: Infos,
    sheet: Option<Sheet>,
    /// Le titre de la fenêtre, tel qu'envoyé.
    window_title: String,
    /// Les albums de l'accueil, et quand ils ont été cherchés.
    books: Vec<PathBuf>,
    books_at: Option<f64>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        widgets::install(&cc.egui_ctx);

        let mut app = Self {
            state: State::load(),
            book: None,
            hud_until: 0.0,
            scroll: 0.0,
            state_dirty_since: None,
            shots: crate::shots::Shots::from_env(),
            auto_next: false,
            auto_play: false,
            auto_since: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            library: library::Library::new(cc.egui_ctx.clone()),
            infos: Infos::new(cc.egui_ctx.clone()),
            sheet: None,
            window_title: String::new(),
            books: Vec::new(),
            books_at: None,
        };
        // Le fichier passé en argument, sinon le dernier album lu (sauf pour
        // tester l'accueil avec SERJ_ACCUEIL).
        let reopen = std::env::var_os("SERJ_ACCUEIL").is_none();
        let path = open.or_else(|| {
            app.state
                .recents
                .first()
                .filter(|p| reopen && p.exists())
                .cloned()
        });
        if let Some(path) = path {
            app.open(&cc.egui_ctx, path);
        }
        app
    }

    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.save_position();
        self.state.opened(&path);
        self.state.save();
        let position = self.state.position(&path);
        self.book = Some(Book {
            loader: Loader::open(path.clone(), position.manga, ctx.clone()),
            path,
            page_count: None,
            pages: HashMap::new(),
            position,
            to_last_panel: false,
            camera_move: Some(Move {
                from: From::WholePage,
                start: None,
                secs: ENTER_SECS,
            }),
            error: None,
        });
        self.auto_play = false;
        self.auto_since = None;
        self.hud_until = ctx.input(|i| i.time) + HUD_SECS;
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.save_position();
        self.book = None;
        self.auto_play = false;
        self.window_title.clear();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Serj".into()));
    }

    /// Les infos de l'album `path` (demandées au besoin).
    fn info(&mut self, path: &Path) -> &Info {
        let manga = self.state.position(path).manga;
        let correction = self.state.correction(path).cloned();
        self.infos.get(path, manga, correction.as_ref())
    }

    /// Le nom complet de l'album : « Lanfeust de Troy 2 · Thanos l'incongru ».
    fn label(&mut self, path: &Path) -> String {
        self.info(path).full_title().unwrap_or_else(|| title(path))
    }

    fn pick_file(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().add_filter("BD en PDF", &["pdf", "PDF"]);
        if let Some(dir) = self.state.recents.first().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open(ctx, path);
        }
    }

    fn save_position(&mut self) {
        if let Some(book) = &self.book {
            self.state.set_position(&book.path, book.position);
        }
        self.state.save();
        self.state_dirty_since = None;
    }
}

impl Book {
    fn receive(&mut self, ctx: &egui::Context, whole_page: bool) {
        while let Ok(event) = self.loader.events.try_recv() {
            match event {
                Event::Opened { pages } => {
                    let fresh = self.position.pages == 0;
                    self.page_count = Some(pages);
                    self.position.pages = pages;
                    // Un album jamais lu commence à son début : la dernière
                    // page pour un manga.
                    if fresh || self.position.page >= pages {
                        self.position.page = self.position.rank(0);
                        self.position.panel = 0;
                    }
                }
                Event::Failed(e) => self.error = Some(e),
                Event::Page(page) => {
                    let index = page.index;
                    let page = upload(ctx, page);
                    // Page rendue de nouveau (le sens de lecture a changé) : la
                    // caméra glisse de l'ancienne case vers la nouvelle.
                    if index == self.position.page
                        && self.camera_move.is_none()
                        && let Some(old) = self.pages.get(&index)
                    {
                        let from = self.target(old, whole_page);
                        self.glide(From::Rect(from), MOVE_SECS);
                    }
                    self.pages.insert(index, page);
                }
            }
        }
        if let Some(count) = self.page_count {
            // La page lue d'abord, puis la suivante, puis la précédente, etc.
            // (dans l'ordre de lecture).
            let r = self.position.rank(self.position.page);
            let mut wanted = vec![self.position.page];
            for d in 1..=KEEP_AROUND {
                if r + d < count {
                    wanted.push(self.position.rank(r + d));
                }
                if let Some(back) = r.checked_sub(d) {
                    wanted.push(self.position.rank(back));
                }
            }
            let far: Vec<usize> = self
                .pages
                .keys()
                .copied()
                .filter(|k| !wanted.contains(k))
                .collect();
            for k in far {
                self.pages.remove(&k);
                self.loader.forget(k);
            }
            self.loader.want(wanted);
        }
    }

    fn current(&self) -> Option<&Page> {
        self.pages.get(&self.position.page)
    }

    fn panel_count(&self) -> Option<usize> {
        self.current().map(|p| p.panels.len())
    }

    /// Part de l'album lue, de 0 à 1 : les pages finies, plus la part des
    /// cases lues dans la page en cours.
    fn progress(&self, whole_page: bool) -> Option<f32> {
        let count = self.page_count.filter(|&c| c > 0)? as f32;
        let page = self.position.rank(self.position.page) as f32;
        let in_page = match self.panel_count() {
            Some(n) if !whole_page => (self.position.panel.min(n - 1) + 1) as f32 / n as f32,
            _ => 1.0,
        };
        Some(((page + in_page) / count).clamp(0.0, 1.0))
    }

    /// Où la caméra doit aller : la case lue, ou la planche entière.
    fn target(&self, page: &Page, whole_page: bool) -> Rect {
        if whole_page {
            page.whole()
        } else {
            page.panels[self.position.panel.min(page.panels.len() - 1)]
        }
    }

    /// Où est la caméra en ce moment (en pixels de la page).
    fn camera(&mut self, now: f64, whole_page: bool) -> Option<Rect> {
        let page = self.pages.get(&self.position.page)?;
        let target = self.target(page, whole_page);
        let Some(m) = &mut self.camera_move else {
            return Some(target);
        };
        let start = *m.start.get_or_insert(now);
        let t = ((now - start) / m.secs).clamp(0.0, 1.0) as f32;
        let from = match m.from {
            From::Rect(r) => r,
            From::WholePage => page.whole(),
        };
        if t >= 1.0 {
            self.camera_move = None;
            return Some(target);
        }
        Some(lerp_rect(from, target, ease(t)))
    }

    fn glide(&mut self, from: From, secs: f64) {
        self.camera_move = Some(Move {
            from,
            start: None,
            secs,
        });
    }

    /// Arrive sur la page `page` (indice dans le PDF).
    fn enter(&mut self, page: usize, to_last_panel: bool, whole_page: bool) {
        self.position = Position {
            page,
            panel: 0,
            ..self.position
        };
        self.to_last_panel = to_last_panel;
        self.camera_move = if whole_page {
            None
        } else {
            Some(Move {
                from: From::WholePage,
                start: None,
                secs: ENTER_SECS,
            })
        };
    }

    fn next(&mut self, now: f64, whole_page: bool) {
        let (Some(count), Some(panels)) = (self.page_count, self.panel_count()) else {
            return;
        };
        let here = self.camera(now, whole_page);
        let rank = self.position.rank(self.position.page);
        if !whole_page && self.position.panel + 1 < panels {
            self.position.panel += 1;
            if let Some(here) = here {
                self.glide(From::Rect(here), MOVE_SECS);
            }
        } else if rank + 1 < count {
            self.enter(self.position.rank(rank + 1), false, whole_page);
        }
    }

    fn previous(&mut self, now: f64, whole_page: bool) {
        let here = self.camera(now, whole_page);
        let rank = self.position.rank(self.position.page);
        if !whole_page && self.position.panel > 0 && self.current().is_some() {
            self.position.panel -= 1;
            if let Some(here) = here {
                self.glide(From::Rect(here), MOVE_SECS);
            }
        } else if rank > 0 {
            self.enter(self.position.rank(rank - 1), !whole_page, whole_page);
        }
    }

    fn go_to_page(&mut self, page: usize, whole_page: bool) {
        let Some(count) = self.page_count else { return };
        self.enter(page.min(count.saturating_sub(1)), false, whole_page);
    }

    /// Lecture en manga ou en BD. Les cases de la planche sont redétectées dans
    /// le nouvel ordre.
    fn set_manga(&mut self, on: bool, now: f64, whole_page: bool) {
        let here = self.camera(now, whole_page);
        let at_start = self.position.set_manga(on);
        self.loader.right_to_left(on);
        if at_start {
            self.enter(self.position.page, false, whole_page);
        } else {
            self.position.panel = 0;
            if let Some(here) = here {
                self.glide(From::Rect(here), MOVE_SECS);
            }
        }
    }
}

fn upload(ctx: &egui::Context, page: LoadedPage) -> Page {
    let LoadedPage {
        index,
        image,
        panels,
    } = page;
    let size = vec2(image.width as f32, image.height as f32);
    let color = egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
    let options = TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..TextureOptions::LINEAR
    };
    let texture = ctx.load_texture(format!("page-{index}"), color, options);
    // Un peu d'air autour de chaque case, pour voir son cadre.
    let air = size.y * 0.008;
    let whole = Rect::from_min_size(Pos2::ZERO, size);
    let panels = panels
        .iter()
        .map(|r| {
            Rect::from_min_max(
                pos2(r.x0 as f32, r.y0 as f32),
                pos2(r.x1 as f32, r.y1 as f32),
            )
            .expand(air)
            .intersect(whole)
        })
        .collect();
    Page {
        texture,
        size,
        panels,
    }
}

fn ease(t: f32) -> f32 {
    // Départ et arrivée en douceur.
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        // Un PDF glissé dans la fenêtre.
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .find(|p| is_pdf(p))
        });
        if let Some(path) = dropped {
            self.open(&ctx, path);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::O)) {
            self.pick_file(&ctx);
        }

        self.infos.receive();
        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0.0, BACKGROUND);
        if self.book.is_some() {
            self.reading(ui, &ctx, area, now);
        } else {
            self.welcome(ui, &ctx, area, now);
        }

        if let Some(shots) = &mut self.shots {
            let idle = self
                .book
                .as_ref()
                .is_none_or(|b| b.camera_move.is_none() && b.current().is_some());
            self.auto_next = shots.frame(&ctx, now, idle);
        }

        // Position enregistrée une seconde après le dernier mouvement.
        if let Some(since) = self.state_dirty_since {
            if now - since > 1.0 {
                self.save_position();
            } else {
                ctx.request_repaint_after(Duration::from_millis(1100));
            }
        }
    }

    fn on_exit(&mut self) {
        self.save_position();
    }
}

impl App {
    fn welcome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, area: Rect, now: f64) {
        self.library.receive(ctx);
        // Les dossiers relus de temps en temps : un album ajouté y apparaît seul.
        if self.books_at.is_none_or(|at| now - at > 5.0) {
            self.books = library::books(&self.state.recents, &self.state.folders);
            self.books_at = Some(now);
        }
        let pad = if area.width() > 700.0 { 44.0 } else { 18.0 };
        let column = Rect::from_min_max(area.min + vec2(pad, 0.0), area.max - vec2(pad, 0.0));
        let mut clicks = Clicks::default();
        ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(30.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Serj")
                                    .size(32.0)
                                    .strong()
                                    .color(ACCENT),
                            );
                            ui.label(
                                egui::RichText::new("Tes BD, case par case")
                                    .size(14.0)
                                    .color(MUTED),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let compact = ui.available_width() < 380.0;
                            let label = |full: &'static str| if compact { "" } else { full };
                            clicks.add_folder |= widgets::button(
                                ui,
                                Some(Icon::Folder),
                                label("Ajouter un dossier"),
                                Tone::Plain,
                            )
                            .on_hover_text("Montrer ici tous les PDF d'un dossier")
                            .clicked();
                            clicks.pick |= widgets::button(
                                ui,
                                Some(Icon::Plus),
                                label("Ouvrir un PDF"),
                                Tone::Primary,
                            )
                            .on_hover_text("⌘ O, ou glisse un PDF dans la fenêtre")
                            .clicked();
                        });
                    });

                    if !self.state.folders.is_empty() {
                        ui.add_space(16.0);
                        self.folder_chips(ui, &mut clicks);
                    }

                    if self.books.is_empty() {
                        empty_library(ui);
                        return;
                    }
                    // Le nom du fichier tout de suite, ce qu'on trouve en ligne ensuite.
                    let books = self.books.clone();
                    let infos: HashMap<PathBuf, Info> = books
                        .iter()
                        .map(|p| (p.clone(), self.info(p).clone()))
                        .collect();
                    let (reading, rest): (Vec<PathBuf>, Vec<PathBuf>) =
                        books.into_iter().partition(|p| {
                            let pos = self.state.position(p);
                            pos.pages > 0 && pos.progress < 0.999
                        });
                    if !reading.is_empty() {
                        let cards: Vec<(PathBuf, String)> = reading
                            .iter()
                            .map(|p| (p.clone(), full_label(&infos[p], p)))
                            .collect();
                        self.shelf(ui, "En cours", &cards, &mut clicks);
                    }
                    for (name, cards) in by_series(&rest, &infos) {
                        self.shelf(ui, &name, &cards, &mut clicks);
                    }
                    ui.add_space(20.0);
                });
        });

        if clicks.pick {
            self.pick_file(ctx);
        }
        if clicks.add_folder
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            if !self.state.folders.contains(&folder) {
                self.state.folders.push(folder);
            }
            self.state.save();
            self.books_at = None;
        }
        if let Some(folder) = clicks.remove_folder {
            self.state.folders.retain(|f| *f != folder);
            self.state.save();
            self.books_at = None;
        }
        if let Some(path) = clicks.forget {
            self.state.recents.retain(|p| *p != path);
            self.state.save();
            self.books_at = None;
        }
        if let Some(path) = clicks.restart {
            let manga = self.state.position(&path).manga;
            self.state.set_position(
                &path,
                Position {
                    manga,
                    ..Position::default()
                },
            );
            self.state.save();
        }
        if let Some(path) = clicks.manga {
            let mut pos = self.state.position(&path);
            pos.set_manga(!pos.manga);
            self.state.set_position(&path, pos);
            self.state.save();
        }
        if let Some(path) = clicks.infos {
            self.sheet = Some(Sheet { path, draft: None });
        }
        if let Some(path) = clicks.open {
            self.sheet = None;
            self.open(ctx, path);
        }
        self.sheet(ctx);
    }

    /// Les dossiers de la bibliothèque, en pastilles qu'on peut retirer.
    fn folder_chips(&self, ui: &mut egui::Ui, clicks: &mut Clicks) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for folder in &self.state.folders {
                let name = folder
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| folder.display().to_string());
                egui::Frame::new()
                    .fill(SURFACE)
                    .corner_radius(14)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 4,
                        top: 3,
                        bottom: 3,
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.label(egui::RichText::new(name).size(12.5).color(TEXT))
                            .on_hover_text(folder.display().to_string());
                        let remove = ui
                            .add(
                                egui::Button::new(egui::RichText::new("×").size(14.0).color(MUTED))
                                    .frame(false),
                            )
                            .on_hover_text("Retirer ce dossier de la bibliothèque");
                        if remove.clicked() {
                            clicks.remove_folder = Some(folder.clone());
                        }
                    });
            }
        });
    }

    /// Une rangée de la bibliothèque : un titre, puis la grille des albums.
    fn shelf(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        books: &[(PathBuf, String)],
        clicks: &mut Clicks,
    ) {
        ui.add_space(30.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).size(18.0).strong().color(TEXT));
            ui.label(
                egui::RichText::new(books.len().to_string())
                    .size(13.0)
                    .color(MUTED),
            );
        });
        ui.add_space(14.0);

        // Les cartes s'élargissent pour remplir la rangée.
        let (min_w, gap) = (150.0_f32, 26.0_f32);
        let width = ui.available_width();
        let cols = (((width + gap) / (min_w + gap)).floor() as usize).max(1);
        let card_w = ((width - (cols - 1) as f32 * gap) / cols as f32).min(220.0);
        for row in books.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (path, label) in row {
                    let (card, infos) = self.book_card(ui, path, label, card_w);
                    if infos {
                        clicks.infos = Some(path.clone());
                    } else if card.clicked() {
                        clicks.open = Some(path.clone());
                    }
                    let recent = self.state.recents.contains(path);
                    let pos = self.state.position(path);
                    card.context_menu(|ui| {
                        if ui.button("Lire").clicked() {
                            clicks.open = Some(path.clone());
                        }
                        if ui.button("Infos sur l'album").clicked() {
                            clicks.infos = Some(path.clone());
                        }
                        let manga = if pos.manga {
                            "Lire comme une BD"
                        } else {
                            "Lire comme un manga"
                        };
                        if ui.button(manga).clicked() {
                            clicks.manga = Some(path.clone());
                        }
                        if pos.pages > 0 && ui.button("Recommencer au début").clicked() {
                            clicks.restart = Some(path.clone());
                        }
                        if recent && ui.button("Retirer des récents").clicked() {
                            clicks.forget = Some(path.clone());
                        }
                    });
                }
            });
            ui.add_space(gap);
        }
    }

    /// Une carte de la bibliothèque : la couverture, le titre, où on en est.
    /// Renvoie aussi si l'on a cliqué sur son bouton « i » (la fiche).
    fn book_card(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        label: &str,
        width: f32,
    ) -> (egui::Response, bool) {
        let cover_h = width * 1.42;
        let (rect, response) = ui.allocate_exact_size(vec2(width, cover_h + 58.0), Sense::click());
        // Survol de toute la carte, bouton « i » compris.
        let hovered = ui.rect_contains_pointer(rect);
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let lift = ui.ctx().animate_bool_with_time(response.id, hovered, 0.12);
        let painter = ui.painter_at(rect.expand(20.0));
        let slot = Rect::from_min_size(rect.min - vec2(0.0, 5.0 * lift), vec2(width, cover_h));
        let cover_rect = match self.library.cover(path) {
            library::Cover::Ready(texture) => {
                // La couverture entière, posée en bas de sa place.
                let size = texture.size_vec2();
                let scale = (slot.width() / size.x).min(slot.height() / size.y);
                let fitted = size * scale;
                let r = Rect::from_min_size(
                    pos2(slot.center().x - fitted.x / 2.0, slot.bottom() - fitted.y),
                    fitted,
                );
                let shadow = egui::epaint::Shadow {
                    offset: [0, 6 + (6.0 * lift) as i8],
                    blur: 18 + (10.0 * lift) as u8,
                    spread: 0,
                    color: Color32::from_black_alpha(150),
                };
                painter.add(shadow.as_shape(r, 8));
                egui::Image::from_texture((texture.id(), size))
                    .corner_radius(8.0)
                    .paint_at(ui, r);
                r
            }
            cover => {
                let failed = matches!(cover, library::Cover::Failed);
                painter.rect_filled(slot, 8.0, SURFACE);
                let text = if failed { "PDF illisible" } else { "…" };
                painter.text(
                    slot.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(14.0),
                    MUTED,
                );
                slot
            }
        };
        if lift > 0.0 {
            painter.rect_stroke(
                cover_rect,
                8.0,
                egui::Stroke::new(2.0, ACCENT.gamma_multiply(lift)),
                egui::StrokeKind::Outside,
            );
        }

        // Où on en est : une étiquette, ou une barre au pied de la couverture.
        let pos = self.state.position(path);
        let started = pos.pages > 0;
        let finished = started && pos.progress >= 0.999;
        let mut at = cover_rect.min + vec2(8.0, 8.0);
        if !started {
            at.x = widgets::badge(&painter, at, "Nouveau", ACCENT, ON_ACCENT).right() + 6.0;
        } else if finished {
            let fill = Color32::from_black_alpha(200);
            at.x = widgets::badge(&painter, at, "Terminé", fill, TEXT).right() + 6.0;
        }
        if pos.manga {
            widgets::badge(&painter, at, "Manga", Color32::from_black_alpha(200), TEXT);
        }
        if started && !finished {
            let track = Rect::from_min_max(
                pos2(cover_rect.left() + 8.0, cover_rect.bottom() - 12.0),
                pos2(cover_rect.right() - 8.0, cover_rect.bottom() - 8.0),
            );
            let mut done = track;
            done.max.x = track.left() + track.width() * pos.progress.clamp(0.0, 1.0);
            painter.rect_filled(track.expand(1.0), 3.0, Color32::from_black_alpha(170));
            painter.rect_filled(done, 2.0, ACCENT);
        }

        // Le titre, sur deux lignes au plus.
        let mut job = egui::text::LayoutJob::simple(
            label.to_owned(),
            FontId::proportional(13.5),
            if hovered {
                Color32::WHITE
            } else {
                Color32::from_gray(220)
            },
            width,
        );
        job.wrap.max_rows = 2;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let title_h = galley.size().y;
        painter.galley(pos2(rect.left(), slot.bottom() + 10.0), galley, TEXT);
        if started && !finished {
            painter.text(
                pos2(rect.left(), slot.bottom() + 14.0 + title_h),
                Align2::LEFT_TOP,
                format!(
                    "Page {} / {}   ·   {}",
                    pos.rank(pos.page) + 1,
                    pos.pages,
                    percent(pos.progress)
                ),
                FontId::proportional(11.5),
                MUTED,
            );
        }

        // Le bouton « i » de la fiche, au survol.
        let mut infos = false;
        if lift > 0.0 {
            let c = pos2(cover_rect.right() - 20.0, cover_rect.top() + 20.0);
            let button = ui.interact(
                Rect::from_center_size(c, vec2(30.0, 30.0)),
                response.id.with("fiche"),
                Sense::click(),
            );
            let alpha = if button.hovered() { 240 } else { 190 };
            painter.circle_filled(c, 15.0, Color32::from_black_alpha(alpha));
            painter.circle_stroke(
                c,
                15.0,
                egui::Stroke::new(1.0, Color32::from_white_alpha(40)),
            );
            painter.text(
                c,
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(17.0),
                TEXT.gamma_multiply(lift),
            );
            if button.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            infos = button.on_hover_text("Infos sur l'album").clicked();
        }
        (response.on_hover_text(path.display().to_string()), infos)
    }

    /// La fiche de l'album : auteurs, éditeur, résumés, sources ; et de quoi
    /// corriger ce qui a servi à la recherche.
    fn sheet(&mut self, ctx: &egui::Context) {
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        let path = sheet.path.clone();
        let mut draft = sheet.draft.take();
        let info = self.info(&path).clone();
        let pending = self.infos.pending(&path);
        let cover = match self.library.cover(&path) {
            library::Cover::Ready(t) => Some(t.clone()),
            _ => None,
        };
        let width = (ctx.content_rect().width() - 96.0).clamp(300.0, 760.0);
        enum Do {
            Nothing,
            Read,
            Edit,
            Search(Guess),
            Forget,
            Again,
        }
        let mut action = Do::Nothing;
        let frame = egui::Frame::new()
            .fill(Color32::from_rgb(24, 24, 29))
            .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(18)))
            .corner_radius(16)
            .inner_margin(28);
        let modal = egui::Modal::new(egui::Id::new("fiche"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(170))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal_top(|ui| {
                    if let Some(t) = &cover {
                        let size = t.size_vec2();
                        let w = (width * 0.26).min(180.0);
                        ui.add(
                            egui::Image::from_texture((t.id(), vec2(w, w * size.y / size.x)))
                                .corner_radius(8.0),
                        );
                        ui.add_space(22.0);
                    }
                    ui.vertical(|ui| {
                        if let Some(series) = &info.series {
                            let text = match info.volume {
                                Some(v) => format!("{series}  ·  Tome {v}"),
                                None => series.clone(),
                            };
                            ui.label(egui::RichText::new(text).size(13.0).strong().color(ACCENT));
                        }
                        let name = info.title.clone().unwrap_or_else(|| title(&path));
                        ui.label(egui::RichText::new(name).size(24.0).strong().color(TEXT));
                        ui.add_space(6.0);
                        if !info.authors.is_empty() {
                            ui.label(egui::RichText::new(info.credits()).size(14.0).color(TEXT));
                        }
                        let facts: Vec<String> = [
                            info.publisher.clone(),
                            info.year.map(|y| y.to_string()),
                            info.pages.map(|p| format!("{p} pages")),
                            info.isbn.as_ref().map(|i| format!("ISBN {i}")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        if !facts.is_empty() {
                            ui.label(egui::RichText::new(facts.join("   ·   ")).color(MUTED));
                        }
                        if !info.genres.is_empty() {
                            ui.label(
                                egui::RichText::new(info.genres.join(", "))
                                    .size(12.5)
                                    .color(MUTED),
                            );
                        }
                        ui.add_space(8.0);
                        if pending {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new("Recherche en ligne…").color(MUTED));
                            });
                        } else if !info.searched {
                            ui.label(
                                egui::RichText::new(
                                    "Hors ligne : infos tirées du nom du fichier. \
                                     Nouvel essai au prochain lancement.",
                                )
                                .color(MUTED),
                            );
                        } else if info.sources.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Rien trouvé en ligne. Corrige la série ou le titre \
                                     pour aider la recherche.",
                                )
                                .color(MUTED),
                            );
                        }
                        ui.add_space(14.0);
                        if draft.is_none() {
                            ui.horizontal(|ui| {
                                if widgets::button(ui, Some(Icon::Play), "Lire", Tone::Primary)
                                    .clicked()
                                {
                                    action = Do::Read;
                                }
                                if widgets::button(ui, None, "Corriger", Tone::Plain)
                                    .on_hover_text("Série, tome, titre ou auteurs mal reconnus")
                                    .clicked()
                                {
                                    action = Do::Edit;
                                }
                                if widgets::button(ui, None, "Chercher à nouveau", Tone::Plain)
                                    .clicked()
                                {
                                    action = Do::Again;
                                }
                            });
                        }
                    });
                });

                ui.add_space(20.0);
                if let Some(d) = &mut draft {
                    ui.label(
                        egui::RichText::new("Corriger les infos")
                            .size(16.0)
                            .strong()
                            .color(TEXT),
                    );
                    ui.label(
                        egui::RichText::new("La recherche en ligne repartira de ce que tu indiques.")
                            .color(MUTED),
                    );
                    ui.add_space(10.0);
                    egui::Grid::new("correction")
                        .num_columns(2)
                        .spacing([14.0, 10.0])
                        .show(ui, |ui| {
                            for (name, value, hint) in [
                                ("Série", &mut d.series, "Lanfeust de Troy"),
                                ("Tome", &mut d.volume, "2"),
                                ("Titre", &mut d.title, "Thanos l'incongru"),
                                ("Auteurs", &mut d.authors, "séparés par des virgules"),
                            ] {
                                ui.label(egui::RichText::new(name).color(MUTED));
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .hint_text(hint)
                                        .desired_width(width * 0.6),
                                );
                                ui.end_row();
                            }
                        });
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, None, "Chercher", Tone::Primary).clicked() {
                            action = Do::Search(d.guess());
                        }
                        if widgets::button(ui, None, "Annuler", Tone::Plain).clicked() {
                            action = Do::Edit;
                        }
                        if widgets::button(ui, None, "Revenir au nom du fichier", Tone::Plain)
                            .clicked()
                        {
                            action = Do::Forget;
                        }
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 420.0).max(160.0))
                        .show(ui, |ui| {
                            let none = if info.retry_at.is_some() {
                                "Pas encore de résumé : le service était saturé, nouvel essai demain."
                            } else if pending {
                                "…"
                            } else {
                                "Pas de résumé trouvé pour cet album."
                            };
                            prose(ui, "Résumé", info.summary.as_deref().unwrap_or(none));
                            if let Some(text) = &info.series_summary {
                                ui.add_space(14.0);
                                prose(ui, "La série", text);
                            }
                        });
                    if !info.sources.is_empty() {
                        ui.add_space(14.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Sources :").size(12.0).color(MUTED));
                            for (name, url) in &info.sources {
                                ui.hyperlink_to(egui::RichText::new(name).size(12.0), url);
                            }
                        });
                    }
                }
            });

        let correction = self.state.correction(&path).cloned();
        let manga = self.state.position(&path).manga;
        match action {
            Do::Nothing => {}
            Do::Read => {
                self.sheet = None;
                self.open(ctx, path);
                return;
            }
            Do::Edit => {
                draft = match draft {
                    Some(_) => None,
                    None => Some(Draft::from_info(&info)),
                }
            }
            Do::Search(guess) => {
                self.state.set_correction(&path, Some(guess));
                self.state.save();
                draft = None;
            }
            Do::Forget => {
                self.state.set_correction(&path, None);
                self.state.save();
                draft = None;
            }
            Do::Again => self.infos.refresh(&path, manga, correction.as_ref()),
        }
        if modal.should_close() && draft.is_none() {
            self.sheet = None;
        } else if let Some(sheet) = &mut self.sheet {
            sheet.draft = draft;
        }
    }

    fn reading(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, area: Rect, now: f64) {
        let whole_page = self.state.whole_page;
        self.book
            .as_mut()
            .expect("album ouvert")
            .receive(ctx, whole_page);
        // Le vrai titre de l'album, dès que la recherche l'a trouvé.
        let path = self.book.as_ref().expect("album ouvert").path.clone();
        let name = self.label(&path);
        if name != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name} — Serj")));
            self.window_title = name.clone();
        }

        // Clavier, souris, molette.
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let zoomed = self.zoom > 1.0;
        let mut key_zoom = None;
        let mut input = ctx.input_mut(|i| {
            // Zoom : ⌘ + / ⌘ − / ⌘ 0, et Z pour zoomer ou revenir.
            let cmd = egui::Modifiers::COMMAND;
            if i.consume_key(cmd, Key::Plus) | i.consume_key(cmd, Key::Equals) {
                key_zoom = Some(1.5);
            }
            if i.consume_key(cmd, Key::Minus) {
                key_zoom = Some(1.0 / 1.5);
            }
            if i.consume_key(cmd, Key::Num0) {
                key_zoom = Some(0.0);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Z) {
                key_zoom = Some(if zoomed { 0.0 } else { KEY_ZOOM });
            }
            let shift_space = i.consume_key(egui::Modifiers::SHIFT, Key::Space);
            let mut any = |keys: &[Key]| {
                keys.iter().fold(false, |hit, k| {
                    i.consume_key(egui::Modifiers::NONE, *k) | hit
                })
            };
            let escape = any(&[Key::Escape]);
            // Échap dézoome d'abord.
            if escape && zoomed {
                key_zoom = Some(0.0);
            }
            let escape = escape && !zoomed;
            Actions {
                next: any(&[
                    Key::ArrowRight,
                    Key::ArrowDown,
                    Key::Space,
                    Key::PageDown,
                    Key::Enter,
                ]),
                previous: any(&[Key::ArrowLeft, Key::ArrowUp, Key::PageUp, Key::Backspace])
                    || shift_space,
                first: any(&[Key::Home]),
                last: any(&[Key::End]),
                toggle_page: any(&[Key::P]),
                toggle_manga: any(&[Key::M]),
                toggle_auto: any(&[Key::A]),
                faster: any(&[Key::Minus]),
                slower: any(&[Key::Plus, Key::Equals]),
                // Échap : sort du plein écran, sinon ferme l'album.
                fullscreen: any(&[Key::F]) || (escape && is_fullscreen),
                close: i.consume_key(egui::Modifiers::COMMAND, Key::W)
                    || (escape && !is_fullscreen),
                ..Actions::default()
            }
        });
        // Clic à droite pour avancer, à gauche pour reculer ; une fois zoomé,
        // on fait glisser la planche (à la souris ou à la molette).
        let response = ui.interact(area, ui.id().with("lecture"), Sense::click_and_drag());
        if response.clicked()
            && !zoomed
            && let Some(p) = response.interact_pointer_pos()
        {
            if p.x > area.left() + area.width() / 3.0 {
                input.next = true;
            } else {
                input.previous = true;
            }
        }
        let wheel = if zoomed {
            0.0
        } else {
            ctx.input(|i| i.smooth_scroll_delta.y)
        };
        self.scroll += wheel;
        if self.scroll.abs() > 90.0 {
            if self.scroll < 0.0 {
                input.next = true;
            } else {
                input.previous = true;
            }
            self.scroll = 0.0;
        }
        if wheel == 0.0 {
            self.scroll *= 0.8;
        }
        input.next |= std::mem::take(&mut self.auto_next);
        self.apply(ctx, input, now);
        if self.book.is_none() {
            return;
        }
        let auto_part = self.autoplay(ctx, now);
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO) {
            self.hud_until = self.hud_until.max(now + 1.0);
        }

        let whole_page = self.state.whole_page;
        let book = self.book.as_mut().expect("album ouvert");
        // La dernière case de la page précédente, une fois celle-ci rendue.
        if book.to_last_panel
            && let Some(n) = book.panel_count()
        {
            book.position.panel = n - 1;
            book.to_last_panel = false;
        }

        let painter = ui.painter_at(area);
        if let Some(error) = &book.error {
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                error,
                FontId::proportional(18.0),
                MUTED,
            );
        } else if let Some(camera) = book.camera(now, whole_page) {
            let page = book.current().expect("page rendue");
            // La caméra remplit l'écran (moins une marge), centrée.
            let room = area.shrink(SCREEN_MARGIN);
            let fit = (room.width() / camera.width()).min(room.height() / camera.height());

            // Le zoom : pincement, ⌘ molette ou clavier, vers le pointeur ;
            // puis on fait glisser la vue.
            let (pinch, pointer, scroll) =
                ctx.input(|i| (i.zoom_delta(), i.pointer.hover_pos(), i.smooth_scroll_delta));
            let factor = match key_zoom {
                Some(0.0) => 1.0 / self.zoom,
                Some(f) => f,
                None => pinch,
            };
            if factor != 1.0 {
                let anchor = match key_zoom {
                    Some(_) => area.center(),
                    None => pointer
                        .filter(|p| area.contains(*p))
                        .unwrap_or(area.center()),
                };
                let old = fit * self.zoom;
                self.zoom = (self.zoom * factor).clamp(1.0, MAX_ZOOM);
                let new = fit * self.zoom;
                // Le point de la page sous `anchor` y reste.
                let under = camera.center() + self.pan + (anchor - area.center()) / old;
                self.pan = under - (anchor - area.center()) / new - camera.center();
                self.hud_until = now + HUD_SECS;
            }
            if self.zoom > 1.0 {
                let scale = fit * self.zoom;
                self.pan -= (response.drag_delta() + scroll) / scale;
                if response.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
                // La vue reste sur la planche.
                let half = area.size() / scale / 2.0;
                let center = camera.center() + self.pan;
                let clamp = |c: f32, half: f32, size: f32| {
                    if 2.0 * half >= size {
                        size / 2.0
                    } else {
                        c.clamp(half, size - half)
                    }
                };
                let clamped = pos2(
                    clamp(center.x, half.x, page.size.x),
                    clamp(center.y, half.y, page.size.y),
                );
                self.pan = clamped - camera.center();
            } else {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            let scale = fit * self.zoom;
            let center = camera.center() + self.pan;
            let to_screen = |p: Pos2| area.center() + (p - center) * scale;
            let page_rect =
                Rect::from_min_max(to_screen(Pos2::ZERO), to_screen(page.size.to_pos2()));
            let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
            painter.image(page.texture.id(), page_rect, uv, Color32::WHITE);
            if !whole_page {
                // Le reste de la planche, assombri autour de la case.
                let hole = Rect::from_min_max(to_screen(camera.min), to_screen(camera.max));
                for r in [
                    Rect::from_min_max(area.min, pos2(area.max.x, hole.min.y)),
                    Rect::from_min_max(pos2(area.min.x, hole.max.y), area.max),
                    Rect::from_min_max(pos2(area.min.x, hole.min.y), pos2(hole.min.x, hole.max.y)),
                    Rect::from_min_max(pos2(hole.max.x, hole.min.y), pos2(area.max.x, hole.max.y)),
                ] {
                    painter.rect_filled(r, 0.0, DIM);
                }
            }
            if book.camera_move.is_some() {
                ctx.request_repaint();
            }
        } else {
            let text = if book.page_count.is_none() {
                "Ouverture…"
            } else {
                "Rendu de la page…"
            };
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(16.0),
                MUTED,
            );
        }

        if let Some(progress) = book.progress(whole_page) {
            book.position.progress = progress;
        }

        let mut actions = Actions::default();
        // La barre d'avancement, tout en bas : on y clique ou on la fait
        // glisser pour sauter à une page. Elle se remplit dans le sens de
        // lecture : de droite à gauche pour un manga.
        if let (Some(count), Some(progress)) = (book.page_count, book.progress(whole_page)) {
            let manga = book.position.manga;
            let zone = Rect::from_min_max(pos2(area.left(), area.bottom() - 18.0), area.max);
            let bar = ui.interact(zone, ui.id().with("avancement"), Sense::click_and_drag());
            let active = bar.hovered() || bar.dragged();
            let height = if active { 8.0 } else { 3.0 };
            let track = Rect::from_min_max(pos2(area.left(), area.bottom() - height), area.max);
            painter.rect_filled(track, 0.0, Color32::from_gray(if active { 60 } else { 42 }));
            let mut done = track;
            if manga {
                done.min.x = track.right() - track.width() * progress;
            } else {
                done.max.x = track.left() + track.width() * progress;
            }
            painter.rect_filled(done, 0.0, ACCENT);
            if active {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                self.hud_until = self.hud_until.max(now + 1.0);
            }
            if let Some(p) = bar.hover_pos().or(bar.interact_pointer_pos()) {
                let mut t = ((p.x - track.left()) / track.width()).clamp(0.0, 0.9999);
                if manga {
                    t = 0.9999 - t;
                }
                let rank = (t * count as f32) as usize;
                let label = painter.layout_no_wrap(
                    format!("Page {}", rank + 1),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                let x = p.x.clamp(area.left() + 40.0, area.right() - 40.0);
                let tip = Rect::from_center_size(
                    pos2(x, track.top() - 20.0),
                    label.size() + vec2(16.0, 8.0),
                );
                painter.rect_filled(tip, 6.0, Color32::from_black_alpha(220));
                painter.galley(tip.center() - label.size() / 2.0, label, Color32::WHITE);
                let target = book.position.rank(rank);
                if (bar.clicked() || bar.dragged()) && target != book.position.page {
                    actions.jump = Some(target);
                }
            }
        }

        // Les commandes et l'indication « Page · Case », un instant après
        // chaque mouvement, en fondu.
        let shown = now < self.hud_until;
        let opacity = ctx.animate_bool_with_time(ui.id().with("hud"), shown, 0.2);
        if shown {
            ctx.request_repaint_after(Duration::from_secs_f64((self.hud_until - now).max(0.05)));
        } else if is_fullscreen {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        if opacity > 0.0 {
            let book = self.book.as_ref().expect("album ouvert");
            if let Some(count) = book.page_count {
                let mut text = format!(
                    "Page {} / {count}",
                    book.position.rank(book.position.page) + 1
                );
                if !whole_page && let Some(n) = book.panel_count() {
                    text += &format!("   ·   Case {} / {n}", book.position.panel.min(n - 1) + 1);
                }
                if let Some(progress) = book.progress(whole_page) {
                    text += &format!("   ·   {}", percent(progress));
                }
                if self.zoom > 1.0 {
                    text += &format!("   ·   Zoom {} %", (self.zoom * 100.0).round() as u32);
                }
                let mut painter = painter.clone();
                painter.set_opacity(opacity);
                let galley = painter.layout_no_wrap(text, FontId::proportional(14.0), TEXT);
                let pill = Rect::from_center_size(
                    pos2(area.center().x, area.bottom() - 40.0),
                    galley.size() + vec2(32.0, 14.0),
                );
                painter.rect_filled(pill, pill.height() / 2.0, Color32::from_black_alpha(200));
                painter.galley(pill.center() - galley.size() / 2.0, galley, TEXT);
            }
            let (bar, hovered) = self.toolbar(ui, area, opacity, is_fullscreen, &name);
            if hovered {
                self.hud_until = self.hud_until.max(now + 1.0);
            }
            actions.merge(bar);
        }

        // La lecture automatique : un anneau qui se remplit jusqu'au pas suivant.
        if let Some(part) = auto_part {
            let center = pos2(area.right() - 34.0, area.bottom() - 40.0);
            widgets::ring(&painter, center, 11.0, part, ACCENT);
            widgets::icon(&painter, Icon::Pause, center, 11.0, TEXT);
        }

        self.apply(ctx, actions, now);
    }

    /// La barre d'outils en haut : retour à l'accueil, titre, modes de lecture,
    /// lecture automatique, plein écran. Renvoie ce qui a été cliqué, et si la
    /// souris est dessus.
    fn toolbar(
        &self,
        ui: &mut egui::Ui,
        area: Rect,
        opacity: f32,
        fullscreen: bool,
        name: &str,
    ) -> (Actions, bool) {
        let book = self.book.as_ref().expect("album ouvert");
        let mut a = Actions::default();
        let mut painter = ui.painter_at(area);
        painter.set_opacity(opacity);
        let band = Rect::from_min_size(area.min, vec2(area.width(), 110.0));
        widgets::shade(
            &painter,
            band,
            Color32::from_black_alpha(230),
            Color32::TRANSPARENT,
        );
        let inner = Rect::from_min_max(
            area.min + vec2(14.0, 14.0),
            pos2(area.right() - 14.0, area.top() + 48.0),
        );
        let compact = area.width() < 820.0;
        let label = |full: &'static str| if compact { "" } else { full };
        let tone = |on: bool| if on { Tone::Active } else { Tone::Plain };
        let whole_page = self.state.whole_page;
        let manga = book.position.manga;
        let mut hovered = false;

        let mut right_edge = inner.right();
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
                a.fullscreen = widgets::button(ui, Some(icon), "", Tone::Plain)
                    .on_hover_text("Plein écran (F)")
                    .clicked();
                ui.add_space(10.0);
                a.toggle_manga = widgets::button(
                    ui,
                    Some(Icon::RightToLeft),
                    label("Manga"),
                    tone(manga),
                )
                .on_hover_text(
                    "Manga : de la dernière page à la première, cases de droite à gauche (M)",
                )
                .clicked();
                ui.add_space(10.0);
                let page = widgets::button(ui, Some(Icon::Page), label("Page"), tone(whole_page))
                    .on_hover_text("Page entière (P)");
                let panels =
                    widgets::button(ui, Some(Icon::Panels), label("Cases"), tone(!whole_page))
                        .on_hover_text("Case par case (P)");
                a.toggle_page = (page.clicked() && !whole_page) || (panels.clicked() && whole_page);
                ui.add_space(10.0);
                a.slower = widgets::button(ui, Some(Icon::Plus), "", Tone::Plain)
                    .on_hover_text("Plus de temps par case (+)")
                    .clicked();
                let (icon, tip) = if self.auto_play {
                    (Icon::Pause, "Arrêter la lecture automatique (A)")
                } else {
                    (Icon::Play, "Lecture automatique (A)")
                };
                a.toggle_auto = widgets::button(
                    ui,
                    Some(icon),
                    &format!("{} s", self.state.auto_secs),
                    tone(self.auto_play),
                )
                .on_hover_text(tip)
                .clicked();
                a.faster = widgets::button(ui, Some(Icon::Minus), "", Tone::Plain)
                    .on_hover_text("Moins de temps par case (−)")
                    .clicked();
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
                a.close = widgets::button(ui, Some(Icon::Back), label("Accueil"), Tone::Plain)
                    .on_hover_text("Revenir à la bibliothèque (Échap)")
                    .clicked();
                hovered |= ui.rect_contains_pointer(ui.min_rect());
                let left = ui.min_rect().right() + 16.0;
                let room = right_edge - left - 16.0;
                if room > 60.0 {
                    let mut job = egui::text::LayoutJob::simple(
                        name.to_owned(),
                        FontId::proportional(14.0),
                        Color32::WHITE,
                        room,
                    );
                    job.wrap.max_rows = 1;
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    painter.galley(
                        pos2(left, inner.center().y - galley.size().y / 2.0),
                        galley,
                        TEXT,
                    );
                }
            },
        );
        (a, hovered)
    }

    /// Fait avancer la lecture automatique. Renvoie la part écoulée avant le
    /// pas suivant (de 0 à 1), si elle est en cours.
    fn autoplay(&mut self, ctx: &egui::Context, now: f64) -> Option<f32> {
        if !self.auto_play {
            return None;
        }
        let book = self.book.as_ref()?;
        // Le temps ne compte qu'une fois la case arrivée à l'écran.
        // Pas de pas automatique pendant qu'on regarde un détail de près.
        let idle = self.zoom <= 1.0
            && book.error.is_none()
            && book.camera_move.is_none()
            && book.current().is_some()
            && !book.to_last_panel;
        if !idle {
            return Some(0.0);
        }
        let since = *self.auto_since.get_or_insert(now);
        let part = ((now - since) / self.state.auto_secs) as f32;
        if part < 1.0 {
            ctx.request_repaint_after(Duration::from_millis(33));
            return Some(part);
        }
        let before = book.position;
        let step = Actions {
            next: true,
            auto: true,
            ..Actions::default()
        };
        self.apply(ctx, step, now);
        self.auto_since = None;
        let book = self.book.as_ref()?;
        if book.position.page == before.page && book.position.panel == before.panel {
            // Fin de l'album.
            self.auto_play = false;
            self.hud_until = now + HUD_SECS;
            return None;
        }
        Some(0.0)
    }

    fn apply(&mut self, ctx: &egui::Context, a: Actions, now: f64) {
        let whole_page = self.state.whole_page;
        let Some(book) = self.book.as_mut() else {
            return;
        };
        let before = book.position;
        if a.next {
            book.next(now, whole_page);
        }
        if a.previous {
            book.previous(now, whole_page);
        }
        if a.first {
            book.go_to_page(book.position.rank(0), whole_page);
        }
        if a.last
            && let Some(count) = book.page_count
        {
            book.go_to_page(book.position.rank(count.saturating_sub(1)), whole_page);
        }
        if let Some(page) = a.jump {
            book.go_to_page(page, whole_page);
        }
        if a.toggle_page {
            self.state.whole_page = !whole_page;
            if let Some(here) = book.camera(now, whole_page) {
                book.glide(From::Rect(here), MOVE_SECS);
            }
        }
        if a.toggle_manga {
            book.set_manga(!book.position.manga, now, self.state.whole_page);
        }
        let moved = book.position.page != before.page || book.position.panel != before.panel;
        if moved {
            self.auto_since = None;
        }
        if a.toggle_auto {
            self.auto_play = !self.auto_play;
            self.auto_since = None;
        }
        if a.faster || a.slower {
            let secs = self.state.auto_secs;
            self.state.auto_secs = if a.slower {
                AUTO_STEPS
                    .iter()
                    .copied()
                    .find(|&s| s > secs)
                    .unwrap_or(secs)
            } else {
                AUTO_STEPS
                    .iter()
                    .rev()
                    .copied()
                    .find(|&s| s < secs)
                    .unwrap_or(secs)
            };
        }
        if moved || a.toggle_page || a.toggle_manga {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        let changed = moved || a.toggle_page || a.toggle_manga || a.faster || a.slower;
        if changed {
            self.state_dirty_since.get_or_insert(now);
        }
        if (changed || a.toggle_auto) && !a.auto {
            self.hud_until = now + HUD_SECS;
        }
        if a.fullscreen {
            let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
        }
        if a.close {
            self.close(ctx);
        }
    }
}

impl Actions {
    fn merge(&mut self, other: Actions) {
        self.next |= other.next;
        self.previous |= other.previous;
        self.first |= other.first;
        self.last |= other.last;
        self.jump = self.jump.or(other.jump);
        self.toggle_page |= other.toggle_page;
        self.toggle_manga |= other.toggle_manga;
        self.toggle_auto |= other.toggle_auto;
        self.faster |= other.faster;
        self.slower |= other.slower;
        self.fullscreen |= other.fullscreen;
        self.close |= other.close;
    }
}

/// L'accueil sans aucun album.
fn empty_library(ui: &mut egui::Ui) {
    ui.add_space(90.0);
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 36.0, SURFACE);
        widgets::icon(ui.painter(), Icon::Folder, r.center(), 30.0, MUTED);
        ui.add_space(18.0);
        ui.label(
            egui::RichText::new("Ta bibliothèque est vide")
                .size(20.0)
                .strong()
                .color(TEXT),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Ouvre un PDF, glisse-le dans la fenêtre, ou ajoute le dossier où sont tes BD.",
            )
            .color(MUTED),
        );
    });
}

impl Draft {
    fn from_info(info: &Info) -> Self {
        Self {
            series: info.series.clone().unwrap_or_default(),
            volume: info.volume.map(|v| v.to_string()).unwrap_or_default(),
            title: info.title.clone().unwrap_or_default(),
            authors: info
                .authors
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    fn guess(&self) -> Guess {
        let text = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        Guess {
            series: text(&self.series),
            volume: self.volume.trim().parse().ok(),
            title: text(&self.title),
            authors: self.authors.split(',').filter_map(text).collect(),
        }
    }
}

/// Un paragraphe de la fiche, sous son intitulé.
fn prose(ui: &mut egui::Ui, heading: &str, text: &str) {
    ui.label(egui::RichText::new(heading).size(15.0).strong().color(TEXT));
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text)
            .size(14.0)
            .color(Color32::from_gray(205)),
    );
}

/// Les albums rangés par série (celles d'au moins deux albums, par nom, dans
/// l'ordre des tomes), puis les autres. Avec le libellé de chaque carte.
fn by_series(
    books: &[PathBuf],
    infos: &HashMap<PathBuf, Info>,
) -> Vec<(String, Vec<(PathBuf, String)>)> {
    let mut series: BTreeMap<String, (String, Vec<&PathBuf>)> = BTreeMap::new();
    for p in books {
        if let Some(name) = &infos[p].series {
            series
                .entry(meta::normalize(name))
                .or_insert_with(|| (name.clone(), Vec::new()))
                .1
                .push(p);
        }
    }
    let mut shelves = Vec::new();
    let mut others: Vec<&PathBuf> = books
        .iter()
        .filter(|p| infos[*p].series.is_none())
        .collect();
    for (_, (name, mut list)) in series {
        if list.len() < 2 {
            others.extend(list);
            continue;
        }
        list.sort_by_key(|p| {
            (
                infos[*p].volume.unwrap_or(u32::MAX),
                full_label(&infos[*p], p),
            )
        });
        let cards = list
            .iter()
            .map(|p| {
                let label = infos[*p].short_title().unwrap_or_else(|| title(p));
                ((*p).clone(), label)
            })
            .collect();
        shelves.push((name, cards));
    }
    if !others.is_empty() {
        others.sort_by_key(|p| meta::normalize(&full_label(&infos[*p], p)));
        let name = if shelves.is_empty() {
            "Bibliothèque"
        } else {
            "Autres albums"
        };
        let cards = others
            .iter()
            .map(|p| ((*p).clone(), full_label(&infos[*p], p)))
            .collect();
        shelves.push((name.to_string(), cards));
    }
    shelves
}

fn full_label(info: &Info, path: &Path) -> String {
    info.full_title().unwrap_or_else(|| title(path))
}

/// Le nom de l'album : celui du fichier, sans extension.
fn title(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Album".into())
}

fn percent(progress: f32) -> String {
    format!("{} %", (progress * 100.0).floor() as u32)
}

pub fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}
