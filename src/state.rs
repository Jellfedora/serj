//! Ce que Serj retient d'une fois sur l'autre : le dernier album ouvert, les
//! albums récents et où on s'est arrêté dans chacun.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Position {
    pub page: usize,
    pub panel: usize,
    /// Pages de l'album et part lue (0 à 1), pour l'accueil.
    pub pages: usize,
    pub progress: f32,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Albums ouverts, le plus récent d'abord.
    pub recents: Vec<PathBuf>,
    pub positions: BTreeMap<String, Position>,
    /// Lecture page entière plutôt que case par case.
    pub whole_page: bool,
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

    pub fn position(&self, path: &Path) -> Position {
        self.positions.get(&key(path)).copied().unwrap_or_default()
    }

    pub fn set_position(&mut self, path: &Path, position: Position) {
        self.positions.insert(key(path), position);
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
