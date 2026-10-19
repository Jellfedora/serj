//! Les infos des albums, cherchées en tâche de fond : le nom du fichier tout de
//! suite, puis en ligne quand c'est possible (catalogue de la BnF, Google Books
//! pour le résumé, Wikipédia pour la série, AniList pour les mangas). Gardées
//! dans le cache : chaque album n'est cherché qu'une fois.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use serj::meta::{self, Author, Guess, Info, Record};

/// Au plus une requête par service toutes les… (pour rester poli).
const PAUSE: Duration = Duration::from_millis(350);
/// Un résumé introuvable faute de quota est recherché de nouveau le lendemain.
const RETRY_SECS: u64 = 24 * 3600;
const USER_AGENT: &str = concat!(
    "Serj/",
    env!("CARGO_PKG_VERSION"),
    " (lecteur de BD ; https://github.com/Jellfedora/serj)"
);

struct Job {
    path: PathBuf,
    guess: Guess,
    manga: bool,
    /// Chercher de nouveau même si le cache a une réponse.
    force: bool,
}

pub struct Infos {
    map: HashMap<PathBuf, Info>,
    /// Albums dont la recherche n'est pas finie.
    pending: HashSet<PathBuf>,
    requests: Sender<Job>,
    results: Receiver<(PathBuf, Info)>,
}

impl Infos {
    pub fn new(repaint: egui::Context) -> Self {
        let (requests, inbox) = channel::<Job>();
        let (outbox, results) = channel();
        std::thread::Builder::new()
            .name("serj-infos".into())
            .spawn(move || {
                let mut worker = Worker::new();
                for job in inbox {
                    let info = worker.run(&job);
                    let _ = outbox.send((job.path, info));
                    repaint.request_repaint();
                }
            })
            .expect("thread des infos");
        Self {
            map: HashMap::new(),
            pending: HashSet::new(),
            requests,
            results,
        }
    }

    /// Range les infos arrivées.
    pub fn receive(&mut self) {
        while let Ok((path, info)) = self.results.try_recv() {
            // Une réponse à une ancienne recherche (corrigée depuis) : ignorée.
            if self.map.get(&path).is_some_and(|i| i.query != info.query) {
                continue;
            }
            self.pending.remove(&path);
            self.map.insert(path, info);
        }
    }

    /// Les infos de `path` : tout de suite ce que dit le nom du fichier (ou la
    /// correction), puis ce qu'on trouve en ligne. `manga` et `correction`
    /// changés : on cherche de nouveau.
    pub fn get(&mut self, path: &Path, manga: bool, correction: Option<&Guess>) -> &Info {
        let guess = correction.cloned().unwrap_or_else(|| file_guess(path));
        let stale = self
            .map
            .get(path)
            .is_none_or(|i| i.manga != manga || i.query != guess);
        if stale {
            let force = self.map.contains_key(path);
            let mut info = Info::from_guess(&guess);
            info.manga = manga;
            self.map.insert(path.to_path_buf(), info);
            self.pending.insert(path.to_path_buf());
            let _ = self.requests.send(Job {
                path: path.to_path_buf(),
                guess,
                manga,
                force,
            });
        }
        &self.map[path]
    }

    /// Cherche de nouveau en ligne, sans le cache.
    pub fn refresh(&mut self, path: &Path, manga: bool, correction: Option<&Guess>) {
        let guess = correction.cloned().unwrap_or_else(|| file_guess(path));
        self.pending.insert(path.to_path_buf());
        let _ = self.requests.send(Job {
            path: path.to_path_buf(),
            guess,
            manga,
            force: true,
        });
    }

    /// La recherche de `path` est en cours.
    pub fn pending(&self, path: &Path) -> bool {
        self.pending.contains(path)
    }
}

/// Cherche les infos de `path` tout de suite, sans le cache (`serj --infos`).
pub fn lookup_now(path: &Path, manga: bool) -> Info {
    Worker::new().lookup(&file_guess(path), manga)
}

/// Ce que dit le nom du fichier.
pub fn file_guess(path: &Path) -> Guess {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    meta::guess(&stem)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Le fichier de cache des infos de `path`.
fn cache_file(path: &Path) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let dirs = directories::ProjectDirs::from("", "", "Serj")?;
    Some(
        dirs.cache_dir()
            .join("infos")
            .join(format!("{:016x}.json", hasher.finish())),
    )
}

/// Une requête n'a pas abouti : pas de réseau, ou service saturé.
enum Failure {
    Offline,
    Busy,
}

struct Worker {
    agent: ureq::Agent,
    /// La présentation de chaque série, cherchée une seule fois.
    series: HashMap<String, Option<(String, String)>>,
}

impl Worker {
    fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        Self {
            agent,
            series: HashMap::new(),
        }
    }

    fn run(&mut self, job: &Job) -> Info {
        let cached = cache_file(&job.path).and_then(|f| {
            let bytes = std::fs::read(f).ok()?;
            serde_json::from_slice::<Info>(&bytes).ok()
        });
        if let Some(info) = &cached
            && !job.force
            && info.searched
            && info.manga == job.manga
            && info.query == job.guess
            && info.retry_at.is_none_or(|t| now() < t)
        {
            return info.clone();
        }
        let info = self.lookup(&job.guess, job.manga);
        if info.searched
            && let Some(file) = cache_file(&job.path)
        {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_vec_pretty(&info) {
                let _ = std::fs::write(file, json);
            }
        }
        // Hors ligne : ce qu'on avait déjà vaut mieux que le seul nom du fichier.
        match cached {
            Some(old) if !info.searched && old.query == job.guess => old,
            _ => info,
        }
    }

    fn lookup(&mut self, guess: &Guess, manga: bool) -> Info {
        let mut info = Info::from_guess(guess);
        info.manga = manga;
        let mut reached = false;

        // Le catalogue de la BnF : titre exact, auteurs, éditeur, année, ISBN.
        let mut isbns = Vec::new();
        match self.bnf(guess, manga) {
            Ok(found) => {
                reached = true;
                if let Some(f) = found {
                    // La série et le numéro du nom de fichier sont plus sûrs que
                    // ceux d'une notice (« One piece magazine. Vol »).
                    info.series = info.series.or(f.series);
                    info.volume = info.volume.or(f.volume);
                    info.title = f.title.or(info.title);
                    if !f.authors.is_empty() {
                        info.authors = f.authors;
                    }
                    info.publisher = f.publisher;
                    info.year = f.year;
                    info.pages = f.pages;
                    info.isbn = f.isbns.first().cloned();
                    isbns = f.isbns;
                    if let Some(link) = f.link {
                        info.sources.push(("BnF".into(), link));
                    }
                }
            }
            Err(Failure::Offline | Failure::Busy) => {}
        }

        // Le résumé de l'album : Google Books, par ISBN.
        for isbn in isbns.iter().take(3) {
            match self.google(isbn) {
                Ok(Some((summary, link))) => {
                    info.summary = Some(summary);
                    info.sources.push(("Google Books".into(), link));
                    break;
                }
                Ok(None) => reached = true,
                Err(Failure::Busy) => {
                    reached = true;
                    info.retry_at = Some(now() + RETRY_SECS);
                    break;
                }
                Err(Failure::Offline) => break,
            }
        }

        // Pour un manga : AniList (résumé, genres, auteurs).
        if manga && let Ok(found) = self.anilist(guess) {
            reached = true;
            if let Some(a) = found {
                info.series_summary = a.summary;
                info.genres = a.genres;
                if info.authors.is_empty() {
                    info.authors = a.authors;
                }
                info.year = info.year.or(a.year);
                if info.series.is_none() {
                    info.series = a.title;
                }
                info.sources.push(("AniList".into(), a.link));
            }
        }

        // La série présentée par Wikipédia.
        if let Some(name) = info.series.clone().or_else(|| info.title.clone()) {
            match self.wikipedia(&name, manga) {
                Ok(Some((extract, link))) => {
                    reached = true;
                    info.series_summary.get_or_insert(extract);
                    info.sources.push(("Wikipédia".into(), link));
                }
                Ok(None) => reached = true,
                Err(_) => {}
            }
        }

        info.searched = reached;
        info
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, Failure> {
        std::thread::sleep(PAUSE);
        let mut request = self.agent.get(url);
        for (k, v) in query {
            request = request.query(*k, *v);
        }
        match request.call() {
            Ok(mut response) => response
                .body_mut()
                .read_to_string()
                .map_err(|_| Failure::Offline),
            Err(ureq::Error::StatusCode(403 | 429 | 503)) => Err(Failure::Busy),
            Err(ureq::Error::StatusCode(_)) => Ok(String::new()),
            Err(_) => Err(Failure::Offline),
        }
    }

    /// Les notices de la BnF, de plus en plus larges jusqu'à trouver l'album.
    fn bnf(&self, g: &Guess, manga: bool) -> Result<Option<meta::Found>, Failure> {
        let clean = |s: &str| s.replace('"', " ");
        let author = g
            .authors
            .first()
            .and_then(|a| a.split_whitespace().last())
            .map(|surname| format!(" and bib.author all \"{}\"", clean(surname)))
            .unwrap_or_default();
        let mut queries = Vec::new();
        match (&g.series, g.volume) {
            (Some(s), Some(v)) => queries.push(format!(
                "bib.title all \"{}\" and bib.title all \"{v}\"",
                clean(s)
            )),
            (Some(s), None) => queries.push(format!("bib.title all \"{}\"", clean(s))),
            _ => {}
        }
        if let Some(title) = &g.title {
            let prefix = g.series.as_deref().map(clean).unwrap_or_default();
            for word in meta::keywords(title).iter().take(2) {
                queries.push(format!("bib.title all \"{prefix} {word}\""));
            }
            if g.series.is_none() {
                queries.push(format!("bib.title all \"{}\"", clean(title)));
            }
        }
        let mut records: Vec<Record> = Vec::new();
        let mut best = None;
        for q in queries {
            let xml = self.get(
                "https://catalogue.bnf.fr/api/SRU",
                &[
                    ("version", "1.2"),
                    ("operation", "searchRetrieve"),
                    ("recordSchema", "dublincore"),
                    ("maximumRecords", "40"),
                    ("query", &format!("{q}{author}")),
                ],
            )?;
            records.extend(meta::parse_bnf(&xml));
            best = meta::best_bnf(&records, g, manga);
            // Titre et numéro retrouvés : inutile d'élargir.
            if best
                .as_ref()
                .is_some_and(|f| f.title.is_some() && (g.volume.is_none() || f.volume == g.volume))
            {
                break;
            }
        }
        Ok(best)
    }

    /// Le résumé de l'album sur Google Books, et la page du livre.
    fn google(&self, isbn: &str) -> Result<Option<(String, String)>, Failure> {
        let body = self.get(
            "https://www.googleapis.com/books/v1/volumes",
            &[("q", &format!("isbn:{isbn}"))],
        )?;
        let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let Some(item) = v["items"].get(0) else {
            return Ok(None);
        };
        let summary = item["volumeInfo"]["description"]
            .as_str()
            .map(meta::strip_html)
            .filter(|s| s.len() > 40);
        let link = item["volumeInfo"]["infoLink"]
            .as_str()
            .unwrap_or("https://books.google.com")
            .to_string();
        Ok(summary.map(|s| (s, link)))
    }

    /// L'introduction de l'article Wikipédia de la série.
    fn wikipedia(&mut self, name: &str, manga: bool) -> Result<Option<(String, String)>, Failure> {
        let key = meta::normalize(name);
        if let Some(found) = self.series.get(&key) {
            return Ok(found.clone());
        }
        let api = "https://fr.wikipedia.org/w/api.php";
        let kind = if manga { "manga" } else { "bande dessinée" };
        let body = self.get(
            api,
            &[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", &format!("{name} {kind}")),
                ("srlimit", "5"),
                ("format", "json"),
            ],
        )?;
        let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        // Un article dont le titre est la série (« Lanfeust de Troy »,
        // « Naruto (manga) »), pas une page qui la cite.
        let page = v["query"]["search"].as_array().and_then(|results| {
            results
                .iter()
                .filter_map(|r| r["title"].as_str())
                .find(|t| {
                    let t = meta::normalize(t.split(" (").next().unwrap_or(t));
                    t == key
                })
        });
        let found = match page {
            Some(title) => {
                let body = self.get(
                    api,
                    &[
                        ("action", "query"),
                        ("prop", "extracts|info"),
                        ("exintro", "1"),
                        ("explaintext", "1"),
                        ("inprop", "url"),
                        ("redirects", "1"),
                        ("titles", title),
                        ("format", "json"),
                    ],
                )?;
                let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                v["query"]["pages"].as_object().and_then(|pages| {
                    let p = pages.values().next()?;
                    let extract = p["extract"].as_str()?.trim();
                    let link = p["fullurl"].as_str()?.to_string();
                    (extract.len() > 40).then(|| (shorten(extract, 1400), link))
                })
            }
            None => None,
        };
        self.series.insert(key, found.clone());
        Ok(found)
    }

    fn anilist(&self, g: &Guess) -> Result<Option<Manga>, Failure> {
        let Some(name) = g.series.as_ref().or(g.title.as_ref()) else {
            return Ok(None);
        };
        std::thread::sleep(PAUSE);
        let query = "query($s:String){Media(search:$s,type:MANGA){title{romaji english} \
            description(asHtml:false) genres startDate{year} siteUrl \
            staff(perPage:4){edges{role node{name{full}}}}}}";
        let body = serde_json::json!({ "query": query, "variables": { "s": name } });
        let response = self
            .agent
            .post("https://graphql.anilist.co")
            .header("Accept", "application/json")
            .send_json(&body);
        let v: Value = match response {
            Ok(mut r) => r.body_mut().read_json().map_err(|_| Failure::Offline)?,
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(ureq::Error::StatusCode(429)) => return Err(Failure::Busy),
            Err(ureq::Error::StatusCode(_)) => return Ok(None),
            Err(_) => return Err(Failure::Offline),
        };
        let m = &v["data"]["Media"];
        if m.is_null() {
            return Ok(None);
        }
        let title = m["title"]["romaji"]
            .as_str()
            .or(m["title"]["english"].as_str())
            .map(str::to_string);
        // Un autre manga au nom voisin : on n'en veut pas.
        let same = [&m["title"]["romaji"], &m["title"]["english"]]
            .iter()
            .filter_map(|t| t.as_str())
            .any(|t| meta::normalize(t).contains(&meta::normalize(name)));
        if !same {
            return Ok(None);
        }
        let authors = m["staff"]["edges"]
            .as_array()
            .map(|edges| {
                edges
                    .iter()
                    // Les auteurs, pas les traducteurs ni les assistants.
                    .filter(|e| {
                        e["role"].as_str().is_some_and(|r| {
                            r.contains("Story") || r.contains("Art") || r.contains("Original")
                        })
                    })
                    .filter_map(|e| {
                        Some(Author {
                            name: e["node"]["name"]["full"].as_str()?.to_string(),
                            role: e["role"].as_str().map(|r| {
                                match r {
                                    "Story & Art" => "Scénario et dessin",
                                    "Story" => "Scénario",
                                    "Art" => "Dessin",
                                    other => other,
                                }
                                .to_string()
                            }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(Manga {
            title,
            summary: m["description"].as_str().map(meta::strip_html),
            genres: m["genres"]
                .as_array()
                .map(|g| {
                    g.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            year: m["startDate"]["year"].as_u64().map(|y| y as u32),
            authors,
            link: m["siteUrl"]
                .as_str()
                .unwrap_or("https://anilist.co")
                .to_string(),
        }))
    }
}

struct Manga {
    title: Option<String>,
    summary: Option<String>,
    genres: Vec<String>,
    year: Option<u32>,
    authors: Vec<Author>,
    link: String,
}

/// Les premiers paragraphes de `text`, jusqu'à `max` caractères environ.
fn shorten(text: &str, max: usize) -> String {
    let mut out = String::new();
    for paragraph in text.split('\n').map(str::trim).filter(|p| !p.is_empty()) {
        if !out.is_empty() && out.chars().count() + paragraph.chars().count() > max {
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(paragraph);
    }
    out
}
