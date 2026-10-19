//! La bibliothèque de l'accueil : les albums ouverts et ceux des dossiers
//! ajoutés, avec leur couverture. Les couvertures sont rendues en tâche de fond
//! et gardées dans le cache, pour s'afficher tout de suite la fois suivante.
//! Celles des tomes qui manquent sont téléchargées (BnF, Open Library).

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use serj::pdf::Document;

/// Hauteur des couvertures rendues, en pixels.
const COVER_HEIGHT: f32 = 480.0;
/// Profondeur maximale de recherche dans les dossiers ajoutés.
const MAX_DEPTH: usize = 4;
/// Albums trouvés au plus dans les dossiers (un dossier de tout un disque ne bloque rien).
const MAX_FOUND: usize = 2000;

pub enum Cover {
    Pending,
    Ready(egui::TextureHandle),
    Failed,
}

pub struct Library {
    pub covers: HashMap<PathBuf, Cover>,
    requests: Sender<PathBuf>,
    /// Les couvertures à télécharger : la clé, et les adresses à essayer.
    downloads: Sender<(PathBuf, Vec<String>)>,
    results: Receiver<(PathBuf, Option<egui::ColorImage>)>,
}

impl Library {
    pub fn new(repaint: egui::Context) -> Self {
        let (requests, inbox) = channel::<PathBuf>();
        let (outbox, results) = channel();
        let (outbox_web, repaint_web) = (outbox.clone(), repaint.clone());
        std::thread::Builder::new()
            .name("serj-couvertures".into())
            .spawn(move || {
                for path in inbox {
                    let _ = outbox.send((path.clone(), cover(&path)));
                    repaint.request_repaint();
                }
            })
            .expect("thread des couvertures");
        // À part : le réseau ne retarde pas les couvertures des PDF.
        let (downloads, inbox) = channel::<(PathBuf, Vec<String>)>();
        let outbox = outbox_web;
        std::thread::Builder::new()
            .name("serj-couvertures-web".into())
            .spawn(move || {
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(std::time::Duration::from_secs(15)))
                    .build()
                    .new_agent();
                for (key, urls) in inbox {
                    let image = urls.iter().find_map(|url| download(&agent, url));
                    let _ = outbox.send((key, image));
                    repaint_web.request_repaint();
                }
            })
            .expect("thread des couvertures en ligne");
        Self {
            covers: HashMap::new(),
            requests,
            downloads,
            results,
        }
    }

    /// Range les couvertures arrivées.
    pub fn receive(&mut self, ctx: &egui::Context) {
        while let Ok((path, image)) = self.results.try_recv() {
            let cover = match image {
                Some(image) => Cover::Ready(ctx.load_texture(
                    format!("couverture-{}", path.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                )),
                None => Cover::Failed,
            };
            self.covers.insert(path, cover);
        }
    }

    /// La couverture de `path`, demandée au besoin.
    pub fn cover(&mut self, path: &Path) -> &Cover {
        if !self.covers.contains_key(path) {
            let _ = self.requests.send(path.to_path_buf());
            self.covers.insert(path.to_path_buf(), Cover::Pending);
        }
        &self.covers[path]
    }

    /// La couverture d'un album qu'on n'a pas, d'après `urls` (la première
    /// qui donne une image), rangée sous `key`.
    pub fn web_cover(&mut self, key: &Path, urls: &[String]) -> &Cover {
        if !self.covers.contains_key(key) {
            if urls.is_empty() {
                return &Cover::Failed;
            }
            let _ = self.downloads.send((key.to_path_buf(), urls.to_vec()));
            self.covers.insert(key.to_path_buf(), Cover::Pending);
        }
        &self.covers[key]
    }
}

/// Une image en ligne, depuis le cache sinon téléchargée.
fn download(agent: &ureq::Agent, url: &str) -> Option<egui::ColorImage> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let cached = directories::ProjectDirs::from("", "", "Serj").map(|d| {
        d.cache_dir()
            .join("couvertures")
            .join(format!("web-{:016x}.png", hasher.finish()))
    });
    let image = match cached.as_ref().and_then(|f| image::open(f).ok()) {
        Some(i) => i.to_rgba8(),
        None => {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let bytes = agent
                .get(url)
                .header("User-Agent", concat!("Serj/", env!("CARGO_PKG_VERSION")))
                .call()
                .ok()?
                .body_mut()
                .with_config()
                .limit(10 << 20)
                .read_to_vec()
                .ok()?;
            // Une page d'erreur, ou une image vide (un pixel), n'est pas une couverture.
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            if image.width() < 40 || image.height() < 40 {
                return None;
            }
            if let Some(file) = &cached {
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = image.save(file);
            }
            image
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        image.as_raw(),
    ))
}

/// Les albums à montrer : les récents d'abord (dans leur ordre), puis ceux
/// ajoutés et ceux des dossiers, par nom.
pub fn books(recents: &[PathBuf], files: &[PathBuf], folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut books: Vec<PathBuf> = recents
        .iter()
        .filter(|p| p.exists() && seen.insert((*p).clone()))
        .cloned()
        .collect();
    let mut found: Vec<PathBuf> = files.iter().filter(|p| p.exists()).cloned().collect();
    for folder in folders {
        scan(folder, 0, &mut found);
    }
    found.sort_by_key(|p| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    books.extend(found.into_iter().filter(|p| seen.insert(p.clone())));
    books
}

fn scan(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || found.len() >= MAX_FOUND {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        if path.is_dir() {
            scan(&path, depth + 1, found);
        } else if crate::app::is_pdf(&path) {
            found.push(path);
        }
    }
}

/// Le fichier de cache de la couverture : change avec le fichier (taille, date).
fn cache_file(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok().hash(&mut hasher);
    let dirs = directories::ProjectDirs::from("", "", "Serj")?;
    Some(
        dirs.cache_dir()
            .join("couvertures")
            .join(format!("{:016x}.png", hasher.finish())),
    )
}

/// La première page de `path`, en petit : depuis le cache, sinon rendue.
fn cover(path: &Path) -> Option<egui::ColorImage> {
    let cached = cache_file(path);
    let image = cached
        .as_ref()
        .and_then(|f| image::open(f).ok())
        .map(|i| i.to_rgba8())
        .or_else(|| {
            let doc = Document::open(std::fs::read(path).ok()?).ok()?;
            let page = doc.render(0, COVER_HEIGHT)?;
            let image =
                image::RgbaImage::from_raw(page.width as u32, page.height as u32, page.rgba)?;
            if let Some(file) = &cached {
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = image.save(file);
            }
            Some(image)
        })?;
    let size = [image.width() as usize, image.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        image.as_raw(),
    ))
}
