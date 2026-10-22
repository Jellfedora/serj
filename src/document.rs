//! Les albums en pages d'images : PDF (rendus par hayro) et archives d'images
//! (CBZ, CBR), lus de la même façon.

use std::path::Path;

use crate::archive::{self, Archive};
use crate::pdf::{self, PageImage};

pub enum Document {
    Pdf(pdf::Document),
    Archive(Archive),
}

impl Document {
    pub fn open(path: &Path) -> Result<Self, String> {
        if archive::is_archive(path) {
            return Archive::open(path).map(Self::Archive);
        }
        let bytes =
            std::fs::read(path).map_err(|e| format!("Impossible de lire le fichier ({e})"))?;
        pdf::Document::open(bytes).map(Self::Pdf)
    }

    pub fn page_count(&self) -> usize {
        match self {
            Self::Pdf(d) => d.page_count(),
            Self::Archive(a) => a.page_count(),
        }
    }

    /// Rend la page `index` pour qu'elle fasse environ `target_height` pixels de haut.
    pub fn render(&self, index: usize, target_height: f32) -> Option<PageImage> {
        match self {
            Self::Pdf(d) => d.render(index, target_height),
            Self::Archive(a) => a.render(index, target_height),
        }
    }
}

/// La première page de `path`, en petit (pour sa couverture).
pub fn cover(path: &Path, target_height: f32) -> Option<PageImage> {
    if archive::is_archive(path) {
        return archive::cover(path, target_height);
    }
    pdf::Document::open(std::fs::read(path).ok()?)
        .ok()?
        .render(0, target_height)
}
