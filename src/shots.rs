//! Mode de test : `SERJ_SHOTS=dossier SERJ_STEPS=n` enregistre une image de la
//! fenêtre à chaque arrêt de la caméra, avance d'une case, et ferme Serj après
//! n pas. Sert à vérifier l'affichage sans capture d'écran du système.

use std::path::PathBuf;

pub struct Shots {
    dir: PathBuf,
    steps_left: usize,
    taken: usize,
    /// Moment où l'image a été demandée, tant qu'elle n'est pas arrivée.
    waiting: Option<f64>,
    next_at: f64,
}

impl Shots {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("SERJ_SHOTS")?);
        let _ = std::fs::create_dir_all(&dir);
        let steps = std::env::var("SERJ_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
        Some(Self { dir, steps_left: steps, taken: 0, waiting: None, next_at: 1.0 })
    }

    /// À appeler à chaque image ; vrai quand il faut avancer d'un pas.
    pub fn frame(&mut self, ctx: &egui::Context, now: f64, idle: bool) -> bool {
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = img.save(self.dir.join(format!("shot{:02}.png", self.taken)));
            }
            self.taken += 1;
            self.waiting = None;
            self.next_at = now + 0.9;
            if self.steps_left == 0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return false;
            }
            self.steps_left -= 1;
            return true;
        }
        // Une demande restée sans réponse est refaite.
        let stale = self.waiting.is_some_and(|t| now - t > 2.0);
        if (self.waiting.is_none() || stale) && idle && now >= self.next_at {
            self.waiting = Some(now);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint();
        false
    }
}
