//! Ce que Serj retient d'une fois sur l'autre : le dernier album ouvert, les
//! albums récents et où on s'est arrêté dans chacun.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serj::meta::Guess;

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Position {
    pub page: usize,
    pub panel: usize,
    /// Pages de l'album et part lue (0 à 1), pour l'accueil.
    pub pages: usize,
    pub progress: f32,
    /// Manga : pages lues de la dernière à la première, cases de droite à gauche.
    pub manga: bool,
    /// Marqué comme lu à la main (même sans l'avoir lu dans Serj).
    pub read: bool,
}

impl Position {
    /// Ouvert au moins une fois dans Serj.
    pub fn started(&self) -> bool {
        self.pages > 0
    }

    /// Lu jusqu'au bout, ou marqué comme lu.
    pub fn finished(&self) -> bool {
        self.read || (self.started() && self.progress >= 0.999)
    }

    /// Commencé et pas fini.
    pub fn in_progress(&self) -> bool {
        self.started() && !self.finished()
    }

    /// Marque l'album comme lu, ou comme pas encore lu : on le reprendra alors
    /// au début.
    pub fn mark_read(&mut self, read: bool) {
        if read {
            self.read = true;
            if self.started() {
                self.progress = 1.0;
            }
        } else {
            *self = Position {
                manga: self.manga,
                ..Position::default()
            };
        }
    }

    /// Rang de `page` dans l'ordre de lecture (et inversement : la page d'un
    /// rang) ; à l'envers pour un manga.
    pub fn rank(&self, page: usize) -> usize {
        if self.manga && self.pages > 0 {
            self.pages - 1 - page.min(self.pages - 1)
        } else {
            page
        }
    }

    /// Passe en manga (ou en BD). Au tout début de l'album, on va au début dans
    /// le nouveau sens ; renvoie vrai dans ce cas.
    pub fn set_manga(&mut self, on: bool) -> bool {
        let at_start = self.rank(self.page) == 0 && self.panel == 0;
        self.manga = on;
        if at_start {
            self.page = self.rank(0);
        }
        at_start
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Albums ouverts, le plus récent d'abord.
    pub recents: Vec<PathBuf>,
    pub positions: BTreeMap<String, Position>,
    /// Lecture page entière plutôt que case par case.
    pub whole_page: bool,
    /// Dossiers dont les PDF sont montrés dans la bibliothèque de l'accueil.
    pub folders: Vec<PathBuf>,
    /// PDF ajoutés un par un à la bibliothèque.
    pub files: Vec<PathBuf>,
    /// Lecture automatique : secondes avant de passer à la case (ou page) suivante.
    pub auto_secs: f64,
    /// Infos d'albums corrigées à la main (série, tome, titre, auteurs), pour
    /// la recherche en ligne.
    pub corrections: BTreeMap<String, Guess>,
    /// Le thème de l'interface (`widgets::THEMES`).
    pub theme: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            recents: Vec::new(),
            positions: BTreeMap::new(),
            whole_page: false,
            folders: Vec::new(),
            files: Vec::new(),
            auto_secs: 5.0,
            corrections: BTreeMap::new(),
            theme: "nuit".into(),
        }
    }
}

const MAX_RECENTS: usize = 12;

fn file() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "Serj")?;
    Some(dirs.config_dir().join("state.json"))
}

impl State {
    pub fn load() -> Self {
        file()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        let Ok(json) = serde_json::to_vec_pretty(self) else {
            return;
        };
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Écrit à côté puis renomme : un arrêt brutal ne laisse pas un fichier à moitié écrit.
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, f);
        }
    }

    pub fn opened(&mut self, path: &Path) {
        self.recents.retain(|p| p != path);
        self.recents.insert(0, path.to_path_buf());
        self.recents.truncate(MAX_RECENTS);
    }

    /// Ajoute des PDF et des dossiers à la bibliothèque.
    pub fn add(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            let list = if p.is_dir() {
                &mut self.folders
            } else if crate::app::is_pdf(&p) {
                &mut self.files
            } else {
                continue;
            };
            if !list.contains(&p) {
                list.push(p);
            }
        }
    }

    pub fn position(&self, path: &Path) -> Position {
        self.positions.get(&key(path)).copied().unwrap_or_default()
    }

    pub fn set_position(&mut self, path: &Path, position: Position) {
        self.positions.insert(key(path), position);
    }

    /// L'album `from` s'appelle maintenant `to` : sa place, sa correction et
    /// son rang dans les récents le suivent.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        for p in self.recents.iter_mut().chain(&mut self.files) {
            if p == from {
                *p = to.to_path_buf();
            }
        }
        if let Some(pos) = self.positions.remove(&key(from)) {
            self.positions.insert(key(to), pos);
        }
        if let Some(c) = self.corrections.remove(&key(from)) {
            self.corrections.insert(key(to), c);
        }
    }

    pub fn correction(&self, path: &Path) -> Option<&Guess> {
        self.corrections.get(&key(path))
    }

    pub fn set_correction(&mut self, path: &Path, guess: Option<Guess>) {
        match guess {
            Some(g) => self.corrections.insert(key(path), g),
            None => self.corrections.remove(&key(path)),
        };
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
