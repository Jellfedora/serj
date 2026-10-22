//! Lecture des PDF : chaque page rendue en image RGBA par hayro (pur Rust).

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::content::ops::TypedInstruction;
use hayro::hayro_syntax::object::Object;
use hayro::hayro_syntax::page::Page;
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

    /// Un livre plutôt qu'une BD : la plupart des pages, prises çà et là dans
    /// le document, portent surtout du texte (beaucoup de caractères, ou du
    /// texte sans image).
    pub fn mostly_text(&self) -> bool {
        let pages = self.pdf.pages();
        let count = pages.len();
        if count == 0 {
            return false;
        }
        let samples: Vec<usize> = if count <= 10 {
            (0..count).collect()
        } else {
            (1..=10).map(|k| count * k / 11).collect()
        };
        let text_pages = samples
            .iter()
            .filter(|&&i| {
                let (chars, images) = page_contents(&pages[i]);
                chars >= 500 || (chars >= 150 && images == 0)
            })
            .count();
        text_pages * 2 > samples.len()
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

/// Combien la page affiche de caractères (octets de texte), et d'objets
/// dessinés d'un coup (des images, le plus souvent).
fn page_contents(page: &Page) -> (usize, usize) {
    let (mut chars, mut images) = (0, 0);
    let mut ops = page.typed_operations();
    while let Some(op) = ops.next() {
        match op {
            TypedInstruction::ShowText(t) => chars += t.0.as_bytes().len(),
            TypedInstruction::NextLineAndShowText(t) => chars += t.0.as_bytes().len(),
            TypedInstruction::ShowTexts(t) => {
                for item in t.0.iter::<Object>() {
                    if let Object::String(s) = item {
                        chars += s.as_bytes().len();
                    }
                }
            }
            TypedInstruction::XObject(_) => images += 1,
            _ => {}
        }
    }
    (chars, images)
}
