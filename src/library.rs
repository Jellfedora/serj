//! La bibliothèque de l'accueil : les albums ouverts et ceux des dossiers
//! ajoutés, avec leur couverture. Les couvertures sont rendues en tâche de fond
//! et gardées dans le cache, pour s'afficher tout de suite la fois suivante.
//! Celles des tomes qui manquent sont téléchargées (BnF, Open Library).
//! Les albums ajoutés sont copiés dans le dossier de Serj, rangés par série ;
//! les livres à part, dans « Livres ». Un PDF fait surtout de texte est
//! reconnu comme un livre, en tâche de fond aussi.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use serj::{archive, document, ebook};

/// Hauteur des couvertures rendues, en pixels.
const COVER_HEIGHT: f32 = 480.0;
/// Profondeur maximale de recherche dans un dossier ajouté.
const MAX_DEPTH: usize = 4;
/// Albums trouvés au plus dans un dossier (un dossier de tout un disque ne bloque rien).
const MAX_FOUND: usize = 2000;

pub enum Cover {
    Pending,
    Ready(egui::TextureHandle),
    Failed,
}

/// Où en sont les copies vers le dossier de Serj.
#[derive(Clone, Default)]
pub struct Copying {
    /// Albums qui restent à copier, celui en cours compris.
    pub left: usize,
    /// L'album en cours.
    pub name: String,
    /// Octets copiés, et à copier en tout, depuis que la file s'est remplie.
    pub done: u64,
    pub total: u64,
}

pub struct Library {
    pub covers: HashMap<PathBuf, Cover>,
    requests: Sender<PathBuf>,
    /// Les couvertures à télécharger : la clé, et les adresses à essayer.
    downloads: Sender<(PathBuf, Vec<String>)>,
    results: Receiver<(PathBuf, Option<egui::ColorImage>)>,
    /// Les albums à copier : l'original et la copie voulue.
    copies: Sender<(PathBuf, PathBuf)>,
    /// Les copies faites : l'original et sa copie.
    copied: Receiver<(PathBuf, PathBuf)>,
    copying: Arc<Mutex<Copying>>,
    /// Les PDF dont on cherche s'ils sont des livres, et les réponses.
    kinds: Sender<PathBuf>,
    detected: Receiver<(PathBuf, bool)>,
}

impl Library {
    pub fn new(repaint: egui::Context) -> Self {
        let (requests, inbox) = channel::<PathBuf>();
        let (outbox, results) = channel();
        let (outbox_web, repaint_web) = (outbox.clone(), repaint.clone());
        let repaint_copies = repaint.clone();
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
        // À part aussi : copier un gros album ne retarde pas les couvertures.
        let (copies, inbox) = channel::<(PathBuf, PathBuf)>();
        let (outbox, copied) = channel();
        let repaint = repaint_copies;
        let copying = Arc::new(Mutex::new(Copying::default()));
        let progress = copying.clone();
        std::thread::Builder::new()
            .name("serj-copies".into())
            .spawn(move || {
                for (from, to) in inbox {
                    let size = std::fs::metadata(&from).map_or(0, |m| m.len());
                    let start = {
                        let mut p = progress.lock().unwrap();
                        p.name = title(&from);
                        p.done
                    };
                    let mut shown = std::time::Instant::now();
                    let copied = copy(&from, &to, |n| {
                        progress.lock().unwrap().done = start + n;
                        if shown.elapsed().as_millis() > 100 {
                            shown = std::time::Instant::now();
                            repaint.request_repaint();
                        }
                    });
                    {
                        let mut p = progress.lock().unwrap();
                        p.done = start + size;
                        p.left = p.left.saturating_sub(1);
                        if p.left == 0 {
                            *p = Copying::default();
                        }
                    }
                    if let Some(to) = copied {
                        let _ = outbox.send((from, to));
                    }
                    repaint.request_repaint();
                }
            })
            .expect("thread des copies");
        // À part encore : lire un PDF pour savoir si c'est un livre.
        let (kinds, inbox) = channel::<PathBuf>();
        let (outbox, detected) = channel();
        std::thread::Builder::new()
            .name("serj-rayons".into())
            .spawn(move || {
                for path in inbox {
                    let text = std::fs::read(&path)
                        .ok()
                        .and_then(|bytes| serj::pdf::Document::open(bytes).ok())
                        .is_some_and(|doc| doc.mostly_text());
                    let _ = outbox.send((path, text));
                }
            })
            .expect("thread des rayons");
        Self {
            kinds,
            detected,
            covers: HashMap::new(),
            requests,
            downloads,
            results,
            copies,
            copied,
            copying,
        }
    }

    /// Cherche si le PDF `path` est un livre (réponse dans [`Self::detected`]).
    pub fn detect(&self, path: &Path) {
        let _ = self.kinds.send(path.to_path_buf());
    }

    /// Les PDF examinés depuis la dernière fois : vrai pour un livre.
    pub fn detected(&self) -> Vec<(PathBuf, bool)> {
        self.detected.try_iter().collect()
    }

    /// Copie l'album `path` dans le dossier de Serj (sous `folder`, voir
    /// [`folder`]), en tâche de fond ; rien s'il y est déjà.
    pub fn import(&self, path: &Path, folder: PathBuf) {
        let Some(dir) = books_dir() else { return };
        let Some(name) = path.file_name() else { return };
        if path.starts_with(&dir) {
            return;
        }
        let dir = dir.join(folder);
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        {
            let mut p = self.copying.lock().unwrap();
            p.left += 1;
            p.total += size;
        }
        let _ = self.copies.send((path.to_path_buf(), dir.join(name)));
    }

    /// Où en sont les copies (`left` à zéro : aucune en cours).
    pub fn copying(&self) -> Copying {
        self.copying.lock().unwrap().clone()
    }

    /// Les copies finies depuis la dernière fois : l'original et sa copie.
    pub fn copied(&self) -> Vec<(PathBuf, PathBuf)> {
        self.copied.try_iter().collect()
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

/// Où ranger un album dans le dossier de Serj : sous sa série (« Lanfeust de
/// Troy »), un livre dans « Livres », sous sa série ou son auteur.
pub fn folder(livre: bool, series: Option<&str>, author: Option<&str>) -> PathBuf {
    let mut folder = PathBuf::new();
    if livre {
        folder.push("Livres");
    }
    let name = match livre {
        true => series.or(author),
        false => series,
    };
    if let Some(name) = name.and_then(serj::meta::file_safe) {
        folder.push(name);
    }
    folder
}

/// Les fichiers que Serj sait lire : PDF, CBZ, CBR et livres en texte.
pub fn is_book(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        || archive::is_archive(path)
        || ebook::is_ebook(path)
}

/// Les extensions que Serj sait lire, pour les fenêtres d'ouverture.
pub const EXTENSIONS: [&str; 9] = [
    "pdf", "cbz", "cbr", "epub", "mobi", "azw3", "azw", "fb2", "txt",
];

/// Le dossier où Serj range ses copies des albums.
pub fn books_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "Serj")?;
    Some(dirs.data_dir().join("books"))
}

/// Ouvre le dossier des albums de Serj dans le Finder (l'Explorateur…).
pub fn open_books_dir() {
    let Some(dir) = books_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(not(any(target_os = "macos", windows)))]
    let opener = "xdg-open";
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

/// Met l'album `path` à la corbeille, s'il est dans le dossier de Serj (jamais
/// un original ailleurs), et retire le dossier de sa série s'il est vide (et
/// « Livres » aussi).
pub fn delete(path: &Path) {
    let Some(books) = books_dir() else { return };
    if !path.starts_with(&books) || trash::delete(path).is_err() {
        return;
    }
    let mut dir = path.parent();
    while let Some(d) = dir
        && d != books
        && d.starts_with(&books)
        && std::fs::remove_dir(d).is_ok()
    {
        dir = d.parent();
    }
}

fn title(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Copie `from` vers `to` (ou `to (2)`… si un autre fichier porte ce nom) ;
/// renvoie la copie. Un fichier de même nom et de même taille est pris pour
/// une copie déjà faite. `progress` reçoit les octets copiés.
fn copy(from: &Path, to: &Path, mut progress: impl FnMut(u64)) -> Option<PathBuf> {
    let size = std::fs::metadata(from).ok()?.len();
    let dir = to.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    let stem = to.file_stem()?.to_string_lossy().into_owned();
    let extension = to
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 1.. {
        let target = match n {
            1 => to.to_path_buf(),
            n => dir.join(format!("{stem} ({n}){extension}")),
        };
        match std::fs::metadata(&target) {
            Ok(meta) if meta.len() == size => return Some(target),
            Ok(_) => continue,
            Err(_) => {}
        }
        // Copié à côté puis renommé : une copie interrompue ne passe pas pour un album.
        let part = dir.join(format!(".{stem}{extension}.part"));
        if copy_file(from, &part, &mut progress).is_err() {
            let _ = std::fs::remove_file(&part);
            return None;
        }
        std::fs::rename(&part, &target).ok()?;
        return Some(target);
    }
    None
}

fn copy_file(from: &Path, to: &Path, progress: &mut impl FnMut(u64)) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut reader = std::fs::File::open(from)?;
    let mut writer = std::fs::File::create(to)?;
    let mut buffer = vec![0; 1 << 20];
    let mut done = 0;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n])?;
        done += n as u64;
        progress(done);
    }
    writer.sync_all()
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
/// ajoutés, par nom.
pub fn books(recents: &[PathBuf], files: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut books: Vec<PathBuf> = recents
        .iter()
        .filter(|p| p.exists() && seen.insert((*p).clone()))
        .cloned()
        .collect();
    let mut found: Vec<PathBuf> = files.iter().filter(|p| p.exists()).cloned().collect();
    found.sort_by_key(|p| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    books.extend(found.into_iter().filter(|p| seen.insert(p.clone())));
    books
}

/// Les PDF de `dir` et de ses sous-dossiers.
pub fn scan(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
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
        } else if is_book(&path) {
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

/// La première page de `path`, en petit : depuis le cache, sinon rendue (pour
/// un livre en texte, l'image de couverture qu'il porte).
fn cover(path: &Path) -> Option<egui::ColorImage> {
    let cached = cache_file(path);
    let image = cached
        .as_ref()
        .and_then(|f| image::open(f).ok())
        .map(|i| i.to_rgba8())
        .or_else(|| {
            let image = if ebook::is_ebook(path) {
                let bytes = ebook::metadata(path)?.1?;
                let image = image::load_from_memory(&bytes).ok()?;
                let scale = COVER_HEIGHT / image.height().max(1) as f32;
                let image = if scale < 1.0 {
                    image.resize(
                        (image.width() as f32 * scale) as u32,
                        COVER_HEIGHT as u32,
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    image
                };
                image.to_rgba8()
            } else {
                let page = document::cover(path, COVER_HEIGHT)?;
                image::RgbaImage::from_raw(page.width as u32, page.height as u32, page.rgba)?
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copie_sans_ecraser() {
        let root = std::env::temp_dir().join(format!("serj-copie-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (a, b) = (root.join("a.pdf"), root.join("b.pdf"));
        std::fs::write(&a, b"album a").unwrap();
        std::fs::write(&b, b"album bb").unwrap();
        let to = root.join("Série").join("Tome 1.pdf");
        assert_eq!(copy(&a, &to, |_| {}), Some(to.clone()));
        // Déjà copié : la même copie.
        assert_eq!(copy(&a, &to, |_| {}), Some(to.clone()));
        // Un autre album du même nom : à côté.
        let other = root.join("Série").join("Tome 1 (2).pdf");
        assert_eq!(copy(&b, &to, |_| {}), Some(other.clone()));
        assert_eq!(std::fs::read(&other).unwrap(), b"album bb");
        let _ = std::fs::remove_dir_all(&root);
    }
}
