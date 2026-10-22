//! Les BD en archives d'images : CBZ (zip) et CBR (rar). Les pages sont les
//! images de l'archive, dans l'ordre de leurs noms (« page 2 » avant « page 10 »).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::pdf::PageImage;

pub const EXTENSIONS: [&str; 2] = ["cbz", "cbr"];

pub fn is_archive(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

enum Pages {
    /// Un zip : chaque image est lue quand on la demande.
    Zip(Mutex<zip::ZipArchive<std::io::BufReader<std::fs::File>>>),
    /// Un rar ne se lit que d'un bout à l'autre : ses images sont gardées.
    Rar(Vec<Vec<u8>>),
}

pub struct Archive {
    names: Vec<String>,
    pages: Pages,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self, String> {
        // Un CBR est parfois un zip renommé, et inversement.
        if is_zip(path) {
            let file = std::fs::File::open(path)
                .map_err(|e| format!("Impossible de lire le fichier ({e})"))?;
            let zip = zip::ZipArchive::new(std::io::BufReader::new(file))
                .map_err(|_| "Archive illisible".to_string())?;
            let mut names: Vec<String> = zip
                .file_names()
                .filter(|n| is_image(n))
                .map(str::to_string)
                .collect();
            names.sort_by(|a, b| natural(a, b));
            if names.is_empty() {
                return Err("Aucune image dans cette archive".into());
            }
            Ok(Self {
                names,
                pages: Pages::Zip(Mutex::new(zip)),
            })
        } else {
            let mut found = rar_images(path, |_| true)?;
            found.sort_by(|a, b| natural(&a.0, &b.0));
            if found.is_empty() {
                return Err("Aucune image dans cette archive".into());
            }
            let (names, images) = found.into_iter().unzip();
            Ok(Self {
                names,
                pages: Pages::Rar(images),
            })
        }
    }

    pub fn page_count(&self) -> usize {
        self.names.len()
    }

    /// L'image de la page `index`, réduite à `target_height` pixels de haut au
    /// plus (jamais agrandie : elle reste nette telle quelle).
    pub fn render(&self, index: usize, target_height: f32) -> Option<PageImage> {
        let bytes = match &self.pages {
            Pages::Zip(zip) => {
                let mut zip = zip.lock().ok()?;
                let mut file = zip.by_name(self.names.get(index)?).ok()?;
                let mut bytes = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut bytes).ok()?;
                bytes
            }
            Pages::Rar(images) => images.get(index)?.clone(),
        };
        decode(&bytes, target_height)
    }
}

/// La première image d'une archive, pour sa couverture.
pub fn cover(path: &Path, target_height: f32) -> Option<PageImage> {
    if is_zip(path) {
        return Archive::open(path).ok()?.render(0, target_height);
    }
    // Le nom de la première page d'abord, puis seulement elle.
    let mut names: Vec<String> = unrar::Archive::new(&path.to_path_buf())
        .open_for_listing()
        .ok()?
        .flatten()
        .filter(|h| h.is_file())
        .map(|h| h.filename.to_string_lossy().into_owned())
        .filter(|n| is_image(n))
        .collect();
    names.sort_by(|a, b| natural(a, b));
    let first = names.into_iter().next()?;
    let (_, bytes) = rar_images(path, |n| n == first).ok()?.into_iter().next()?;
    decode(&bytes, target_height)
}

fn is_zip(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok_and(|_| magic == *b"PK\x03\x04")
}

/// Les images d'un rar que `wanted` retient, avec leur nom.
fn rar_images(
    path: &Path,
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let path: PathBuf = path.to_path_buf();
    let mut archive = unrar::Archive::new(&path)
        .open_for_processing()
        .map_err(|_| "Archive illisible".to_string())?;
    let mut found = Vec::new();
    while let Some(header) = archive
        .read_header()
        .map_err(|_| "Archive abîmée".to_string())?
    {
        let name = header.entry().filename.to_string_lossy().into_owned();
        archive = if header.entry().is_file() && is_image(&name) && wanted(&name) {
            let (bytes, rest) = header.read().map_err(|_| "Archive abîmée".to_string())?;
            found.push((name, bytes));
            rest
        } else {
            header.skip().map_err(|_| "Archive abîmée".to_string())?
        };
    }
    Ok(found)
}

fn is_image(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // Les vignettes de macOS (`__MACOSX/._page.jpg`) ne sont pas des pages.
    !lower.starts_with("__macosx")
        && !lower.rsplit('/').next().is_some_and(|f| f.starts_with('.'))
        && [".jpg", ".jpeg", ".png", ".gif", ".webp"]
            .iter()
            .any(|e| lower.ends_with(e))
}

fn decode(bytes: &[u8], target_height: f32) -> Option<PageImage> {
    let mut image = image::load_from_memory(bytes).ok()?;
    if image.height() as f32 > target_height * 1.05 {
        let width = (image.width() as f32 * target_height / image.height() as f32).round() as u32;
        image = image.resize_exact(
            width.max(1),
            target_height as u32,
            image::imageops::FilterType::Triangle,
        );
    }
    let rgba = image.to_rgba8();
    Some(PageImage {
        width: rgba.width() as usize,
        height: rgba.height() as usize,
        rgba: rgba.into_raw(),
    })
}

/// Compare deux noms en lisant les nombres comme des nombres : « 2 » avant « 10 ».
pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let number = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s.trim_start_matches('0').to_string()
                };
                let (m, n) = (number(&mut x), number(&mut y));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(&n));
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_pages_by_number() {
        let mut names = vec!["p10.jpg", "p2.jpg", "P1.jpg", "cover.jpg", "p02b.jpg"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(
            names,
            ["cover.jpg", "P1.jpg", "p2.jpg", "p02b.jpg", "p10.jpg"]
        );
        assert!(is_image("Album/page 1.JPG"));
        assert!(!is_image("__MACOSX/Album/._page 1.jpg"));
        assert!(!is_image("ComicInfo.xml"));
    }
}
