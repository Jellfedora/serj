//! Les mises à jour, depuis les releases GitHub de Serj : une vérification en
//! tâche de fond trouve une version plus récente ; si on l'accepte, l'archive de
//! ce système est téléchargée, vérifiée avec l'empreinte que publie GitHub, mise
//! à la place de l'appli, qui redémarre. Installé par Ronnie, Serj lui dit aussi
//! sa nouvelle version.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO: &str = "Jellfedora/serj";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// La cible dans le nom des archives (sur macOS, Apple Silicon seulement).
const TARGET: &str = if cfg!(target_os = "macos") {
    "aarch64-apple-darwin"
} else {
    env!("TARGET")
};
const ARCHIVE_EXT: &str = if cfg!(windows) { "zip" } else { "tar.gz" };
/// Une archive fait une dizaine de Mo : au-delà de 300, ce n'est pas la nôtre.
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;
const USER_AGENT: &str = concat!("serj/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// « sha256:<hex> », calculé par GitHub à l'envoi.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone)]
pub struct Available {
    pub version: String,
    /// La page de la release.
    pub url: String,
    /// Ses notes : la section du CHANGELOG.
    pub notes: String,
    asset: Asset,
}

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Available),
    Installing(String),
    /// La version installée, active au redémarrage.
    Installed(String),
    Failed(String),
}

pub struct Updater {
    state: Arc<Mutex<State>>,
    /// L'exécutable lancé, lu au démarrage (sur Linux, il pointe sur un
    /// fichier supprimé une fois remplacé).
    exe: Option<PathBuf>,
    /// Octets reçus, et combien il y en a (0 : on ne sait pas).
    progress: Arc<Mutex<(u64, u64)>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            state: Arc::default(),
            exe: std::env::current_exe().ok(),
            progress: Arc::default(),
        }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub fn progress(&self) -> (u64, u64) {
        *self.progress.lock().unwrap()
    }

    /// Serj peut se remplacer lui-même : il tourne depuis un paquet installé,
    /// pas depuis `cargo run`.
    pub fn can_install(&self) -> bool {
        let Some(exe) = &self.exe else { return false };
        if cfg!(target_os = "macos") {
            app_bundle(exe).is_some()
        } else {
            !exe.components().any(|c| c.as_os_str() == "target")
        }
    }

    /// Cherche une version plus récente, en tâche de fond.
    pub fn check(&self, ctx: &egui::Context) {
        if matches!(
            self.state(),
            State::Checking | State::Installing(_) | State::Installed(_)
        ) {
            return;
        }
        *self.state.lock().unwrap() = State::Checking;
        let (state, ctx) = (self.state.clone(), ctx.clone());
        thread::spawn(move || {
            let result = match latest() {
                Ok(Some(available)) => State::Available(available),
                Ok(None) => State::UpToDate,
                Err(e) => State::Failed(format!("{e:#}")),
            };
            *state.lock().unwrap() = result;
            ctx.request_repaint();
        });
    }

    /// Télécharge et installe `available`, en tâche de fond.
    pub fn install(&self, ctx: &egui::Context, available: Available) {
        let Some(exe) = self.exe.clone() else { return };
        *self.state.lock().unwrap() = State::Installing(available.version.clone());
        *self.progress.lock().unwrap() = (0, 0);
        let (state, progress, ctx) = (self.state.clone(), self.progress.clone(), ctx.clone());
        thread::spawn(move || {
            let result = match install(&exe, &available, &progress, &ctx) {
                Ok(()) => State::Installed(available.version),
                Err(e) => State::Failed(format!("{e:#}")),
            };
            *state.lock().unwrap() = result;
            ctx.request_repaint();
        });
    }

    /// Relance l'appli (à jour) ; à l'appelant de fermer celle-ci ensuite.
    pub fn relaunch(&self) -> Result<()> {
        let exe = self.exe.as_deref().context("exécutable introuvable")?;
        #[cfg(target_os = "macos")]
        if let Some(bundle) = app_bundle(exe) {
            return start_after_exit(
                Path::new("/usr/bin/open"),
                &[std::ffi::OsStr::new("-n"), bundle.as_os_str()],
            );
        }
        start_after_exit(exe, &[])
    }
}

/// Lance `program` une fois ce processus fini (ou après 10 s) : lancée avant,
/// la nouvelle instance serait prise pour celle-ci (macOS ramènerait juste
/// l'appli qui se ferme au premier plan).
#[cfg(unix)]
fn start_after_exit(program: &Path, args: &[&std::ffi::OsStr]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    const SCRIPT: &str = r#"pid=$1; shift; i=0
while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
exec "$@""#;
    Command::new("/bin/sh")
        .args(["-c", SCRIPT, "sh", &std::process::id().to_string()])
        .arg(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .with_context(|| format!("impossible de lancer {}", program.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn start_after_exit(program: &Path, args: &[&std::ffi::OsStr]) -> Result<()> {
    Command::new(program).args(args).spawn()?;
    Ok(())
}

fn agent(total: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(total))
        .user_agent(USER_AGENT)
        .build()
        .new_agent()
}

/// La dernière release, si elle est plus récente que celle-ci. Sans archive
/// pour ce système, c'est une erreur plutôt que « à jour ».
fn latest() -> Result<Option<Available>> {
    let release: Release = agent(Duration::from_secs(30))
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("GitHub injoignable")?
        .body_mut()
        .read_json()?;
    let version = release.tag_name.trim_start_matches('v').to_owned();
    if !is_newer(&version, VERSION) {
        return Ok(None);
    }
    let name = format!("serj-{TARGET}.{ARCHIVE_EXT}");
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == name)
        .with_context(|| format!("la version {version} n'a pas d'archive {name}"))?;
    Ok(Some(Available {
        version,
        url: release.html_url,
        notes: release.body,
        asset,
    }))
}

/// (majeur, mineur, correctif, préversion comme « 1.2.0-beta »).
fn parse_version(v: &str) -> Option<(u64, u64, u64, bool)> {
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((core, rest)) => (core, v.as_bytes()[core.len()] == b'-' && !rest.is_empty()),
        None => (v, false),
    };
    let mut parts = core.split('.').map(str::parse::<u64>);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        pre,
    ))
}

fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        // Mêmes numéros : la version finale passe après ses préversions.
        (Some((a, b, c, pa)), Some((x, y, z, px))) => {
            (a, b, c) > (x, y, z) || ((a, b, c) == (x, y, z) && px && !pa)
        }
        _ => false,
    }
}

fn download(asset: &Asset, progress: &Mutex<(u64, u64)>, ctx: &egui::Context) -> Result<Vec<u8>> {
    use std::io::Read as _;
    // Sans l'empreinte de GitHub, l'archive ne peut pas être vérifiée : on ne
    // l'installe pas.
    let expected = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("la release ne publie pas l'empreinte SHA-256 de son archive")?;
    let mut response = agent(Duration::from_secs(900))
        .get(&asset.browser_download_url)
        .call()
        .context("téléchargement impossible")?;
    let total: u64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .reader();
    let mut bytes = Vec::with_capacity(total.min(MAX_DOWNLOAD) as usize);
    let mut buf = vec![0u8; 64 * 1024];
    let mut shown = std::time::Instant::now();
    loop {
        let n = reader.read(&mut buf).context("téléchargement interrompu")?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if shown.elapsed() > Duration::from_millis(150) {
            shown = std::time::Instant::now();
            *progress.lock().unwrap() = (bytes.len() as u64, total);
            ctx.request_repaint();
        }
    }
    *progress.lock().unwrap() = (bytes.len() as u64, total);
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("l'archive téléchargée est abîmée (empreinte SHA-256 différente)");
    }
    Ok(bytes)
}

fn install(
    exe: &Path,
    available: &Available,
    progress: &Mutex<(u64, u64)>,
    ctx: &egui::Context,
) -> Result<()> {
    let archive = download(&available.asset, progress, ctx)?;
    #[cfg(target_os = "macos")]
    if let Some(bundle) = app_bundle(exe) {
        replace_bundle(&bundle, &archive)?;
        tell_ronnie(&bundle, &available.version);
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let dir = exe
            .parent()
            .context("dossier de l'exécutable introuvable")?;
        let staging = tempdir(dir)?;
        let result = (|| {
            extract(&archive, &staging)?;
            let name = if cfg!(windows) { "serj.exe" } else { "serj" };
            let new =
                find(&staging, name).with_context(|| format!("{name} absent de l'archive"))?;
            self_replace::self_replace(&new).context("impossible de remplacer l'exécutable")
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result?;
        tell_ronnie(exe, &available.version);
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = exe;
        bail!("Serj ne tourne pas depuis une appli installée")
    }
}

/// Installé par Ronnie (`…/modules/serj/app/…`) : son fichier `version` suit,
/// pour qu'il ne propose pas une mise à jour déjà faite.
fn tell_ronnie(installed: &Path, version: &str) {
    for dir in installed.ancestors() {
        if dir.file_name().is_some_and(|n| n == "app")
            && let Some(module) = dir.parent()
            && module.join("module.json").is_file()
            && module.join("version").is_file()
        {
            let _ = std::fs::write(module.join("version"), version);
            return;
        }
    }
}

/// Un dossier neuf à côté de `near` (même disque : les déplacements sont des
/// renommages).
fn tempdir(near: &Path) -> Result<PathBuf> {
    let dir = near.join(format!(".serj-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir)
        .with_context(|| format!("impossible d'écrire dans {}", near.display()))?;
    Ok(dir)
}

fn extract(archive: &[u8], into: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        zip::ZipArchive::new(std::io::Cursor::new(archive))?.extract(into)?;
    }
    #[cfg(not(windows))]
    {
        tar::Archive::new(flate2::read::GzDecoder::new(archive)).unpack(into)?;
    }
    Ok(())
}

/// Le premier fichier ou dossier nommé `name` sous `dir`.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if entry.file_name() == name {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find(&path, name)
        {
            return Some(found);
        }
    }
    None
}

/// Le paquet `.app` qui contient `exe`, s'il y en a un.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// Remplace tout le paquet (exécutable, Info.plist, icône), et remet l'ancien
/// si le nouveau ne peut pas prendre sa place.
#[cfg(target_os = "macos")]
fn replace_bundle(bundle: &Path, archive: &[u8]) -> Result<()> {
    let parent = bundle.parent().context("dossier de l'appli introuvable")?;
    let staging = tempdir(parent)?;
    let result = (|| {
        extract(archive, &staging)?;
        let new = find(&staging, "Serj.app").context("Serj.app absent de l'archive")?;
        let old = staging.join("previous.app");
        std::fs::rename(bundle, &old)
            .with_context(|| format!("impossible de remplacer {}", bundle.display()))?;
        if let Err(e) = std::fs::rename(&new, bundle) {
            let _ = std::fs::rename(&old, bundle);
            return Err(e).context("impossible d'installer la nouvelle version");
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("1.0.0", "0.10.0"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(!is_newer("n'importe quoi", "0.2.1"));
        assert!(is_newer("0.3.0", "0.3.0-beta.1"));
        assert!(!is_newer("0.3.0-beta.1", "0.3.0"));
    }

    #[test]
    fn tells_ronnie_its_version() {
        let root = std::env::temp_dir().join(format!("serj-ronnie-{}", std::process::id()));
        let app = root.join("modules/serj/app/Serj.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(root.join("modules/serj/version"), "0.2.1").unwrap();
        std::fs::write(root.join("modules/serj/module.json"), "{}").unwrap();
        tell_ronnie(&root.join("modules/serj/app/Serj.app"), "0.3.0");
        let version = std::fs::read_to_string(root.join("modules/serj/version")).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(version, "0.3.0");
    }
}
