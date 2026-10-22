//! Les infos des albums, cherchées en tâche de fond : le nom du fichier (ou ce
//! qu'un livre en texte dit de lui-même) tout de suite, puis en ligne quand
//! c'est possible (catalogue de la BnF, Wikipédia pour la série, AniList pour
//! les mangas). Gardées dans le cache : chaque album n'est cherché qu'une fois.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::Value;
use serj::meta::{self, Author, Guess, Info, Record};

/// Au plus une requête par service toutes les… (pour rester poli).
const PAUSE: Duration = Duration::from_millis(350);
const USER_AGENT: &str = concat!(
    "Serj/",
    env!("CARGO_PKG_VERSION"),
    " (lecteur de BD ; https://github.com/Jellfedora/serj)"
);

struct Job {
    path: PathBuf,
    guess: Guess,
    manga: bool,
    livre: bool,
    /// La recherche part d'infos corrigées à la main : elles priment sur
    /// celles du catalogue.
    corrected: bool,
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

    /// Range les infos arrivées ; renvoie les albums concernés.
    pub fn receive(&mut self) -> Vec<PathBuf> {
        let mut arrived = Vec::new();
        while let Ok((path, info)) = self.results.try_recv() {
            // Une réponse à une ancienne recherche (corrigée depuis) : ignorée.
            if self.map.get(&path).is_some_and(|i| i.query != info.query) {
                continue;
            }
            self.pending.remove(&path);
            self.map.insert(path.clone(), info);
            arrived.push(path);
        }
        arrived
    }

    /// Les infos déjà connues de `path`, sans en demander.
    pub fn known(&self, path: &Path) -> Option<&Info> {
        self.map.get(path)
    }

    /// Le fichier `from` a été renommé `to`, d'après ses infos : elles le
    /// suivent, sans nouvelle recherche (`query` : ce que dit le nouveau nom).
    pub fn moved(&mut self, from: &Path, to: &Path, query: Guess) {
        self.pending.remove(from);
        if let Some(mut info) = self.map.remove(from) {
            info.query = query;
            save(to, &info);
            if let Some(old) = cache_file(from) {
                let _ = std::fs::remove_file(old);
            }
            self.map.insert(to.to_path_buf(), info);
        }
    }

    /// Les infos de `path` : tout de suite ce que dit le nom du fichier (ou la
    /// correction), puis ce qu'on trouve en ligne. `manga`, `livre` et
    /// `correction` changés : on cherche de nouveau.
    pub fn get(
        &mut self,
        path: &Path,
        manga: bool,
        livre: bool,
        correction: Option<&Guess>,
    ) -> &Info {
        let guess = correction.cloned().unwrap_or_else(|| file_guess(path));
        let stale = self
            .map
            .get(path)
            .is_none_or(|i| i.manga != manga || i.livre != livre || i.query != guess);
        if stale {
            let force = self.map.contains_key(path);
            let mut info = Info::from_guess(&guess);
            info.manga = manga;
            info.livre = livre;
            info.summary = embedded(path).and_then(|m| m.description);
            self.map.insert(path.to_path_buf(), info);
            self.pending.insert(path.to_path_buf());
            let _ = self.requests.send(Job {
                path: path.to_path_buf(),
                guess,
                manga,
                livre,
                // Un livre qui dit son titre et ses auteurs : comme corrigé à la main.
                corrected: correction.is_some()
                    || embedded(path).is_some_and(|m| m.title.is_some()),
                force,
            });
        }
        &self.map[path]
    }

    /// Les infos du tome `volume` de la série `series`, que la bibliothèque n'a
    /// pas : cherchées comme celles d'un album, sous une clé à part.
    pub fn volume(
        &mut self,
        series: &str,
        volume: u32,
        manga: bool,
        livre: bool,
    ) -> (PathBuf, &Info) {
        let key = volume_key(series, volume);
        let guess = Guess {
            series: Some(series.to_string()),
            volume: Some(volume),
            ..Guess::default()
        };
        self.get(&key, manga, livre, Some(&guess));
        (key.clone(), &self.map[&key])
    }

    /// Cherche de nouveau en ligne, sans le cache.
    pub fn refresh(&mut self, path: &Path, manga: bool, livre: bool, correction: Option<&Guess>) {
        let guess = correction.cloned().unwrap_or_else(|| file_guess(path));
        self.pending.insert(path.to_path_buf());
        let _ = self.requests.send(Job {
            path: path.to_path_buf(),
            guess,
            manga,
            livre,
            corrected: correction.is_some() || embedded(path).is_some_and(|m| m.title.is_some()),
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
    let livre = serj::ebook::is_ebook(path);
    let mut info = Worker::new().lookup(&file_guess(path), manga, livre, livre);
    info.summary = embedded(path).and_then(|m| m.description);
    info
}

/// La clé d'un tome qu'on n'a pas (aucun fichier ne porte ce nom).
pub fn volume_key(series: &str, volume: u32) -> PathBuf {
    PathBuf::from(format!("serj-tome-manquant:{}:{volume}", meta::normalize(series)))
}

/// Ce que dit le nom du fichier ; pour un livre en texte, ce qu'il dit de
/// lui-même (titre, auteurs, série), le nom du fichier sinon.
pub fn file_guess(path: &Path) -> Guess {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let from_name = meta::guess(&stem);
    match embedded(path) {
        Some(m) if m.title.is_some() => Guess {
            series: m.series,
            volume: m.series_index,
            title: m.title,
            authors: if m.authors.is_empty() {
                from_name.authors
            } else {
                m.authors
            },
        },
        _ => from_name,
    }
}

/// Les infos qu'un livre en texte porte en lui, lues une fois par fichier.
pub fn embedded(path: &Path) -> Option<serj::ebook::Metadata> {
    static CACHE: std::sync::LazyLock<
        std::sync::Mutex<HashMap<PathBuf, Option<serj::ebook::Metadata>>>,
    > = std::sync::LazyLock::new(Default::default);
    if !serj::ebook::is_ebook(path) {
        return None;
    }
    if let Some(known) = CACHE.lock().ok()?.get(path) {
        return known.clone();
    }
    let found = serj::ebook::metadata(path).map(|(m, _)| m);
    CACHE.lock().ok()?.insert(path.to_path_buf(), found.clone());
    found
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

fn save(path: &Path, info: &Info) {
    let Some(file) = cache_file(path) else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(info) {
        let _ = std::fs::write(file, json);
    }
}

/// Une requête n'a pas abouti : pas de réseau, ou service saturé.
enum Failure {
    Offline,
    Busy,
}

struct Worker {
    agent: ureq::Agent,
    /// L'article de chaque série, cherché une seule fois.
    series: HashMap<String, Option<Article>>,
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
            && info.format >= meta::INFO_FORMAT
            && info.manga == job.manga
            && info.livre == job.livre
            && info.query == job.guess
        {
            return info.clone();
        }
        let mut info = self.lookup(&job.guess, job.manga, job.livre, job.corrected);
        // Ce que le livre dit de lui-même complète le catalogue.
        if let Some(m) = embedded(&job.path) {
            info.summary = m.description;
            info.publisher = info.publisher.or(m.publisher);
            info.year = info.year.or(m.year);
        }
        if info.searched {
            save(&job.path, &info);
        }
        // Hors ligne : ce qu'on avait déjà vaut mieux que le seul nom du fichier.
        match cached {
            Some(old) if !info.searched && old.query == job.guess => old,
            _ => info,
        }
    }

    fn lookup(&mut self, guess: &Guess, manga: bool, livre: bool, corrected: bool) -> Info {
        let mut info = Info::from_guess(guess);
        info.manga = manga;
        info.livre = livre;
        let mut reached = false;

        // Le catalogue de la BnF : titre exact, auteurs, éditeur, année, ISBN.
        match self.bnf(guess, manga, livre) {
            Ok(found) => {
                reached = true;
                if let Some(f) = found {
                    // La série et le numéro du nom de fichier sont plus sûrs que
                    // ceux d'une notice (« One piece magazine. Vol »).
                    info.series = info.series.or(f.series);
                    info.volume = info.volume.or(f.volume);
                    // Un titre corrigé à la main reste tel quel ; celui du nom
                    // de fichier a souvent perdu ses accents et apostrophes.
                    info.title = if corrected {
                        info.title.or(f.title)
                    } else {
                        f.title.or(info.title)
                    };
                    // Les auteurs du catalogue (avec leur rôle), sauf si on en
                    // a indiqué d'autres.
                    let names = |a: &[Author]| {
                        let mut n: Vec<String> = a.iter().map(|a| meta::normalize(&a.name)).collect();
                        n.sort();
                        n
                    };
                    let others = corrected
                        && !info.authors.is_empty()
                        && names(&info.authors) != names(&f.authors);
                    if !f.authors.is_empty() && !others {
                        info.authors = f.authors;
                    }
                    info.publisher = f.publisher;
                    info.year = f.year;
                    info.pages = f.pages;
                    info.isbn = f.isbns.first().cloned();
                    // La couverture : celle de la notice, sinon d'après l'ISBN.
                    if let Some(ark) = f.link.as_deref().and_then(|l| l.find("ark:").map(|i| &l[i..])) {
                        info.covers.push(format!(
                            "https://catalogue.bnf.fr/couverture?appName=NE&idArk={ark}&couverture=1"
                        ));
                    }
                    for isbn in f.isbns.iter().take(2) {
                        info.covers.push(format!(
                            "https://covers.openlibrary.org/b/isbn/{isbn}-M.jpg?default=false"
                        ));
                    }
                    if let Some(link) = f.link {
                        info.sources.push(("BnF".into(), link));
                    }
                }
            }
            Err(Failure::Offline | Failure::Busy) => {}
        }

        // Pour un manga : AniList (résumé, genres, auteurs).
        if manga
            && !livre
            && let Ok(found) = self.anilist(guess)
        {
            reached = true;
            if let Some(a) = found {
                info.series_summary = a.summary;
                info.genres = a.genres;
                info.series_volumes = a.volumes;
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

        // La série présentée par Wikipédia, et combien elle compte de tomes
        // (pour un livre : sa série seulement, ou le livre lui-même).
        if let Some(name) = info.series.clone().or_else(|| info.title.clone()) {
            let kind = match (livre, manga) {
                (true, _) => "roman",
                (false, true) => "manga",
                (false, false) => "bande dessinée",
            };
            match self.wikipedia(&name, kind) {
                Ok(Some(article)) => {
                    reached = true;
                    if let Some(extract) = article.extract {
                        info.series_summary.get_or_insert(extract);
                    }
                    info.series_volumes = info.series_volumes.max(article.volumes);
                    info.sources.push(("Wikipédia".into(), article.link));
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
    fn bnf(&self, g: &Guess, manga: bool, livre: bool) -> Result<Option<meta::Found>, Failure> {
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
            best = meta::best_bnf(&records, g, manga, livre);
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

    /// L'article Wikipédia de la série : son introduction, et le nombre de
    /// tomes que donne son infobox.
    fn wikipedia(&mut self, name: &str, kind: &str) -> Result<Option<Article>, Failure> {
        let key = meta::normalize(name);
        if let Some(found) = self.series.get(&format!("{kind}:{key}")) {
            return Ok(found.clone());
        }
        let api = "https://fr.wikipedia.org/w/api.php";
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
        // Pas l'adaptation : « Harry Potter (série télévisée) », « (film) »…
        let other_medium = |t: &str| {
            let qualifier = meta::normalize(t.split_once(" (").map_or("", |(_, q)| q));
            [
                "film",
                "serie televisee",
                "telefilm",
                "jeu video",
                "serie danimation",
                "dessin anime",
                "chanson",
                "comedie musicale",
                "piece de theatre",
            ]
            .iter()
            .any(|w| qualifier.contains(w))
        };
        let page = v["query"]["search"].as_array().and_then(|results| {
            results
                .iter()
                .filter_map(|r| r["title"].as_str())
                .find(|t| {
                    let base = meta::normalize(t.split(" (").next().unwrap_or(t));
                    base == key && !other_medium(t)
                })
        });
        let found = match page {
            Some(title) => {
                let body = self.get(
                    api,
                    &[
                        ("action", "query"),
                        ("prop", "extracts|info|revisions"),
                        ("exintro", "1"),
                        ("explaintext", "1"),
                        ("inprop", "url"),
                        ("rvprop", "content"),
                        ("rvslots", "main"),
                        ("rvsection", "0"),
                        ("redirects", "1"),
                        ("titles", title),
                        ("format", "json"),
                    ],
                )?;
                let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                v["query"]["pages"].as_object().and_then(|pages| {
                    let p = pages.values().next()?;
                    let extract = p["extract"]
                        .as_str()
                        .map(str::trim)
                        .filter(|e| e.len() > 40)
                        .map(|e| shorten(&meta::without_pronunciation(e), 1400));
                    let volumes = p["revisions"][0]["slots"]["main"]["*"]
                        .as_str()
                        .and_then(meta::infobox_volumes);
                    (extract.is_some() || volumes.is_some()).then(|| Article {
                        extract,
                        link: p["fullurl"].as_str().unwrap_or(api).to_string(),
                        volumes,
                    })
                })
            }
            None => None,
        };
        self.series.insert(format!("{kind}:{key}"), found.clone());
        Ok(found)
    }

    fn anilist(&self, g: &Guess) -> Result<Option<Manga>, Failure> {
        let Some(name) = g.series.as_ref().or(g.title.as_ref()) else {
            return Ok(None);
        };
        std::thread::sleep(PAUSE);
        let query = "query($s:String){Media(search:$s,type:MANGA){title{romaji english} \
            description(asHtml:false) genres volumes startDate{year} siteUrl \
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
            volumes: m["volumes"].as_u64().map(|v| v as u32).filter(|&v| v > 0),
            authors,
            link: m["siteUrl"]
                .as_str()
                .unwrap_or("https://anilist.co")
                .to_string(),
        }))
    }
}

#[derive(Clone)]
struct Article {
    extract: Option<String>,
    link: String,
    volumes: Option<u32>,
}

struct Manga {
    title: Option<String>,
    summary: Option<String>,
    genres: Vec<String>,
    year: Option<u32>,
    /// Le nombre de tomes, une fois la série finie.
    volumes: Option<u32>,
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
