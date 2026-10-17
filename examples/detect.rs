//! Débogage : rend des pages d'un PDF et y dessine les cases détectées.
//! cargo run --release --example detect -- fichier.pdf dossier_sortie [première] [dernière]

use serj::{panels, pdf::Document};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let doc = Document::open(std::fs::read(&args[1]).unwrap()).unwrap();
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    let first: usize = args.get(3).map_or(0, |s| s.parse().unwrap());
    let last: usize = args
        .get(4)
        .map_or(doc.page_count() - 1, |s| s.parse().unwrap());
    println!("{} pages", doc.page_count());
    for i in first..=last.min(doc.page_count() - 1) {
        let t = std::time::Instant::now();
        let page = doc.render(i, 3600.0).unwrap();
        let rendered = t.elapsed();
        let found = panels::detect(page.width, page.height, &page.rgba, false);
        println!(
            "page {i}: {}x{} rendu {rendered:?} détection {:?} → {} cases",
            page.width,
            page.height,
            t.elapsed() - rendered,
            found.len()
        );
        let mut img =
            image::RgbaImage::from_raw(page.width as u32, page.height as u32, page.rgba).unwrap();
        for (n, r) in found.iter().enumerate() {
            let c = image::Rgba([230, 30, 60, 255]);
            for t in 0..10 {
                for x in r.x0..r.x1 {
                    img.put_pixel(x as u32, (r.y0 + t).min(r.y1 - 1) as u32, c);
                    img.put_pixel(x as u32, (r.y1 - 1).saturating_sub(t) as u32, c);
                }
                for y in r.y0..r.y1 {
                    img.put_pixel((r.x0 + t).min(r.x1 - 1) as u32, y as u32, c);
                    img.put_pixel((r.x1 - 1).saturating_sub(t) as u32, y as u32, c);
                }
            }
            // Numéro de case : n+1 petits carrés en haut à gauche.
            for k in 0..=n {
                for dy in 0..18 {
                    for dx in 0..18 {
                        img.put_pixel((r.x0 + 14 + k * 24 + dx) as u32, (r.y0 + 14 + dy) as u32, c);
                    }
                }
            }
        }
        img.save(out.join(format!("p{i:03}.png"))).unwrap();
    }
}
