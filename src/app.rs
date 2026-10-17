//! La fenêtre de Serj : l'accueil, puis la lecture, où une caméra glisse de
//! case en case et assombrit le reste de la planche.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{
    Align2, Color32, FontId, Key, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2, pos2,
    vec2,
};

use crate::loader::{Event, LoadedPage, Loader};
use crate::state::{Position, State};

const BACKGROUND: Color32 = Color32::from_rgb(18, 18, 20);
/// Le reste de la planche, autour de la case lue : à peine visible.
const DIM: Color32 = Color32::from_rgba_premultiplied(18, 18, 20, 232);
const ACCENT: Color32 = Color32::from_rgb(240, 196, 92);
const MUTED: Color32 = Color32::from_rgb(140, 140, 150);

/// Durée du glissement d'une case à l'autre, et du zoom en arrivant sur une page.
const MOVE_SECS: f64 = 0.38;
const ENTER_SECS: f64 = 0.55;
/// Marge autour de la case à l'écran, en points.
const SCREEN_MARGIN: f32 = 20.0;
/// Pages gardées de part et d'autre de la page lue.
const KEEP_AROUND: usize = 2;
/// Combien de temps l'indication « Page · Case » reste visible.
const HUD_SECS: f64 = 2.2;

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

pub struct App {
    state: State,
    book: Option<Book>,
    hud_until: f64,
    scroll: f32,
    state_dirty_since: Option<f64>,
    shots: Option<crate::shots::Shots>,
    auto_next: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = BACKGROUND;
        cc.egui_ctx.set_visuals(visuals);

        let mut app = Self {
            state: State::load(),
            book: None,
            hud_until: 0.0,
            scroll: 0.0,
            state_dirty_since: None,
            shots: crate::shots::Shots::from_env(),
            auto_next: false,
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
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Album".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{title} — Serj")));
        self.state.opened(&path);
        self.state.save();
        let position = self.state.position(&path);
        self.book = Some(Book {
            loader: Loader::open(path.clone(), ctx.clone()),
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
        self.hud_until = ctx.input(|i| i.time) + HUD_SECS;
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.save_position();
        self.book = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Serj".into()));
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
            self.state.save();
        }
        self.state_dirty_since = None;
    }
}

impl Book {
    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.loader.events.try_recv() {
            match event {
                Event::Opened { pages } => {
                    self.page_count = Some(pages);
                    self.position.pages = pages;
                    if self.position.page >= pages {
                        self.position.page = 0;
                        self.position.panel = 0;
                    }
                }
                Event::Failed(e) => self.error = Some(e),
                Event::Page(page) => {
                    let index = page.index;
                    let page = upload(ctx, page);
                    self.pages.insert(index, page);
                }
            }
        }
        if let Some(count) = self.page_count {
            // La page lue d'abord, puis la suivante, puis la précédente, etc.
            let p = self.position.page;
            let mut wanted = vec![p];
            for d in 1..=KEEP_AROUND {
                if p + d < count {
                    wanted.push(p + d);
                }
                if let Some(back) = p.checked_sub(d) {
                    wanted.push(back);
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
        let page = self.position.page as f32;
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

    fn next(&mut self, now: f64, whole_page: bool) {
        let (Some(count), Some(panels)) = (self.page_count, self.panel_count()) else {
            return;
        };
        let here = self.camera(now, whole_page);
        if !whole_page && self.position.panel + 1 < panels {
            self.position.panel += 1;
            if let Some(here) = here {
                self.glide(From::Rect(here), MOVE_SECS);
            }
        } else if self.position.page + 1 < count {
            self.position = Position {
                page: self.position.page + 1,
                panel: 0,
                ..self.position
            };
            self.to_last_panel = false;
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
    }

    fn previous(&mut self, now: f64, whole_page: bool) {
        let here = self.camera(now, whole_page);
        if !whole_page && self.position.panel > 0 && self.current().is_some() {
            self.position.panel -= 1;
            if let Some(here) = here {
                self.glide(From::Rect(here), MOVE_SECS);
            }
        } else if self.position.page > 0 {
            self.position = Position {
                page: self.position.page - 1,
                panel: 0,
                ..self.position
            };
            self.to_last_panel = !whole_page;
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
    }

    fn go_to_page(&mut self, page: usize, whole_page: bool) {
        let Some(count) = self.page_count else { return };
        self.position = Position {
            page: page.min(count.saturating_sub(1)),
            panel: 0,
            ..self.position
        };
        self.to_last_panel = false;
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

        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0.0, BACKGROUND);
        if self.book.is_some() {
            self.reading(ui, &ctx, area, now);
        } else {
            self.welcome(ui, &ctx, area);
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
    fn welcome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, area: Rect) {
        let width = 420.0_f32.min(area.width() - 32.0);
        let column = Rect::from_center_size(area.center(), vec2(width, area.height() - 64.0));
        let mut open = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((column.height() * 0.18).max(8.0));
                ui.label(
                    egui::RichText::new("Serj")
                        .size(56.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.label(
                    egui::RichText::new("Lecteur de BD, case par case")
                        .size(16.0)
                        .color(MUTED),
                );
                ui.add_space(28.0);
                let button =
                    egui::Button::new(egui::RichText::new("Ouvrir un PDF…   ⌘ O").size(16.0))
                        .min_size(vec2(220.0, 40.0));
                if ui.add(button).clicked() {
                    self.pick_file(ctx);
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("ou glisse un PDF dans la fenêtre").color(MUTED));

                let recents: Vec<PathBuf> = self
                    .state
                    .recents
                    .iter()
                    .filter(|p| p.exists())
                    .cloned()
                    .collect();
                if !recents.is_empty() {
                    ui.add_space(36.0);
                    ui.label(egui::RichText::new("Récents").size(13.0).color(MUTED));
                    ui.add_space(6.0);
                    for path in recents {
                        let name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let pos = self.state.position(&path);
                        let where_ = if pos.pages == 0 {
                            format!("page {}", pos.page + 1)
                        } else if pos.progress >= 0.999 {
                            "terminé".to_string()
                        } else {
                            format!(
                                "page {} / {}   ·   {}",
                                pos.page + 1,
                                pos.pages,
                                percent(pos.progress)
                            )
                        };
                        let text = format!("{}   ·   {where_}", shorten(&name, 40));
                        let row = ui
                            .add(egui::Button::new(text).frame(false))
                            .on_hover_text(path.display().to_string());
                        if pos.pages > 0 {
                            // Petite barre d'avancement sous le titre.
                            let track = Rect::from_center_size(
                                pos2(row.rect.center().x, row.rect.bottom() + 1.0),
                                vec2(row.rect.width().min(260.0), 2.0),
                            );
                            let mut done = track;
                            done.max.x =
                                track.left() + track.width() * pos.progress.clamp(0.0, 1.0);
                            ui.painter().rect_filled(track, 1.0, Color32::from_gray(50));
                            ui.painter().rect_filled(done, 1.0, ACCENT);
                            ui.add_space(6.0);
                        }
                        if row.clicked() {
                            open = Some(path);
                        }
                    }
                }
            });
        });
        if let Some(path) = open {
            self.open(ctx, path);
        }
    }

    fn reading(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, area: Rect, now: f64) {
        let whole_page = self.state.whole_page;
        let book = self.book.as_mut().expect("album ouvert");
        book.receive(ctx);

        // Clavier.
        let (next, previous, toggle, first, last, fullscreen, escape, close) = ctx.input_mut(|i| {
            let shift_space = i.consume_key(egui::Modifiers::SHIFT, Key::Space);
            let mut any = |keys: &[Key]| {
                keys.iter().fold(false, |hit, k| {
                    i.consume_key(egui::Modifiers::NONE, *k) | hit
                })
            };
            (
                any(&[
                    Key::ArrowRight,
                    Key::ArrowDown,
                    Key::Space,
                    Key::PageDown,
                    Key::Enter,
                ]),
                any(&[Key::ArrowLeft, Key::ArrowUp, Key::PageUp, Key::Backspace]) || shift_space,
                any(&[Key::P]),
                any(&[Key::Home]),
                any(&[Key::End]),
                any(&[Key::F]),
                any(&[Key::Escape]),
                i.consume_key(egui::Modifiers::COMMAND, Key::W),
            )
        });

        // Souris : clic à droite pour avancer, à gauche pour reculer ; molette.
        let response = ui.interact(area, ui.id().with("lecture"), Sense::click());
        let mut click_next = false;
        let mut click_previous = false;
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            if p.x > area.left() + area.width() / 3.0 {
                click_next = true;
            } else {
                click_previous = true;
            }
        }
        let wheel = ctx.input(|i| i.smooth_scroll_delta.y);
        self.scroll += wheel;
        let mut wheel_step = 0;
        if self.scroll.abs() > 90.0 {
            wheel_step = if self.scroll < 0.0 { 1 } else { -1 };
            self.scroll = 0.0;
        }
        if wheel == 0.0 {
            self.scroll *= 0.8;
        }

        let before = book.position;
        if next || click_next || wheel_step > 0 || std::mem::take(&mut self.auto_next) {
            book.next(now, whole_page);
        }
        if previous || click_previous || wheel_step < 0 {
            book.previous(now, whole_page);
        }
        if first {
            book.go_to_page(0, whole_page);
        }
        if last {
            book.go_to_page(usize::MAX, whole_page);
        }
        if toggle {
            self.state.whole_page = !whole_page;
            if let Some(here) = book.camera(now, whole_page) {
                book.glide(From::Rect(here), MOVE_SECS);
            }
        }
        // Échap : sort du plein écran, sinon ferme l'album.
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if fullscreen || (escape && is_fullscreen) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
        }
        let mut close = close || (escape && !is_fullscreen);
        let moved = book.position.page != before.page || book.position.panel != before.panel;
        if moved || toggle {
            self.hud_until = now + HUD_SECS;
            self.state_dirty_since.get_or_insert(now);
        }
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
            let scale = (room.width() / camera.width()).min(room.height() / camera.height());
            let to_screen = |p: Pos2| area.center() + (p - camera.center()) * scale;
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

        // La barre d'avancement, tout en bas : on y clique ou on la fait
        // glisser pour sauter à une page.
        let mut jump = None;
        if let (Some(count), Some(progress)) = (book.page_count, book.progress(whole_page)) {
            let zone = Rect::from_min_max(pos2(area.left(), area.bottom() - 18.0), area.max);
            let bar = ui.interact(zone, ui.id().with("avancement"), Sense::click_and_drag());
            let active = bar.hovered() || bar.dragged();
            let height = if active { 8.0 } else { 3.0 };
            let track = Rect::from_min_max(pos2(area.left(), area.bottom() - height), area.max);
            painter.rect_filled(track, 0.0, Color32::from_gray(if active { 60 } else { 42 }));
            let mut done = track;
            done.max.x = track.left() + track.width() * progress;
            painter.rect_filled(done, 0.0, ACCENT);
            if active {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                self.hud_until = self.hud_until.max(now + 1.0);
            }
            let page_at = |x: f32| {
                let t = ((x - track.left()) / track.width()).clamp(0.0, 0.9999);
                (t * count as f32) as usize
            };
            if let Some(p) = bar.hover_pos().or(bar.interact_pointer_pos()) {
                let target = page_at(p.x);
                let label = painter.layout_no_wrap(
                    format!("Page {}", target + 1),
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
                if (bar.clicked() || bar.dragged()) && target != book.position.page {
                    jump = Some(target);
                }
            }
        }
        if let Some(page) = jump {
            book.go_to_page(page, whole_page);
            self.hud_until = now + HUD_SECS;
            self.state_dirty_since.get_or_insert(now);
        }

        // Où on en est, un instant après chaque mouvement.
        if now < self.hud_until
            && let Some(count) = book.page_count
        {
            let mut text = format!("Page {} / {count}", book.position.page + 1);
            if !whole_page && let Some(n) = book.panel_count() {
                text += &format!("   ·   Case {} / {n}", book.position.panel.min(n - 1) + 1);
            }
            if let Some(progress) = book.progress(whole_page) {
                text += &format!("   ·   {}", percent(progress));
            }
            let galley =
                painter.layout_no_wrap(text, FontId::proportional(14.0), Color32::from_gray(225));
            let pill = Rect::from_center_size(
                pos2(area.center().x, area.bottom() - 52.0),
                galley.size() + vec2(28.0, 12.0),
            );
            painter.rect_filled(pill, 14.0, Color32::from_black_alpha(200));
            painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
            let hint = "Flèches ou clic  case suivante    Barre du bas  aller à une page    P  page entière    F  plein écran    Échap  accueil";
            painter.text(
                pos2(area.center().x, area.bottom() - 22.0),
                Align2::CENTER_BOTTOM,
                hint,
                FontId::proportional(11.0),
                Color32::from_gray(110),
            );
            ctx.request_repaint_after(Duration::from_secs_f64((self.hud_until - now).max(0.05)));
        }

        // Le retour à l'accueil, en haut à gauche, quand on bouge la souris.
        if now < self.hud_until {
            let button = Rect::from_min_size(area.min + vec2(14.0, 14.0), vec2(108.0, 30.0));
            let back = ui.interact(button, ui.id().with("accueil"), Sense::click());
            let fill = if back.hovered() { 235 } else { 190 };
            painter.rect_filled(button, 15.0, Color32::from_black_alpha(fill));
            // La flèche dessinée : la police n'a pas « ← ».
            let tip = pos2(button.left() + 16.0, button.center().y);
            let stroke = egui::Stroke::new(1.6, Color32::WHITE);
            painter.line_segment([tip, tip + vec2(12.0, 0.0)], stroke);
            painter.line_segment([tip, tip + vec2(5.0, -5.0)], stroke);
            painter.line_segment([tip, tip + vec2(5.0, 5.0)], stroke);
            painter.text(
                pos2(button.left() + 38.0, button.center().y),
                Align2::LEFT_CENTER,
                "Accueil",
                FontId::proportional(14.0),
                Color32::WHITE,
            );
            if back.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                self.hud_until = self.hud_until.max(now + 1.0);
            }
            close |= back.clicked();
        }

        if close {
            self.close(ctx);
        }
    }
}

fn percent(progress: f32) -> String {
    format!("{} %", (progress * 100.0).floor() as u32)
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).collect::<String>() + "…"
    }
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}
