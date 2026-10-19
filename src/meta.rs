//! Les infos d'un album : ce que dit le nom du fichier, et le choix de la bonne
//! notice parmi celles du catalogue de la BnF. Rien ici ne va sur internet.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Ce que l'on devine d'un album, d'après le nom de son fichier (ou ce que la
/// personne a corrigé).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Guess {
    pub series: Option<String>,
    pub volume: Option<u32>,
    pub title: Option<String>,
    pub authors: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Author {
    pub name: String,
    /// « Scénario », « Dessin »…, quand le catalogue le dit.
    pub role: Option<String>,
}

/// Une notice du catalogue de la BnF (format Dublin Core).
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub title: String,
    pub people: Vec<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub isbn: Option<String>,
    pub format: Option<String>,
    pub link: Option<String>,
    /// Un livre imprimé (et pas un jeu vidéo, un film, un disque…).
    pub printed: bool,
}

/// L'album trouvé au catalogue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub series: Option<String>,
    pub volume: Option<u32>,
    pub title: Option<String>,
    pub authors: Vec<Author>,
    pub publisher: Option<String>,
    /// La plus ancienne des éditions qui correspondent : à peu près la première parution.
    pub year: Option<u32>,
    pub pages: Option<u32>,
    /// Les ISBN des éditions qui correspondent, la meilleure d'abord.
    pub isbns: Vec<String>,
    pub link: Option<String>,
}

/// Tout ce que Serj sait d'un album, pour la bibliothèque et sa fiche.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    pub series: Option<String>,
    pub volume: Option<u32>,
    pub title: Option<String>,
    pub authors: Vec<Author>,
    pub publisher: Option<String>,
    pub year: Option<u32>,
    pub pages: Option<u32>,
    pub isbn: Option<String>,
    pub summary: Option<String>,
    pub series_summary: Option<String>,
    pub genres: Vec<String>,
    /// Combien de tomes compte la série, quand on le sait (AniList).
    pub series_volumes: Option<u32>,
    /// Où télécharger la couverture, dans l'ordre (pour un tome qu'on n'a pas).
    pub covers: Vec<String>,
    /// Où les infos ont été trouvées : (nom de la source, adresse).
    pub sources: Vec<(String, String)>,
    /// La recherche en ligne a pu se faire (même sans rien trouver).
    pub searched: bool,
    /// Pour un manga (AniList), ou une BD.
    pub manga: bool,
    /// Le résumé n'a pas pu être cherché (service saturé) : à refaire après
    /// cette date, en secondes depuis 1970.
    pub retry_at: Option<u64>,
    /// La recherche qui a donné ces infos, pour savoir s'il faut la refaire.
    pub query: Guess,
    /// La version de la recherche ([`INFO_FORMAT`]) : une plus ancienne est
    /// refaite, pour ce qu'elle ne savait pas encore trouver.
    pub format: u32,
}

/// Augmenté quand la recherche trouve de nouvelles infos (1 : le nombre de
/// tomes de la série).
pub const INFO_FORMAT: u32 = 1;

impl Info {
    pub fn from_guess(g: &Guess) -> Self {
        Self {
            series: g.series.clone(),
            volume: g.volume,
            title: g.title.clone(),
            authors: g
                .authors
                .iter()
                .map(|name| Author {
                    name: name.clone(),
                    role: None,
                })
                .collect(),
            query: g.clone(),
            format: INFO_FORMAT,
            ..Self::default()
        }
    }

    /// « Lanfeust de Troy 2 · Thanos l'incongru », ou ce qu'on en sait.
    pub fn full_title(&self) -> Option<String> {
        let series = self.series.as_ref().map(|s| match self.volume {
            Some(v) => format!("{s} {v}"),
            None => s.clone(),
        });
        match (series, &self.title) {
            (Some(s), Some(t)) => Some(format!("{s} · {t}")),
            (Some(s), None) => Some(s),
            (None, Some(t)) => Some(t.clone()),
            (None, None) => None,
        }
    }

    /// Le titre dans la rangée de sa série : « 2. Thanos l'incongru ».
    pub fn short_title(&self) -> Option<String> {
        match (self.volume, &self.title) {
            (Some(v), Some(t)) => Some(format!("{v}. {t}")),
            (Some(v), None) => Some(format!("Tome {v}")),
            (None, Some(t)) => Some(t.clone()),
            (None, None) => None,
        }
    }

    /// Les auteurs, avec leur rôle : « Christophe Arleston (scénario) ».
    pub fn credits(&self) -> String {
        self.authors
            .iter()
            .map(|a| match &a.role {
                Some(r) => format!("{} ({})", a.name, r.to_lowercase()),
                None => a.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Le nom de fichier (sans extension) qui range l'album, par exemple
/// « Lanfeust de Troy - Tome 2 - Thanos l'incongru ». Relu par [`guess`], il
/// redonne la même série, le même tome et le même titre.
pub fn file_name(info: &Info) -> Option<String> {
    let name = match (&info.series, info.volume, &info.title) {
        (Some(s), Some(v), Some(t)) => format!("{s} - Tome {v} - {t}"),
        (Some(s), Some(v), None) => format!("{s} - Tome {v}"),
        (Some(s), None, Some(t)) if normalize(s) != normalize(t) => format!("{s} - {t}"),
        (_, _, Some(t)) => t.clone(),
        _ => return None,
    };
    // Sans les caractères interdits dans un nom de fichier (macOS, Windows, Linux).
    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = clean.chars().take(150).collect();
    let clean = clean
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    (!clean.is_empty()).then_some(clean)
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("expression valide")
}

/// « … Tome 2 - Titre », « … T02 », « … vol. 3 », « … n° 4 ».
static MARKED: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)^(.*?)[\s\-–:,.]*\b(?:tome|t|vol|volume|n°|no|#)\.?\s*0*(\d{1,4})\b[\s\-–:,.]*(.*)$")
});
/// « Série - 03 - Titre », « Série - 03 ».
static DASHED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(.*?)\s+[-–]\s+0*(\d{1,4})(?:\s+[-–]\s+(.*))?$"));
/// « Série 12 ».
static TRAILING: LazyLock<Regex> = LazyLock::new(|| re(r"^(.*\D)[\s,]+0*(\d{1,3})$"));
/// Notice de la BnF : « Lanfeust de Troy. 2, Thanos l'incongru ».
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(.+?)\.\s*(?:T\.\s*)?0*(\d{1,4})(?:\s*,\s*(.+))?$"));
static PAGES: LazyLock<Regex> = LazyLock::new(|| re(r"(\d+)(?:-\d+)?\s*p\."));
static VOLUMES: LazyLock<Regex> = LazyLock::new(|| re(r"(\d+)\s*vol\."));
static RANGE: LazyLock<Regex> = LazyLock::new(|| re(r"\.\s*\d+\s*[-à&]\s*\d+"));

/// Ce que dit le nom du fichier `stem` (sans l'extension).
pub fn guess(stem: &str) -> Guess {
    let stem = stem.replace('_', " ");
    let (main, groups) = split_groups(&stem);
    let mut g = Guess::default();
    for group in groups {
        if is_source(&group) {
            continue;
        }
        if let Some((series, volume, rest)) = numbered(&group)
            && rest.is_none()
        {
            g.series.get_or_insert(series);
            g.volume.get_or_insert(volume);
        } else if g.authors.is_empty()
            && let Some(authors) = authors(&group)
        {
            g.authors = authors;
        }
    }
    let main = tidy(&main);
    match numbered(&main) {
        Some((series, volume, title)) if !series.is_empty() => {
            g.series = Some(series);
            g.volume = Some(volume);
            g.title = title;
        }
        _ if !main.is_empty() => g.title = Some(main),
        _ => {}
    }
    g
}

/// Série, numéro, et ce qui suit (le titre de l'album).
fn numbered(s: &str) -> Option<(String, u32, Option<String>)> {
    let caps = MARKED
        .captures(s)
        .or_else(|| DASHED.captures(s))
        .or_else(|| TRAILING.captures(s))?;
    let series = tidy(&caps[1]);
    let volume = caps[2].parse().ok()?;
    let rest = caps
        .get(3)
        .map(|m| tidy(m.as_str()))
        .filter(|t| !t.is_empty());
    Some((series, volume, rest))
}

/// Le texte hors parenthèses et crochets, et le contenu de chacune.
fn split_groups(s: &str) -> (String, Vec<String>) {
    let mut main = String::new();
    let mut groups = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => {
                if depth > 0 {
                    current.push(c);
                }
                depth += 1;
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    groups.push(std::mem::take(&mut current));
                } else {
                    current.push(c);
                }
            }
            _ if depth > 0 => current.push(c),
            _ => main.push(c),
        }
    }
    (main, groups)
}

/// Une mention du site d'où vient le fichier, pas une info sur l'album.
fn is_source(group: &str) -> bool {
    let g = group.to_lowercase();
    [
        "lib", ".sk", ".org", ".com", ".net", ".fr", "www", "scan", "ebook", "cbz", "cbr",
    ]
    .iter()
    .any(|s| g.contains(s))
        || g.trim().chars().all(|c| c.is_ascii_digit())
}

/// « Christophe Arleston, Didier Tarquin etc. »
fn authors(group: &str) -> Option<Vec<String>> {
    if group.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let names: Vec<String> = group
        .split([',', ';', '&'])
        .flat_map(|p| p.split(" et "))
        .map(|p| tidy(p.trim().trim_end_matches("etc.").trim_end_matches("etc")))
        .filter(|p| !p.is_empty())
        .collect();
    let plausible = names.len() <= 6
        && names
            .iter()
            .all(|n| (1..=4).contains(&n.split_whitespace().count()));
    (plausible && !names.is_empty()).then_some(names)
}

/// Espaces resserrés, sans tirets ni ponctuation aux bouts.
fn tidy(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| {
            c == '-' || c == '–' || c == ':' || c == ',' || c == '.' || c.is_whitespace()
        })
        .to_string()
}

/// Pour comparer des titres : minuscules, sans accents ni apostrophes (les noms
/// de fichiers les perdent souvent : « lincongru » pour « l'incongru »).
pub fn normalize(s: &str) -> String {
    let flat: String = s
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .filter(|c| !matches!(c, '\'' | '’' | '`' | 'ʼ'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .to_lowercase();
    flat.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn words(s: &str) -> Vec<String> {
    normalize(s)
        .split(' ')
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect()
}

/// Les mots d'un titre qui le distinguent, les plus longs d'abord, pour
/// interroger le catalogue.
pub fn keywords(title: &str) -> Vec<String> {
    let mut w: Vec<String> = words(title).into_iter().filter(|w| w.len() >= 4).collect();
    w.sort_by_key(|w| std::cmp::Reverse(w.len()));
    w
}

/// Les notices d'une réponse du catalogue de la BnF (SRU, Dublin Core).
pub fn parse_bnf(xml: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<oai_dc:dc") {
        let Some(len) = rest[start..].find("</oai_dc:dc>") else {
            break;
        };
        let body = &rest[start..start + len];
        rest = &rest[start + len..];
        let mut r = Record::default();
        for (tag, value) in dc_fields(body) {
            match tag {
                "title" if r.title.is_empty() => r.title = value,
                "creator" | "contributor" => r.people.push(value),
                "publisher" if r.publisher.is_none() => r.publisher = Some(value),
                "date" if r.date.is_none() => r.date = Some(value),
                "format" if r.format.is_none() => r.format = Some(value),
                "type" if value == "texte imprimé" => r.printed = true,
                "identifier" => {
                    if let Some(isbn) = value.strip_prefix("ISBN ") {
                        r.isbn.get_or_insert(isbn.replace('-', ""));
                    } else if value.starts_with("http") {
                        r.link.get_or_insert(value);
                    }
                }
                _ => {}
            }
        }
        if !r.title.is_empty() && r.printed {
            records.push(r);
        }
    }
    records
}

fn dc_fields(body: &str) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("<dc:") {
        rest = &rest[i + 4..];
        let Some(end_name) = rest.find(['>', ' ']) else {
            break;
        };
        let tag = &rest[..end_name];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let close = format!("</dc:{tag}>");
        let Some(close_at) = rest.find(&close) else {
            break;
        };
        if close_at > open_end {
            out.push((tag, unescape(rest[open_end + 1..close_at].trim())));
        }
        rest = &rest[close_at + close.len()..];
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Le titre d'une notice, en morceaux : série, numéro, titre de l'album.
fn record_parts(title: &str) -> (Option<String>, Option<u32>, String) {
    // Avant la mention des auteurs, sans les précisions d'édition.
    let mut main = title.split(" / ").next().unwrap_or(title);
    // « Thanos l'incongru (Nouv. éd. recolorisée) scénario … » : les auteurs
    // suivent la mention d'édition, sans « / ».
    if let Some(i) = main.find(" (").or_else(|| main.find(" [")) {
        main = &main[..i];
    }
    let main = tidy(main);
    if let Some(caps) = NUMBERED.captures(&main) {
        let title = caps.get(3).map(|m| tidy(m.as_str())).unwrap_or_default();
        return (Some(tidy(&caps[1])), caps[2].parse().ok(), title);
    }
    // « Lanfeust de Troy : Thanos l'incongru », « One piece. Romance dawn ».
    if let Some((series, title)) = main.split_once(" : ").or_else(|| main.split_once(". ")) {
        return (Some(tidy(series)), None, tidy(title));
    }
    (None, None, main)
}

/// « Arleston, Christophe (1963-....). Auteur du texte »
fn person(raw: &str) -> Author {
    let (name, role) = match raw.split_once(". ") {
        Some((n, r)) => (n, Some(r)),
        None => (raw, None),
    };
    let name = name.split(" (").next().unwrap_or(name);
    let name = match name.split_once(", ") {
        Some((last, first)) => format!("{first} {last}"),
        None => name.to_string(),
    };
    let role = role.map(|r| {
        match r.trim_end_matches('.') {
            "Auteur du texte" => "Scénario",
            "Illustrateur" => "Dessin",
            "Coloriste" => "Couleurs",
            "Graphiste" => "Graphisme",
            "Traducteur" => "Traduction",
            other => other,
        }
        .to_string()
    });
    Author {
        name: tidy(&name),
        role,
    }
}

/// Ce qu'une notice a de commun avec l'album cherché.
struct Match {
    score: f32,
    /// Une édition ordinaire : ni roman tiré de la BD, ni intégrale, ni coffret.
    ordinary: bool,
    /// Le même album, quelle que soit l'édition : même titre, pas d'autre numéro.
    same: bool,
}

fn score(r: &Record, g: &Guess, manga: bool) -> Match {
    let (series, volume, title) = record_parts(&r.title);
    let whole = normalize(r.title.split(" / ").next().unwrap_or(&r.title));
    let mut score = 0.0;
    let mut same = false;
    if let Some(ours) = g.title.as_deref().map(words).filter(|w| !w.is_empty()) {
        let theirs: HashSet<String> = words(&title).into_iter().collect();
        let hits = ours.iter().filter(|w| theirs.contains(*w)).count();
        score += 4.0 * hits as f32 / ours.len() as f32;
        same = hits == ours.len();
        // Un titre de la notice bien plus long que le nôtre : un autre album.
        if theirs.len() > 2 * ours.len() + 2 {
            score -= 1.0;
        }
    } else if let Some(s) = &g.series
        && g.volume.is_none()
        && normalize(&title) == normalize(s)
    {
        score += 3.0;
    }
    match (g.volume, volume) {
        (Some(a), Some(b)) if a == b => {
            score += 2.0;
            same |= g.title.is_none();
        }
        (Some(_), Some(_)) => {
            score -= 4.0;
            same = false;
        }
        _ => {}
    }
    if let Some(s) = &g.series {
        let s = normalize(s);
        if whole.contains(&s) || series.is_some_and(|x| normalize(&x) == s) {
            score += 1.0;
        }
    }
    let format = r.format.as_deref().unwrap_or("");
    let pages = pages(format);
    let several = VOLUMES
        .captures(format)
        .and_then(|c| c[1].parse::<u32>().ok())
        .is_some_and(|n| n > 1);
    let novel = r.title.contains("texte de") || (!manga && pages.is_some_and(|p| p > 120));
    let ordinary = !novel
        && !several
        && !whole.contains("integrale")
        && !whole.contains("collector")
        && !RANGE.is_match(&r.title);
    if !ordinary {
        score -= 5.0;
    }
    Match {
        score,
        ordinary,
        same,
    }
}

fn pages(format: &str) -> Option<u32> {
    PAGES.captures(format).and_then(|c| c[1].parse().ok())
}

fn year(r: &Record) -> Option<u32> {
    r.date
        .as_deref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
        // Une date aberrante dans une notice (une BD avant 1900) n'en est pas une.
        .filter(|&y| y >= 1900)
}

/// La notice qui correspond le mieux à l'album, s'il y en a une assez proche.
pub fn best_bnf(records: &[Record], g: &Guess, manga: bool) -> Option<Found> {
    let mut scored: Vec<(Match, &Record)> =
        records.iter().map(|r| (score(r, g, manga), r)).collect();
    scored.sort_by(|a, b| b.0.score.total_cmp(&a.0.score));
    let (top, best) = scored.first()?;
    let best = *best;
    if top.score < 3.0 {
        return None;
    }
    // Les autres éditions ordinaires du même album : la plus ancienne donne
    // la première parution, et leurs ISBN servent à chercher un résumé.
    let close: Vec<&Record> = scored
        .iter()
        .filter(|(m, _)| m.ordinary && (m.same || m.score >= top.score - 2.0))
        .map(|(_, r)| *r)
        .collect();
    let (series, volume, title) = record_parts(&best.title);
    let mut authors: Vec<Author> = Vec::new();
    for p in &best.people {
        let a = person(p);
        if !a.name.is_empty() && !authors.iter().any(|x| x.name == a.name) {
            authors.push(a);
        }
    }
    let mut isbns: Vec<String> = Vec::new();
    for r in std::iter::once(best).chain(close.iter().copied()) {
        if let Some(i) = &r.isbn
            && !isbns.contains(i)
        {
            isbns.push(i.clone());
        }
    }
    Some(Found {
        series: series.or_else(|| g.series.clone()),
        volume: volume.or(g.volume),
        title: Some(capitalize(&title)).filter(|t| !t.is_empty()),
        authors,
        publisher: best
            .publisher
            .as_deref()
            .map(|p| p.split(" (").next().unwrap_or(p).to_string()),
        year: close
            .iter()
            .filter_map(|r| year(r))
            .min()
            .or_else(|| year(best)),
        pages: pages(best.format.as_deref().unwrap_or("")),
        isbns,
        link: best.link.clone(),
    })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Le nombre de tomes d'une série, d'après l'infobox de son article Wikipédia
/// (« nombre d'albums = 9 » pour une BD, « nb_volumes = 72 » pour un manga).
pub fn infobox_volumes(wikitext: &str) -> Option<u32> {
    static FIELD: LazyLock<Regex> = LazyLock::new(|| {
        re(r"(?im)^\s*\|\s*(?:nombre d'albums|nombre de tomes|nombre de volumes|nb_volumes|volumes)\s*=\D{0,20}?(\d{1,4})")
    });
    FIELD
        .captures(wikitext)
        .and_then(|c| c[1].parse().ok())
        .filter(|&n| n > 0)
}

/// Enlève les balises HTML d'un texte (résumés d'AniList ou de Google Books).
pub fn strip_html(s: &str) -> String {
    let s = s
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = unescape(&out);
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    lines
        .join("\n")
        .split("\n\n\n")
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(
        series: Option<&str>,
        volume: Option<u32>,
        title: Option<&str>,
        authors: &[&str],
    ) -> Guess {
        Guess {
            series: series.map(str::to_string),
            volume,
            title: title.map(str::to_string),
            authors: authors.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn reads_file_names() {
        let arleston = ["Christophe Arleston", "Didier Tarquin"];
        assert_eq!(
            guess(
                "Lanfeust de Troy- Tome 2 - Thanos lincongru (Christophe Arleston, Didier Tarquin etc.) (z-library.sk, 1lib.sk, z-lib.sk)"
            ),
            g(
                Some("Lanfeust de Troy"),
                Some(2),
                Some("Thanos lincongru"),
                &arleston
            )
        );
        assert_eq!(
            guess(
                "Le frisson de lharuspice (Lanfeust de Troy 5) (Christophe Arleston, Didier Tarquin) (z-library.sk, 1lib.sk, z-lib.sk)"
            ),
            g(
                Some("Lanfeust de Troy"),
                Some(5),
                Some("Le frisson de lharuspice"),
                &arleston
            )
        );
        assert_eq!(
            guess(
                "Paladin dEckmul (Lanfeust de Troy, tome 4) (Didier Tarquin, Christophe Arleston) (z-library.sk)"
            ),
            g(
                Some("Lanfeust de Troy"),
                Some(4),
                Some("Paladin dEckmul"),
                &["Didier Tarquin", "Christophe Arleston"]
            )
        );
        assert_eq!(guess("Naruto 12"), g(Some("Naruto"), Some(12), None, &[]));
        assert_eq!(
            guess("Blacksad - 03 - Ame rouge"),
            g(Some("Blacksad"), Some(3), Some("Ame rouge"), &[])
        );
        assert_eq!(
            guess("Asterix_T02_La serpe d'or"),
            g(Some("Asterix"), Some(2), Some("La serpe d'or"), &[])
        );
        assert_eq!(
            guess("One Piece Vol.105"),
            g(Some("One Piece"), Some(105), None, &[])
        );
        assert_eq!(guess("Maus"), g(None, None, Some("Maus"), &[]));
    }

    #[test]
    fn names_files_so_they_read_back() {
        let info = Info {
            series: Some("Lanfeust de Troy".into()),
            volume: Some(2),
            title: Some("Thanos l'incongru".into()),
            ..Info::default()
        };
        let name = file_name(&info).unwrap();
        assert_eq!(name, "Lanfeust de Troy - Tome 2 - Thanos l'incongru");
        assert_eq!(
            guess(&name),
            g(
                Some("Lanfeust de Troy"),
                Some(2),
                Some("Thanos l'incongru"),
                &[]
            )
        );
        let odd = Info {
            title: Some("Qui ? Quoi : où / quand".into()),
            ..Info::default()
        };
        assert_eq!(file_name(&odd).unwrap(), "Qui Quoi où quand");
        let manga = Info {
            series: Some("Naruto".into()),
            volume: Some(12),
            ..Info::default()
        };
        assert_eq!(
            guess(&file_name(&manga).unwrap()),
            g(Some("Naruto"), Some(12), None, &[])
        );
    }

    #[test]
    fn compares_without_accents_or_apostrophes() {
        assert_eq!(
            normalize("Thanos l'incongru"),
            normalize("Thanos lincongru")
        );
        assert_eq!(normalize("Le paladin d'Eckmül"), "le paladin deckmul");
    }

    fn record(title: &str, date: &str, format: &str, isbn: Option<&str>) -> Record {
        Record {
            title: title.into(),
            people: vec![
                "Arleston, Christophe (1963-....). Auteur du texte".into(),
                "Tarquin, Didier (1967-....). Illustrateur".into(),
            ],
            publisher: Some("Soleil (Toulon)".into()),
            date: Some(date.into()),
            isbn: isbn.map(str::to_string),
            format: Some(format.into()),
            link: None,
            printed: true,
        }
    }

    #[test]
    fn picks_the_album_among_editions() {
        let records = [
            record(
                "Thanos l'incongru / Arleston et Tarquin ; texte de Pat et Chris",
                "2003",
                "252 p. : ill. en coul.",
                Some("2012008658"),
            ),
            record(
                "Thanos l'incongru ([Éd. collector 10e anniversaire]) scénario, Christophe Arleston",
                "2004",
                "2 vol. (48, 47 p.) : ill.",
                None,
            ),
            record(
                "Lanfeust de Troy. 1-4 / [scénario de] Arleston",
                "2005",
                "4 vol. : ill. en coul.",
                None,
            ),
            record(
                "Lanfeust de Troy. 2, Thanos l'incongru (Nouv. éd. recolorisée) scénario Christophe Arleston",
                "2011",
                "1 vol. (48 p.) : ill. en coul.",
                Some("9782302015913"),
            ),
            record(
                "Lanfeust de Troy. 3, Castel Or-Azur (Nouv. éd. recolorisée)",
                "2011",
                "1 vol. (48 p.) : ill. en coul.",
                Some("9782302015920"),
            ),
            record(
                "Lanfeust de Troy : Thanos l'incongru / scénario, Christophe Arleston",
                "2014",
                "1 vol. (47-7 p.) : ill. en coul.",
                None,
            ),
        ];
        let found = best_bnf(
            &records,
            &guess("Lanfeust de Troy- Tome 2 - Thanos lincongru (Christophe Arleston, Didier Tarquin etc.)"),
            false,
        )
        .expect("trouvé");
        assert_eq!(found.series.as_deref(), Some("Lanfeust de Troy"));
        assert_eq!(found.volume, Some(2));
        assert_eq!(found.title.as_deref(), Some("Thanos l'incongru"));
        assert_eq!(found.year, Some(2011));
        assert_eq!(found.pages, Some(48));
        assert_eq!(found.isbns, vec!["9782302015913".to_string()]);
        assert_eq!(found.publisher.as_deref(), Some("Soleil"));
        assert_eq!(
            found.authors[0],
            Author {
                name: "Christophe Arleston".into(),
                role: Some("Scénario".into())
            }
        );
        assert!(best_bnf(&records, &guess("Blacksad - 03 - Ame rouge"), false).is_none());
    }

    #[test]
    fn reads_volume_counts() {
        let bd = "{{Infobox Bande dessinée\n | titre = Lanfeust de Troy\n | nombre d'albums      = 9\n}}";
        assert_eq!(infobox_volumes(bd), Some(9));
        let manga = "{{Infobox Animation et bande dessinée asiatique/Manga\n| nb_volumes          = 72\n| liste_volumes = Liste\n}}";
        assert_eq!(infobox_volumes(manga), Some(72));
        assert_eq!(infobox_volumes("| nb_volumes = \n"), None);
    }

    #[test]
    fn parses_catalogue_answers() {
        let xml = r#"<srw:record><oai_dc:dc xmlns:dc="x"><dc:identifier>http://catalogue.bnf.fr/ark:/12148/cb1</dc:identifier>
<dc:title>La bête fabuleuse / scénario, Arleston ; dessins, Tarquin...</dc:title>
<dc:creator>Arleston, Christophe (1963-....). Auteur du texte</dc:creator>
<dc:publisher>Soleil (Toulon)</dc:publisher><dc:date>2000</dc:date>
<dc:identifier>ISBN 2-84565-033-7</dc:identifier><dc:format>61 p. : ill. en coul.</dc:format>
<dc:type xml:lang="fre">texte imprimé</dc:type>
<dc:title>Lanfeust &amp; co</dc:title></oai_dc:dc></srw:record>"#;
        let records = parse_bnf(xml);
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert!(r.title.starts_with("La bête fabuleuse"));
        assert_eq!(r.isbn.as_deref(), Some("2845650337"));
        assert_eq!(r.date.as_deref(), Some("2000"));
        assert_eq!(
            r.link.as_deref(),
            Some("http://catalogue.bnf.fr/ark:/12148/cb1")
        );
        assert_eq!(
            strip_html("Un <i>pirate</i>.<br><br>Fin &amp; suite"),
            "Un pirate.\n\nFin & suite"
        );
    }
}
