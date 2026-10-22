//! Envoyer un livre vers sa Kindle : Amazon le reçoit par e-mail, en pièce
//! jointe, à l'adresse `…@kindle.com` de la liseuse, depuis une adresse
//! approuvée dans le compte Amazon. Serj l'envoie par SMTP avec le compte de
//! la personne ; le mot de passe reste dans le trousseau du système.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::message::{Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Amazon refuse les pièces jointes plus lourdes.
pub const MAX_BYTES: u64 = 50_000_000;
/// Où approuver l'adresse d'envoi, et trouver celle de la Kindle.
pub const AMAZON_SETTINGS: &str = "https://www.amazon.fr/hz/mycd/preferences/myx#/home/settings/";
/// Les envois gardés dans l'historique.
const MAX_HISTORY: usize = 200;

/// La sécurité de la connexion au serveur d'envoi.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Connexion ordinaire, chiffrée ensuite (port 587).
    #[default]
    StartTls,
    /// Chiffrée d'emblée (port 465).
    Tls,
    /// Pas de chiffrement (un serveur local, pour essayer).
    None,
}

/// Les réglages d'envoi.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// L'adresse de la Kindle (`…@kindle.com`).
    pub kindle: String,
    /// L'adresse d'envoi, approuvée chez Amazon.
    pub from: String,
    pub server: String,
    pub port: u16,
    pub security: Security,
    /// L'identifiant du serveur ; l'adresse d'envoi s'il est vide.
    pub login: String,
}

/// Des serveurs connus, pour remplir les réglages d'un clic : nom, serveur,
/// port, sécurité, et où créer un mot de passe d'application.
pub const PROVIDERS: [(&str, &str, u16, Security, &str); 2] = [
    (
        "Gmail",
        "smtp.gmail.com",
        587,
        Security::StartTls,
        "https://myaccount.google.com/apppasswords",
    ),
    (
        "iCloud",
        "smtp.mail.me.com",
        587,
        Security::StartTls,
        "https://account.apple.com/account/manage",
    ),
];

impl Settings {
    pub fn login(&self) -> &str {
        if self.login.trim().is_empty() {
            self.from.trim()
        } else {
            self.login.trim()
        }
    }

    /// De quoi envoyer : les deux adresses et le serveur.
    pub fn ready(&self) -> bool {
        self.kindle.contains('@') && self.from.contains('@') && !self.server.trim().is_empty()
    }

    /// Le serveur que vise l'adresse d'envoi, s'il est connu.
    pub fn guess_provider(from: &str) -> Option<usize> {
        let domain = from.rsplit('@').next()?.to_lowercase();
        match domain.as_str() {
            "gmail.com" | "googlemail.com" => Some(0),
            "icloud.com" | "me.com" | "mac.com" => Some(1),
            _ => None,
        }
    }
}

/// Un envoi, pour l'historique.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sent {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    pub to: String,
    /// Secondes depuis 1970.
    pub at: u64,
    pub status: Status,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Sending,
    Sent,
    Failed(String),
}

/// Les formats qu'Amazon accepte par e-mail, parmi ceux que Serj lit (pas
/// MOBI ni AZW3 depuis 2022, ni FB2, CBZ, CBR).
pub fn accepted(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        ["epub", "pdf", "txt"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

/// Pourquoi on ne peut pas envoyer `path`, s'il y a une raison.
pub fn refusal(path: &Path) -> Option<String> {
    if !accepted(path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        return Some(format!(
            "Amazon n'accepte pas le format {ext} par e-mail (EPUB, PDF et TXT seulement)"
        ));
    }
    match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_BYTES => {
            Some("Trop lourd pour Amazon : 50 Mo au plus par envoi".into())
        }
        Ok(_) => None,
        Err(_) => Some("Fichier introuvable".into()),
    }
}

// ---------------------------------------------------------------------------
// Le mot de passe, dans le trousseau.

fn entry(settings: &Settings) -> Option<keyring::Entry> {
    let user = format!("{}@{}", settings.login(), settings.server.trim());
    // Pas le mot de passe de la version installée.
    let service = match crate::dirs::DEV {
        true => "Serj dev (envoi vers la Kindle)",
        false => "Serj (envoi vers la Kindle)",
    };
    keyring::Entry::new(service, &user).ok()
}

pub fn password(settings: &Settings) -> Option<String> {
    entry(settings)?.get_password().ok()
}

pub fn set_password(settings: &Settings, password: &str) -> Result<(), String> {
    let entry = entry(settings).ok_or("Trousseau indisponible")?;
    entry
        .set_password(password)
        .map_err(|e| format!("Le trousseau a refusé le mot de passe ({e})"))
}

pub fn forget_password(settings: &Settings) {
    if let Some(entry) = entry(settings) {
        let _ = entry.delete_credential();
    }
}

// ---------------------------------------------------------------------------
// L'envoi, en tâche de fond.

enum Job {
    Send {
        id: u64,
        path: PathBuf,
        title: String,
        settings: Settings,
    },
    Test {
        settings: Settings,
    },
}

/// Une réponse du fil d'envoi.
pub enum Done {
    Sent { id: u64, result: Result<(), String> },
    Tested(Result<(), String>),
}

pub struct Mailer {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

impl Mailer {
    pub fn new(repaint: egui::Context) -> Self {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, done) = channel();
        std::thread::Builder::new()
            .name("serj-kindle".into())
            .spawn(move || {
                for job in inbox {
                    let answer = match job {
                        Job::Send {
                            id,
                            path,
                            title,
                            settings,
                        } => Done::Sent {
                            id,
                            result: send(&path, &title, &settings),
                        },
                        Job::Test { settings } => Done::Tested(test(&settings)),
                    };
                    let _ = outbox.send(answer);
                    repaint.request_repaint();
                }
            })
            .expect("thread des envois");
        Self { jobs, done }
    }

    /// Envoie `path` ; la réponse arrive dans [`Self::done`] sous `id`.
    pub fn send(&self, id: u64, path: &Path, title: &str, settings: &Settings) {
        let _ = self.jobs.send(Job::Send {
            id,
            path: path.to_path_buf(),
            title: title.to_string(),
            settings: settings.clone(),
        });
    }

    /// Essaie de se connecter au serveur, mot de passe compris.
    pub fn test(&self, settings: &Settings) {
        let _ = self.jobs.send(Job::Test {
            settings: settings.clone(),
        });
    }

    pub fn done(&self) -> Vec<Done> {
        self.done.try_iter().collect()
    }
}

/// Ajoute un envoi en tête de l'historique.
pub fn record(history: &mut Vec<Sent>, sent: Sent) {
    history.insert(0, sent);
    history.truncate(MAX_HISTORY);
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// « à l'instant », « il y a 5 min », « hier »… ou la date.
pub fn ago(at: u64, now: u64) -> String {
    let s = now.saturating_sub(at);
    match s {
        0..60 => "à l'instant".into(),
        60..3600 => format!("il y a {} min", s / 60),
        3600..86_400 => format!("il y a {} h", s / 3600),
        86_400..172_800 => "hier".into(),
        _ if s < 7 * 86_400 => format!("il y a {} jours", s / 86_400),
        _ => {
            let (y, m, d) = civil(at / 86_400);
            format!("le {d:02}/{m:02}/{y}")
        }
    }
}

/// Le jour (année, mois, jour) à `days` jours du 1er janvier 1970.
fn civil(days: u64) -> (i64, u32, u32) {
    // Howard Hinnant, « chrono-Compatible Low-Level Date Algorithms ».
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn transport(settings: &Settings) -> Result<SmtpTransport, String> {
    let server = settings.server.trim();
    let builder = match settings.security {
        Security::StartTls => SmtpTransport::starttls_relay(server),
        Security::Tls => SmtpTransport::relay(server),
        Security::None => Ok(SmtpTransport::builder_dangerous(server)),
    }
    .map_err(|e| format!("Serveur d'envoi invalide ({e})"))?;
    let mut builder = builder
        .port(settings.port)
        .timeout(Some(Duration::from_secs(90)));
    if let Some(password) = password(settings).filter(|p| !p.is_empty()) {
        builder = builder.credentials(Credentials::new(settings.login().to_string(), password));
    }
    Ok(builder.build())
}

/// L'erreur du serveur, en clair.
fn explain(e: &lettre::transport::smtp::Error, settings: &Settings) -> String {
    let text = e.to_string();
    if text.contains("535") || text.contains("534") || text.to_lowercase().contains("auth") {
        "Identifiant ou mot de passe refusé par le serveur (Gmail et iCloud demandent un \
         mot de passe d'application)"
            .into()
    } else if e.is_timeout() {
        format!("Le serveur {} ne répond pas", settings.server.trim())
    } else if e.is_tls() {
        format!("Connexion chiffrée impossible ({text})")
    } else if e.is_permanent() {
        format!("Refusé par le serveur ({text})")
    } else {
        format!(
            "Impossible de joindre {}:{} ({text})",
            settings.server.trim(),
            settings.port
        )
    }
}

fn test(settings: &Settings) -> Result<(), String> {
    let transport = transport(settings)?;
    match transport.test_connection() {
        Ok(true) => Ok(()),
        Ok(false) => Err("Le serveur n'a pas accepté la connexion".into()),
        Err(e) => Err(explain(&e, settings)),
    }
}

fn send(path: &Path, title: &str, settings: &Settings) -> Result<(), String> {
    if let Some(why) = refusal(path) {
        return Err(why);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Impossible de lire le livre ({e})"))?;
    let kind = match path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("epub") => "application/epub+zip",
        Some("pdf") => "application/pdf",
        _ => "text/plain; charset=utf-8",
    };
    let content_type = ContentType::parse(kind).map_err(|e| e.to_string())?;
    let address = |s: &str, what: &str| {
        s.trim()
            .parse()
            .map_err(|_| format!("Adresse {what} invalide : {}", s.trim()))
    };
    let message = Message::builder()
        .from(address(&settings.from, "d'envoi")?)
        .to(address(&settings.kindle, "de la Kindle")?)
        .subject(title)
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(format!(
                    "« {title} », envoyé depuis Serj."
                )))
                .singlepart(Attachment::new(attachment_name(path)).body(bytes, content_type)),
        )
        .map_err(|e| format!("Message impossible à construire ({e})"))?;
    transport(settings)?
        .send(&message)
        .map(|_| ())
        .map_err(|e| explain(&e, settings))
}

/// Le nom de la pièce jointe, sans accents ni signes : Amazon s'en sert pour
/// le titre si le livre n'en a pas, et certains serveurs abîment le reste.
fn attachment_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "livre".into());
    let ascii: String = stem
        .nfd()
        .filter(|c| c.is_ascii())
        .map(|c| {
            if c.is_ascii_alphanumeric() || " -_.()".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let ascii = ascii.split_whitespace().collect::<Vec<_>>().join(" ");
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    format!(
        "{}.{extension}",
        if ascii.is_empty() { "livre" } else { &ascii }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_formats_and_names() {
        assert!(accepted(Path::new("a.EPUB")));
        assert!(!accepted(Path::new("a.mobi")));
        assert!(refusal(Path::new("a.cbz")).unwrap().contains("CBZ"));
        assert_eq!(
            attachment_name(Path::new(
                "/x/Harry Potter - Tome 1 - L'École des sorciers.epub"
            )),
            "Harry Potter - Tome 1 - L Ecole des sorciers.epub"
        );
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_747), (2026, 10, 21));
        assert_eq!(ago(1000, 1030), "à l'instant");
        assert_eq!(ago(0, 90_000), "hier");
    }

    /// Envoie vraiment un livre, vers un serveur de test local (la boîte mail
    /// de test de Ronnie) : `SERJ_SMTP_TEST=livre.epub cargo test --bin serj envoie`.
    #[test]
    fn envoie() {
        let Some(path) = std::env::var_os("SERJ_SMTP_TEST") else {
            return;
        };
        let settings = Settings {
            kindle: "liseuse@kindle.com".into(),
            from: "moi@exemple.fr".into(),
            server: "127.0.0.1".into(),
            port: 1025,
            security: Security::None,
            login: String::new(),
        };
        test(&settings).unwrap();
        send(Path::new(&path), "Un livre d'essai", &settings).unwrap();
    }
}
