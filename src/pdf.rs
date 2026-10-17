//! Lecture des PDF : chaque page rendue en image RGBA par hayro (pur Rust).

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

pub struct Document {
    pdf: Pdf,
}

/// Une page rendue : pixels RGBA, ligne par ligne.
pub struct PageImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Document {
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let pdf = Pdf::new(bytes).map_err(|e| format!("PDF illisible ({e:?})"))?;
        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Rend la page `index` pour qu'elle fasse environ `target_height` pixels de haut.
    pub fn render(&self, index: usize, target_height: f32) -> Option<PageImage> {
        let page = self.pdf.pages().get(index)?;
        let (_, h) = page.render_dimensions();
        // Les pixmaps de vello_cpu sont limités à u16 de côté.
        let scale = (target_height / h.max(1.0)).clamp(0.1, 60_000.0 / h.max(1.0));
        let cache = RenderCache::new();
        let pixmap = render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        // Fond blanc opaque : les pixels prémultipliés valent les pixels droits.
        Some(PageImage {
            width: pixmap.width() as usize,
            height: pixmap.height() as usize,
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }
}
