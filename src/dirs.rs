//! Où Serj garde ses affaires : ses réglages, ses copies des albums, ses caches.
//!
//! Une version de développement (`cargo run`) a les siens, dans `dev-data/` à
//! la racine du projet : elle ne touche pas à la bibliothèque de la version
//! installée, et celle-ci pas à la sienne.

use std::path::PathBuf;

/// Les réglages et l'état (`state.json`).
pub fn config() -> Option<PathBuf> {
    dir(|d| d.config_dir().to_path_buf(), "config")
}

/// Les copies des albums (`books`).
pub fn data() -> Option<PathBuf> {
    dir(|d| d.data_dir().to_path_buf(), "data")
}

/// Ce qui se retrouve si on l'efface : couvertures, infos trouvées en ligne.
pub fn cache() -> Option<PathBuf> {
    dir(|d| d.cache_dir().to_path_buf(), "cache")
}

/// Un développement : la version de `cargo run`, pas celle qu'on installe.
pub const DEV: bool = cfg!(debug_assertions);

fn dir(installed: impl Fn(&directories::ProjectDirs) -> PathBuf, dev: &str) -> Option<PathBuf> {
    if DEV {
        return Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("dev-data")
                .join(dev),
        );
    }
    directories::ProjectDirs::from("", "", "Serj").map(|d| installed(&d))
}
