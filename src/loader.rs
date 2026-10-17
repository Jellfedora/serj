//! Chargeur en tâche de fond : ouvre le PDF, rend les pages demandées et y
//! détecte les cases, pour que la lecture ne saccade jamais.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use serj::panels::{self, Rect};
use serj::pdf::{Document, PageImage};

/// Hauteur de rendu des pages, en pixels : assez pour qu'une petite case reste
/// nette une fois agrandie à la taille de l'écran.
const RENDER_HEIGHT: f32 = 3600.0;

enum Request {
    /// Pages voulues, la plus urgente d'abord.
    Want(Vec<usize>),
    /// Page oubliée par l'appli : à rendre de nouveau si on la redemande.
    Forget(usize),
    /// Sens de lecture des cases : de droite à gauche pour un manga. Les pages
    /// déjà rendues le sont de nouveau, dans le nouvel ordre.
    RightToLeft(bool),
}

pub enum Event {
    Opened { pages: usize },
    Failed(String),
    Page(LoadedPage),
}

pub struct LoadedPage {
    pub index: usize,
    pub image: PageImage,
    /// Cases dans l'ordre de lecture, en pixels de `image`.
    pub panels: Vec<Rect>,
}

pub struct Loader {
    requests: Sender<Request>,
    pub events: Receiver<Event>,
}

impl Loader {
    pub fn open(path: PathBuf, right_to_left: bool, repaint: egui::Context) -> Self {
        let (requests, inbox) = channel();
        let (outbox, events) = channel();
        std::thread::Builder::new()
            .name("serj-loader".into())
            .spawn(move || run(path, right_to_left, inbox, outbox, repaint))
            .expect("thread du chargeur");
        Self { requests, events }
    }

    pub fn want(&self, pages: Vec<usize>) {
        let _ = self.requests.send(Request::Want(pages));
    }

    pub fn forget(&self, page: usize) {
        let _ = self.requests.send(Request::Forget(page));
    }

    pub fn right_to_left(&self, rtl: bool) {
        let _ = self.requests.send(Request::RightToLeft(rtl));
    }
}

fn run(
    path: PathBuf,
    mut rtl: bool,
    inbox: Receiver<Request>,
    outbox: Sender<Event>,
    repaint: egui::Context,
) {
    let send = |event| {
        let _ = outbox.send(event);
        repaint.request_repaint();
    };
    let doc = match std::fs::read(&path)
        .map_err(|e| format!("Impossible de lire le fichier ({e})"))
        .and_then(Document::open)
    {
        Ok(doc) => doc,
        Err(e) => return send(Event::Failed(e)),
    };
    let count = doc.page_count();
    send(Event::Opened { pages: count });

    let mut wanted: Vec<usize> = Vec::new();
    let mut sent: HashSet<usize> = HashSet::new();
    let apply =
        |r: Request, wanted: &mut Vec<usize>, sent: &mut HashSet<usize>, rtl: &mut bool| match r {
            Request::Want(pages) => *wanted = pages,
            Request::Forget(page) => {
                sent.remove(&page);
            }
            Request::RightToLeft(on) if on != *rtl => {
                *rtl = on;
                sent.clear();
            }
            Request::RightToLeft(_) => {}
        };
    loop {
        // On n'attend que s'il n'y a rien à rendre ; les nouvelles demandes
        // remplacent toujours les anciennes.
        if wanted.iter().all(|p| sent.contains(p) || *p >= count) {
            match inbox.recv() {
                Ok(r) => apply(r, &mut wanted, &mut sent, &mut rtl),
                Err(_) => return,
            }
        }
        while let Ok(r) = inbox.try_recv() {
            apply(r, &mut wanted, &mut sent, &mut rtl);
        }
        let Some(&index) = wanted.iter().find(|p| !sent.contains(p) && **p < count) else {
            continue;
        };
        sent.insert(index);
        let Some(image) = doc.render(index, RENDER_HEIGHT) else {
            continue;
        };
        let mut found = panels::detect(image.width, image.height, &image.rgba, rtl);
        if found.is_empty() {
            // Page blanche ou presque : on la montre entière.
            found.push(Rect {
                x0: 0,
                y0: 0,
                x1: image.width,
                y1: image.height,
            });
        }
        send(Event::Page(LoadedPage {
            index,
            image,
            panels: found,
        }));
    }
}
