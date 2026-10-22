//! Les livres en texte (EPUB, MOBI / AZW3, FB2, TXT) : lus et mis à plat en une
//! suite de blocs (titres, paragraphes, images), que Serj met en page lui-même
//! à la taille de l'écran. Rien ici ne dessine.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// Comment se pose un paragraphe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Justifié, avec un retrait de première ligne.
    #[default]
    Normal,
    Center,
    Right,
    /// En retrait des deux côtés (citation, poème, épigraphe).
    Quote,
    /// Tel quel, retours à la ligne compris.
    Pre,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
        align: Align,
    },
    /// Une image du livre (sa clé dans [`Ebook::images`]).
    Image(String),
    /// Un saut de scène (`<hr>`, ligne vide de FB2).
    Break,
}

impl Block {
    /// Le texte du bloc, pour compter où on en est.
    pub fn len(&self) -> usize {
        match self {
            Block::Heading { spans, .. } | Block::Paragraph { spans, .. } => {
                spans.iter().map(|s| s.text.chars().count()).sum()
            }
            // Une image compte comme une ligne de texte.
            Block::Image(_) | Block::Break => 60,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Un chapitre : son titre et son premier bloc.
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub title: String,
    pub start: usize,
}

/// Ce que le livre dit de lui-même.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<u32>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub year: Option<u32>,
    pub language: Option<String>,
}

pub struct Ebook {
    pub metadata: Metadata,
    pub blocks: Vec<Block>,
    /// Au moins un, le premier au bloc 0.
    pub chapters: Vec<Chapter>,
    pub images: HashMap<String, Vec<u8>>,
}

/// Les formats de livre que Serj lit.
pub const EXTENSIONS: [&str; 6] = ["epub", "mobi", "azw3", "azw", "fb2", "txt"];

pub fn is_ebook(path: &Path) -> bool {
    extension(path).is_some_and(|e| EXTENSIONS.contains(&e.as_str()))
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_string_lossy().to_lowercase())
}

/// Ouvre le livre `path`, d'après son extension.
pub fn open(path: &Path) -> Result<Ebook, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Impossible de lire le fichier ({e})"))?;
    let mut book = match extension(path).as_deref() {
        Some("epub") => epub(&bytes),
        Some("fb2") => fb2(&bytes),
        Some("txt") => Ok(txt(&bytes)),
        Some("mobi" | "azw3" | "azw") => mobi(bytes),
        _ => Err("Format de livre inconnu".into()),
    }?;
    book.blocks
        .retain(|b| !matches!(b, Block::Paragraph { spans, .. } if spans.is_empty()));
    if book.blocks.is_empty() {
        return Err("Ce livre ne contient pas de texte lisible (protégé par DRM ?)".into());
    }
    if book.chapters.first().is_none_or(|c| c.start > 0) {
        book.chapters.insert(
            0,
            Chapter {
                title: String::new(),
                start: 0,
            },
        );
    }
    let count = book.blocks.len();
    book.chapters.retain(|c| c.start < count);
    book.chapters.dedup_by(|b, a| {
        // Deux chapitres au même bloc : le second garde son titre s'il en a un.
        if a.start == b.start {
            if a.title.is_empty() {
                a.title = std::mem::take(&mut b.title);
            }
            true
        } else {
            false
        }
    });
    Ok(book)
}

/// Les infos du livre et sa couverture, sans lire tout le texte.
pub fn metadata(path: &Path) -> Option<(Metadata, Option<Vec<u8>>)> {
    match extension(path)?.as_str() {
        "epub" => {
            let file = std::fs::File::open(path).ok()?;
            let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
            let package = Package::read(&mut zip)?;
            let cover = package
                .cover
                .as_ref()
                .and_then(|href| read_entry(&mut zip, href));
            Some((package.metadata, cover))
        }
        "fb2" => {
            let book = fb2(&std::fs::read(path).ok()?).ok()?;
            let cover = book.images.get(FB2_COVER).cloned();
            Some((book.metadata, cover))
        }
        "mobi" | "azw3" | "azw" => {
            let m = mobi::Mobi::new(std::fs::read(path).ok()?).ok()?;
            Some((mobi_metadata(&m), mobi_cover(&m)))
        }
        "txt" => Some((Metadata::default(), None)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Du HTML aux blocs.

/// Ce qu'une classe CSS du livre change : l'alignement, le gras, l'italique.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ClassStyle {
    align: Option<Align>,
    bold: bool,
    italic: bool,
}

/// Les classes des feuilles de style du livre qui comptent pour Serj.
#[derive(Default)]
struct Css(HashMap<String, ClassStyle>);

impl Css {
    /// Lit une feuille de style : seules les règles sur une classe comptent
    /// (`.centre`, `p.titre`, `span.italique`…).
    fn read(&mut self, css: &str) {
        let mut rest = css;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let (selectors, body) = (&rest[..open], &rest[open + 1..open + close]);
            rest = &rest[open + close + 1..];
            let mut style = ClassStyle::default();
            for decl in body.split(';') {
                let Some((key, value)) = decl.split_once(':') else {
                    continue;
                };
                let value = value.trim().to_ascii_lowercase();
                match key.trim().to_ascii_lowercase().as_str() {
                    "text-align" => {
                        style.align = match value.trim_end_matches("!important").trim() {
                            "center" => Some(Align::Center),
                            "right" => Some(Align::Right),
                            "left" | "justify" => Some(Align::Normal),
                            _ => None,
                        }
                    }
                    "font-style" => {
                        style.italic = value.starts_with("italic") || value.starts_with("oblique")
                    }
                    "font-weight" => {
                        style.bold = value.starts_with("bold")
                            || value.parse::<u32>().is_ok_and(|w| w >= 600)
                    }
                    _ => {}
                }
            }
            if style == ClassStyle::default() {
                continue;
            }
            // `@media` et commentaires : ignorés avec leur sélecteur.
            let selectors = selectors.rsplit("*/").next().unwrap_or(selectors);
            for selector in selectors.split(',') {
                // La dernière partie du sélecteur seulement : `div.x p.y` → `p.y`.
                let last = selector.split_whitespace().last().unwrap_or("");
                for class in last.split('.').skip(1) {
                    let class = class.split([':', '[', '>', '+', '~']).next().unwrap_or("");
                    if !class.is_empty() {
                        let entry = self.0.entry(class.to_string()).or_default();
                        entry.align = style.align.or(entry.align);
                        entry.bold |= style.bold;
                        entry.italic |= style.italic;
                    }
                }
            }
        }
    }

    fn of(&self, e: &BytesStart) -> ClassStyle {
        let mut style = ClassStyle::default();
        if let Some(classes) = attr(e, "class") {
            for class in classes.split_whitespace() {
                if let Some(c) = self.0.get(class) {
                    style.align = c.align.or(style.align);
                    style.bold |= c.bold;
                    style.italic |= c.italic;
                }
            }
        }
        style
    }
}

/// Un élément ouvert, et ce qu'il change jusqu'à sa fermeture.
struct Open {
    name: String,
    bold: bool,
    italic: bool,
    /// Un bloc, avec l'alignement de ce qu'il contient.
    align: Option<Align>,
    pre: bool,
    skip: bool,
}

/// Construit les blocs à partir d'un document HTML (ou XHTML mal formé).
struct Html<'a> {
    blocks: &'a mut Vec<Block>,
    css: &'a Css,
    spans: Vec<Span>,
    heading: Option<u8>,
    open: Vec<Open>,
}

const BLOCK_TAGS: [&str; 24] = [
    "p",
    "div",
    "li",
    "dt",
    "dd",
    "section",
    "article",
    "blockquote",
    "table",
    "tr",
    "td",
    "th",
    "ul",
    "ol",
    "body",
    "figure",
    "figcaption",
    "aside",
    "header",
    "footer",
    "center",
    "address",
    "nav",
    "dl",
];

/// Des éléments sans fermeture, même écrits `<br>` en HTML.
const VOID_TAGS: [&str; 6] = ["br", "hr", "img", "meta", "link", "input"];

fn heading_level(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    (b.len() == 2 && b[0] == b'h' && (b'1'..=b'6').contains(&b[1])).then(|| b[1] - b'0')
}

impl<'a> Html<'a> {
    fn new(blocks: &'a mut Vec<Block>, css: &'a Css) -> Self {
        Self {
            blocks,
            css,
            spans: Vec::new(),
            heading: None,
            open: Vec::new(),
        }
    }

    fn skipping(&self) -> bool {
        self.open.iter().any(|o| o.skip)
    }

    fn style(&self) -> Style {
        Style {
            bold: self.open.iter().any(|o| o.bold),
            italic: self.open.iter().any(|o| o.italic),
        }
    }

    fn pre(&self) -> bool {
        self.open.iter().any(|o| o.pre)
    }

    fn align(&self) -> Align {
        self.open
            .iter()
            .rev()
            .find_map(|o| o.align)
            .unwrap_or_default()
    }

    /// Lit `html` ; `image` donne la clé d'une image d'après sa source (et la
    /// range dans le livre), `pagebreak` est appelé aux sauts de page MOBI.
    fn read(
        &mut self,
        html: &str,
        image: &mut dyn FnMut(&str) -> Option<String>,
        pagebreak: &mut dyn FnMut(usize),
    ) {
        let mut reader = Reader::from_str(html);
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.allow_dangling_amp = true;
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => self.start(&e, false, image, pagebreak),
                Ok(Event::Empty(e)) => self.start(&e, true, image, pagebreak),
                Ok(Event::End(e)) => {
                    let name = lower(e.local_name().as_ref());
                    self.end(&name);
                }
                Ok(Event::Text(e)) => {
                    if let Ok(text) = e.decode() {
                        self.text(&text);
                    }
                }
                Ok(Event::CData(e)) => {
                    if let Ok(text) = e.decode() {
                        self.text(&text);
                    }
                }
                Ok(Event::GeneralRef(e)) => {
                    let resolved = match e.resolve_char_ref() {
                        Ok(Some(c)) => Some(c.to_string()),
                        _ => e.decode().ok().and_then(|name| {
                            quick_xml::escape::resolve_html5_entity(&name).map(str::to_string)
                        }),
                    };
                    if let Some(text) = resolved {
                        self.raw_text(&text);
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                // Du HTML trop abîmé : on garde ce qu'on a lu.
                Err(_) => break,
            }
        }
        self.flush();
    }

    fn start(
        &mut self,
        e: &BytesStart,
        empty: bool,
        image: &mut dyn FnMut(&str) -> Option<String>,
        pagebreak: &mut dyn FnMut(usize),
    ) {
        let name = lower(e.local_name().as_ref());
        let empty = empty || VOID_TAGS.contains(&name.as_str());
        let skip = matches!(name.as_str(), "head" | "script" | "style" | "title");
        if self.skipping() || (skip && empty) {
            if !empty {
                self.open.push(Open {
                    name,
                    bold: false,
                    italic: false,
                    align: None,
                    pre: false,
                    skip: true,
                });
            }
            return;
        }
        let class = self.css.of(e);
        let block = BLOCK_TAGS.contains(&name.as_str()) || name == "pre";
        let level = heading_level(&name);
        match name.as_str() {
            "br" => self.raw_text("\n"),
            "hr" => {
                self.flush();
                self.blocks.push(Block::Break);
            }
            "pagebreak" => {
                self.flush();
                pagebreak(self.blocks.len());
            }
            "img" | "image" => {
                let src = attr(e, "src")
                    .or_else(|| attr(e, "href"))
                    .or_else(|| attr(e, "recindex").map(|r| format!("recindex:{r}")));
                if let Some(key) = src.and_then(|s| image(&s)) {
                    self.flush();
                    self.blocks.push(Block::Image(key));
                }
            }
            _ if block || level.is_some() => self.flush(),
            _ => {}
        }
        if let Some(level) = level
            && !empty
        {
            self.heading = Some(level);
        }
        if empty {
            return;
        }
        let align = if !block && level.is_none() {
            None
        } else if name == "blockquote" {
            Some(Align::Quote)
        } else if name == "center" || centered(e) {
            Some(Align::Center)
        } else {
            class.align.or(Some(self.align()))
        };
        self.open.push(Open {
            bold: class.bold || matches!(name.as_str(), "b" | "strong"),
            italic: class.italic || matches!(name.as_str(), "i" | "em" | "cite" | "var" | "dfn"),
            align,
            pre: name == "pre",
            skip,
            name,
        });
    }

    fn end(&mut self, name: &str) {
        // L'élément fermé, et ceux restés ouverts dedans (HTML mal formé).
        let Some(at) = self.open.iter().rposition(|o| o.name == name) else {
            return;
        };
        let closes_block = self.open[at..].iter().any(|o| o.align.is_some() || o.pre)
            || heading_level(name).is_some();
        if closes_block && !self.skipping() {
            self.flush();
        }
        self.open.truncate(at);
    }

    /// Du texte du document : les blancs se resserrent, sauf dans `<pre>`.
    fn text(&mut self, text: &str) {
        if self.skipping() {
            return;
        }
        if self.pre() {
            return self.raw_text(text);
        }
        let mut out = String::with_capacity(text.len());
        let mut space = self.ends_with_space();
        for c in text.chars() {
            if matches!(c, ' ' | '\n' | '\r' | '\t') {
                if !space {
                    out.push(' ');
                    space = true;
                }
            } else {
                out.push(c);
                space = false;
            }
        }
        self.push(&out);
    }

    fn raw_text(&mut self, text: &str) {
        if !self.skipping() {
            self.push(text);
        }
    }

    fn ends_with_space(&self) -> bool {
        self.spans
            .last()
            .is_none_or(|s| s.text.ends_with([' ', '\n']))
    }

    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let style = self.style();
        match self.spans.last_mut() {
            Some(last) if last.style == style => last.text.push_str(text),
            _ => self.spans.push(Span {
                text: text.to_string(),
                style,
            }),
        }
    }

    /// Le paragraphe (ou le titre) en cours devient un bloc.
    fn flush(&mut self) {
        let mut spans = std::mem::take(&mut self.spans);
        let heading = self.heading.take();
        let pre = self.pre();
        if !pre {
            trim(&mut spans);
        }
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }
        let block = match heading {
            Some(level) => Block::Heading { level, spans },
            None => {
                let mut align = if pre { Align::Pre } else { self.align() };
                // « * * * », « ⁂ » : un saut de scène, au milieu.
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                if text.chars().count() <= 12 && text.chars().all(|c| !c.is_alphanumeric()) {
                    align = Align::Center;
                }
                Block::Paragraph { spans, align }
            }
        };
        self.blocks.push(block);
    }
}

/// Retire les blancs au début et à la fin du paragraphe.
fn trim(spans: &mut Vec<Span>) {
    while let Some(first) = spans.first_mut() {
        let t = first.text.trim_start_matches([' ', '\n']);
        if t.is_empty() {
            spans.remove(0);
        } else {
            first.text = t.to_string();
            break;
        }
    }
    while let Some(last) = spans.last_mut() {
        let t = last.text.trim_end_matches([' ', '\n']);
        if t.is_empty() {
            spans.pop();
        } else {
            last.text = t.to_string();
            break;
        }
    }
}

fn lower(name: &[u8]) -> String {
    String::from_utf8_lossy(name).to_ascii_lowercase()
}

/// La valeur de l'attribut `name` (sans préfixe d'espace de noms).
fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes().with_checks(false).flatten().find_map(|a| {
        (lower(a.key.local_name().as_ref()) == name)
            .then(|| unescape(&String::from_utf8_lossy(&a.value)))
    })
}

fn unescape(s: &str) -> String {
    quick_xml::escape::unescape_with(s, quick_xml::escape::resolve_html5_entity)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

/// Un bloc centré : `align="center"`, un style ou une classe qui le disent.
fn centered(e: &BytesStart) -> bool {
    let has = |name: &str, needle: &str| {
        attr(e, name).is_some_and(|v| v.to_ascii_lowercase().replace(' ', "").contains(needle))
    };
    has("align", "center") || has("style", "text-align:center") || has("class", "cent")
}

// ---------------------------------------------------------------------------
// EPUB.

/// Le paquet d'un EPUB : ses infos, ses documents dans l'ordre de lecture, sa
/// table des matières et sa couverture (chemins dans l'archive).
struct Package {
    metadata: Metadata,
    spine: Vec<String>,
    toc: Vec<(String, String)>,
    cover: Option<String>,
    /// Les feuilles de style.
    styles: Vec<String>,
}

impl Package {
    fn read<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>) -> Option<Self> {
        let container = read_text(zip, "META-INF/container.xml")?;
        let opf_path = find_attr(&container, "rootfile", "full-path")?;
        let opf = read_text(zip, &opf_path)?;
        let base = parent(&opf_path);

        let mut metadata = Metadata::default();
        // id → (chemin, type, propriétés)
        let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
        let mut spine_ids = Vec::new();
        let mut toc_id = None;
        let mut cover_id = None;
        // EPUB 3 : la collection et son rang (`refines`).
        let mut collections: HashMap<String, String> = HashMap::new();
        let mut positions: HashMap<String, u32> = HashMap::new();

        let mut reader = Reader::from_str(&opf);
        reader.config_mut().check_end_names = false;
        let mut current: Option<(String, Option<String>, Option<String>)> = None;
        let mut text = String::new();
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = lower(e.local_name().as_ref());
                    match name.as_str() {
                        "item" => {
                            if let (Some(id), Some(href)) = (attr(&e, "id"), attr(&e, "href")) {
                                let path = resolve(&base, &href);
                                let kind = attr(&e, "media-type").unwrap_or_default();
                                let props = attr(&e, "properties").unwrap_or_default();
                                manifest.insert(id, (path, kind, props));
                            }
                        }
                        "itemref" => {
                            if attr(&e, "linear").as_deref() != Some("no")
                                && let Some(id) = attr(&e, "idref")
                            {
                                spine_ids.push(id);
                            }
                        }
                        "spine" => toc_id = attr(&e, "toc"),
                        "meta" => {
                            let content = attr(&e, "content");
                            match attr(&e, "name").as_deref() {
                                Some("cover") => cover_id = content,
                                Some("calibre:series") => {
                                    metadata.series = content.map(|s| tidy(&s))
                                }
                                Some("calibre:series_index") => {
                                    metadata.series_index = content.and_then(|s| index(&s));
                                }
                                _ => {}
                            }
                            current = Some((
                                "meta".into(),
                                attr(&e, "property"),
                                attr(&e, "refines").or_else(|| attr(&e, "id")),
                            ));
                            text.clear();
                        }
                        "title" | "creator" | "description" | "publisher" | "date" | "language" => {
                            current = Some((name, None, None));
                            text.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    if current.is_some()
                        && let Ok(t) = e.decode()
                    {
                        text.push_str(&t);
                    }
                }
                Ok(Event::GeneralRef(e)) => {
                    if current.is_some() {
                        if let Ok(Some(c)) = e.resolve_char_ref() {
                            text.push(c);
                        } else if let Ok(name) = e.decode()
                            && let Some(s) = quick_xml::escape::resolve_html5_entity(&name)
                        {
                            text.push_str(s);
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let name = lower(e.local_name().as_ref());
                    if let Some((tag, property, id)) = current.take() {
                        if tag != name {
                            current = Some((tag, property, id));
                            continue;
                        }
                        let value = tidy(&text);
                        if value.is_empty() {
                            continue;
                        }
                        match tag.as_str() {
                            "title" => {
                                metadata.title.get_or_insert(value);
                            }
                            "creator" => metadata.authors.push(person(&value)),
                            "description" => {
                                metadata.description = Some(crate::meta::strip_html(&value));
                            }
                            "publisher" => metadata.publisher = Some(value),
                            "date" => metadata.year = value.get(..4).and_then(|y| y.parse().ok()),
                            "language" => metadata.language = Some(value),
                            "meta" => match (property.as_deref(), id) {
                                (Some("belongs-to-collection"), Some(id)) => {
                                    collections
                                        .insert(id.trim_start_matches('#').to_string(), value);
                                }
                                (Some("group-position"), Some(id)) => {
                                    if let Some(n) = index(&value) {
                                        positions.insert(id.trim_start_matches('#').to_string(), n);
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        if metadata.series.is_none()
            && let Some((id, name)) = collections.into_iter().next()
        {
            metadata.series = Some(name);
            metadata.series_index = positions.get(&id).copied();
        }

        let spine = spine_ids
            .iter()
            .filter_map(|id| manifest.get(id).map(|(p, _, _)| p.clone()))
            .collect();
        let cover = manifest
            .values()
            .find(|(_, _, props)| props.split_whitespace().any(|p| p == "cover-image"))
            .or_else(|| cover_id.and_then(|id| manifest.get(&id)))
            .filter(|(_, kind, _)| kind.starts_with("image/"))
            .map(|(p, _, _)| p.clone());
        // La table des matières : celle d'EPUB 3 (nav), sinon le NCX.
        let nav = manifest
            .values()
            .find(|(_, _, props)| props.split_whitespace().any(|p| p == "nav"))
            .map(|(p, _, _)| p.clone());
        let ncx = toc_id
            .and_then(|id| manifest.get(&id))
            .or_else(|| {
                manifest
                    .values()
                    .find(|(_, kind, _)| kind == "application/x-dtbncx+xml")
            })
            .map(|(p, _, _)| p.clone());
        let mut toc = ncx
            .and_then(|p| {
                Some(toc_entries(
                    &read_text(zip, &p)?,
                    &parent(&p),
                    "content",
                    "src",
                ))
            })
            .unwrap_or_default();
        if toc.is_empty()
            && let Some(p) = nav
            && let Some(html) = read_text(zip, &p)
        {
            toc = toc_entries(&html, &parent(&p), "a", "href");
        }
        let styles = manifest
            .values()
            .filter(|(_, kind, _)| kind == "text/css")
            .map(|(p, _, _)| p.clone())
            .collect();
        Some(Self {
            metadata,
            spine,
            toc,
            cover,
            styles,
        })
    }
}

fn epub(bytes: &[u8]) -> Result<Ebook, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "EPUB illisible (archive abîmée)".to_string())?;
    if zip.by_name("META-INF/encryption.xml").is_ok_and(|mut f| {
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        // Seules les polices peuvent être « cachées » sans DRM.
        s.contains("EncryptedData")
            && !s.contains("http://www.idpf.org/2008/embedding")
            && !s.contains("http://ns.adobe.com/pdf/enc#RC")
    }) {
        return Err("Ce livre est protégé par DRM : Serj ne peut pas l'ouvrir".into());
    }
    let package = Package::read(&mut zip).ok_or("EPUB illisible (pas de paquet)")?;
    let mut blocks = Vec::new();
    let mut chapters = Vec::new();
    let mut images: HashMap<String, Vec<u8>> = HashMap::new();
    let mut css = Css::default();
    for path in &package.styles {
        if let Some(text) = read_text(&mut zip, path) {
            css.read(&text);
        }
    }
    let mut titled: HashMap<&str, &str> = HashMap::new();
    for (path, title) in &package.toc {
        titled.entry(path.as_str()).or_insert(title.as_str());
    }
    for doc in &package.spine {
        let Some(html) = read_text(&mut zip, doc) else {
            continue;
        };
        let start = blocks.len();
        let base = parent(doc);
        let mut image = |src: &str| {
            let key = resolve(&base, src);
            if !images.contains_key(&key) {
                images.insert(key.clone(), read_entry(&mut zip, &key)?);
            }
            Some(key)
        };
        Html::new(&mut blocks, &css).read(&html, &mut image, &mut |_| {});
        // Un document dans la table des matières, ou le premier : un chapitre.
        // Les autres continuent le précédent (un chapitre en plusieurs fichiers).
        let title = titled.get(doc.as_str()).map(|t| t.to_string());
        if title.is_some() || chapters.is_empty() || titled.is_empty() {
            let title = title.unwrap_or_else(|| first_heading(&blocks[start..]));
            chapters.push(Chapter { title, start });
        }
    }
    Ok(Ebook {
        metadata: package.metadata,
        blocks,
        chapters,
        images,
    })
}

/// Les entrées d'une table des matières : (document, titre). `tag` porte le
/// lien dans l'attribut `link` (`content src` du NCX, `a href` d'EPUB 3).
fn toc_entries(xml: &str, base: &str, tag: &str, link: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut entries = Vec::new();
    let mut label = String::new();
    let mut in_label = false;
    // Dans le NCX, le titre (navLabel) vient avant le lien ; dans le nav, dedans.
    let ncx = tag == "content";
    let mut pending_href: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = lower(e.local_name().as_ref());
                if ncx && name == "navlabel" {
                    in_label = true;
                    label.clear();
                } else if name == tag
                    && let Some(href) = attr(&e, link)
                {
                    let path = resolve(base, &href);
                    if ncx {
                        entries.push((path, tidy(&label)));
                    } else {
                        pending_href = Some(path);
                        in_label = true;
                        label.clear();
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if in_label && let Ok(t) = e.decode() {
                    label.push_str(&t);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if in_label {
                    if let Ok(Some(c)) = e.resolve_char_ref() {
                        label.push(c);
                    } else if let Ok(n) = e.decode()
                        && let Some(s) = quick_xml::escape::resolve_html5_entity(&n)
                    {
                        label.push_str(s);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = lower(e.local_name().as_ref());
                if ncx && name == "navlabel" {
                    in_label = false;
                } else if !ncx && name == tag {
                    in_label = false;
                    if let Some(path) = pending_href.take() {
                        entries.push((path, tidy(&label)));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    entries
}

fn first_heading(blocks: &[Block]) -> String {
    blocks
        .iter()
        .take(6)
        .find_map(|b| match b {
            Block::Heading { spans, .. } => Some(tidy(
                &spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            )),
            _ => None,
        })
        .unwrap_or_default()
}

fn read_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    let mut file = zip.by_name(path).ok()?;
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn read_text<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, path: &str) -> Option<String> {
    read_entry(zip, path).map(|b| decode_text(&b))
}

/// Le texte d'un fichier : UTF-8 (ou UTF-16 avec sa marque), sinon Windows-1252.
fn decode_text(bytes: &[u8]) -> String {
    if let Some((encoding, skip)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding
            .decode_without_bom_handling(&bytes[skip..])
            .0
            .into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Le valeur de `name` sur le premier élément `tag` du document.
fn find_attr(xml: &str, tag: &str, name: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if lower(e.local_name().as_ref()) == tag {
                    return attr(&e, name);
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// Le chemin dans l'archive de `href` (relatif au dossier `base`) : sans
/// ancre, décodé (`%20`), `..` résolus.
fn resolve(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let href = percent_decode(href);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|p| !p.is_empty()).collect()
    };
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Les blancs resserrés.
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// « 1 », « 2.0 » : un rang dans une série.
fn index(s: &str) -> Option<u32> {
    let n: f32 = s.trim().replace(',', ".").parse().ok()?;
    (n >= 0.0 && n.fract() == 0.0).then_some(n as u32)
}

/// « Rowling, J. K. » devient « J. K. Rowling ».
fn person(s: &str) -> String {
    match s.split_once(',') {
        Some((last, first)) if !s[last.len() + 1..].contains(',') && !first.trim().is_empty() => {
            format!("{} {}", first.trim(), last.trim())
        }
        _ => s.trim().to_string(),
    }
}

// ---------------------------------------------------------------------------
// FB2 (FictionBook).

const FB2_COVER: &str = "fb2:couverture";

fn fb2(bytes: &[u8]) -> Result<Ebook, String> {
    let xml = decode_xml(bytes);
    let mut reader = Reader::from_str(&xml);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.allow_unmatched_ends = true;

    let mut metadata = Metadata::default();
    let mut blocks = Vec::new();
    let mut chapters = Vec::new();
    let mut images: HashMap<String, Vec<u8>> = HashMap::new();
    let mut cover_ref = None;

    // Où on en est dans le document.
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut author = Vec::new();
    let mut binary: Option<String> = None;
    let mut spans: Vec<Span> = Vec::new();
    let (mut bold, mut italic) = (0u32, 0u32);
    let mut section_depth = 0u8;
    let mut title_spans: Vec<Vec<Span>> = Vec::new();
    let mut new_chapter = false;

    let push = |spans: &mut Vec<Span>, t: &str, bold: u32, italic: u32| {
        if t.is_empty() {
            return;
        }
        let style = Style {
            bold: bold > 0,
            italic: italic > 0,
        };
        match spans.last_mut() {
            Some(last) if last.style == style => last.text.push_str(t),
            _ => spans.push(Span {
                text: t.to_string(),
                style,
            }),
        }
    };
    loop {
        let event = reader.read_event();
        match event {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let empty = matches!(event, Ok(Event::Empty(_)));
                let name = lower(e.local_name().as_ref());
                let in_body = path.iter().any(|p| p == "body");
                let in_title_info = path.iter().any(|p| p == "title-info");
                match name.as_str() {
                    "image" => {
                        let href = attr(e, "href").map(|h| h.trim_start_matches('#').to_string());
                        if path.iter().any(|p| p == "coverpage") {
                            cover_ref = href;
                        } else if in_body && let Some(h) = href {
                            blocks.push(Block::Image(format!("fb2:{h}")));
                        }
                    }
                    "sequence" if in_title_info => {
                        metadata.series = attr(e, "name").map(|s| tidy(&s));
                        metadata.series_index = attr(e, "number").and_then(|n| index(&n));
                    }
                    "binary" => binary = attr(e, "id"),
                    "empty-line" if in_body => blocks.push(Block::Break),
                    "section" if in_body && !empty => {
                        section_depth += 1;
                        new_chapter = true;
                    }
                    "strong" if !empty => bold += 1,
                    "emphasis" if !empty => italic += 1,
                    _ => {}
                }
                if !empty {
                    path.push(name);
                    text.clear();
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.decode() {
                    if binary.is_some() {
                        text.push_str(&t);
                    } else if path.iter().any(|p| p == "body") {
                        let t = t.replace(['\n', '\r', '\t'], " ");
                        push(&mut spans, &t, bold, italic);
                    } else {
                        text.push_str(&t);
                    }
                }
            }
            Ok(Event::GeneralRef(ref e)) => {
                let c = match e.resolve_char_ref() {
                    Ok(Some(c)) => Some(c.to_string()),
                    _ => e.decode().ok().and_then(|n| {
                        quick_xml::escape::resolve_xml_entity(&n).map(str::to_string)
                    }),
                };
                if let Some(c) = c {
                    if path.iter().any(|p| p == "body") {
                        push(&mut spans, &c, bold, italic);
                    } else {
                        text.push_str(&c);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = lower(e.local_name().as_ref());
                let in_body = path.iter().any(|p| p == "body");
                let in_title_info = path.iter().any(|p| p == "title-info");
                let in_publish = path.iter().any(|p| p == "publish-info");
                let parent_is = |p: &str| path.len() >= 2 && path[path.len() - 2] == p;
                match name.as_str() {
                    "book-title" if in_title_info => metadata.title = Some(tidy(&text)),
                    "first-name" | "middle-name" | "last-name" | "nickname"
                        if in_title_info && parent_is("author") =>
                    {
                        let t = tidy(&text);
                        if !t.is_empty() {
                            author.push(t);
                        }
                    }
                    "author" if in_title_info => {
                        if !author.is_empty() {
                            metadata.authors.push(author.join(" "));
                        }
                        author.clear();
                    }
                    "lang" if in_title_info => metadata.language = Some(tidy(&text)),
                    "publisher" if in_publish => metadata.publisher = Some(tidy(&text)),
                    "year" if in_publish => metadata.year = tidy(&text).parse().ok(),
                    "binary" => {
                        if let Some(id) = binary.take()
                            && let Ok(data) = base64_decode(&text)
                        {
                            images.insert(format!("fb2:{id}"), data);
                        }
                    }
                    "strong" => bold = bold.saturating_sub(1),
                    "emphasis" => italic = italic.saturating_sub(1),
                    "section" if in_body => section_depth = section_depth.saturating_sub(1),
                    "p" | "v" | "subtitle" | "text-author" if in_body => {
                        trim(&mut spans);
                        let s = std::mem::take(&mut spans);
                        if path.iter().any(|p| p == "title") {
                            title_spans.push(s);
                        } else if !s.is_empty() {
                            if new_chapter {
                                chapters.push(Chapter {
                                    title: String::new(),
                                    start: blocks.len(),
                                });
                                new_chapter = false;
                            }
                            let align = if name == "subtitle" {
                                Align::Center
                            } else if path
                                .iter()
                                .any(|p| matches!(p.as_str(), "poem" | "epigraph" | "cite"))
                            {
                                Align::Quote
                            } else {
                                Align::Normal
                            };
                            if name == "subtitle" {
                                blocks.push(Block::Heading { level: 4, spans: s });
                            } else {
                                blocks.push(Block::Paragraph { spans: s, align });
                            }
                        }
                    }
                    "title" if in_body => {
                        let lines = std::mem::take(&mut title_spans);
                        if !lines.is_empty() {
                            let title = lines
                                .iter()
                                .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
                                .collect::<Vec<_>>()
                                .join(" — ");
                            // Le titre d'une section ouvre un chapitre (ceux des
                            // sous-sections restent dedans).
                            if new_chapter || section_depth <= 1 {
                                chapters.push(Chapter {
                                    title: tidy(&title),
                                    start: blocks.len(),
                                });
                                new_chapter = false;
                            }
                            let level = (section_depth + 1).min(6);
                            for spans in lines {
                                blocks.push(Block::Heading { level, spans });
                            }
                        }
                    }
                    "annotation" if in_title_info => {}
                    _ => {}
                }
                if in_title_info && path.iter().any(|p| p == "annotation") && name == "p" {
                    let t = tidy(&text);
                    let d = metadata.description.get_or_insert_with(String::new);
                    if !d.is_empty() {
                        d.push('\n');
                    }
                    d.push_str(&t);
                }
                if let Some(i) = path.iter().rposition(|p| *p == name) {
                    path.truncate(i);
                }
                if binary.is_none() {
                    text.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    if let Some(cover) = cover_ref.and_then(|r| images.get(&format!("fb2:{r}")).cloned()) {
        images.insert(FB2_COVER.into(), cover);
    }
    Ok(Ebook {
        metadata,
        blocks,
        chapters,
        images,
    })
}

/// Le XML d'un FB2, dans l'encodage que déclare son en-tête.
fn decode_xml(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_lowercase();
    let declared = head
        .split("encoding=")
        .nth(1)
        .and_then(|s| s.trim_start_matches(['"', '\'']).split(['"', '\'']).next())
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()));
    match declared {
        Some(enc) if enc != encoding_rs::UTF_8 => enc.decode(bytes).0.into_owned(),
        _ => decode_text(bytes),
    }
}

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(clean)
}

// ---------------------------------------------------------------------------
// Texte brut.

fn txt(bytes: &[u8]) -> Ebook {
    let text = decode_text(bytes).replace("\r\n", "\n");
    let blank_lines = text.matches("\n\n").count();
    let lines = text.lines().count().max(1);
    // Des paragraphes séparés par des lignes vides, ou une ligne par paragraphe.
    let paragraphs: Vec<String> = if blank_lines * 6 >= lines {
        text.split("\n\n").map(tidy).collect()
    } else {
        text.lines().map(tidy).collect()
    };
    let mut blocks = Vec::new();
    let mut chapters = Vec::new();
    for p in paragraphs.into_iter().filter(|p| !p.is_empty()) {
        let lower = p.to_lowercase();
        let heading = p.chars().count() < 70
            && [
                "chapitre ",
                "chapter ",
                "partie ",
                "part ",
                "livre ",
                "prologue",
                "épilogue",
                "epilogue",
            ]
            .iter()
            .any(|w| lower.starts_with(w));
        let spans = vec![Span {
            text: p.clone(),
            style: Style::default(),
        }];
        if heading {
            chapters.push(Chapter {
                title: p,
                start: blocks.len(),
            });
            blocks.push(Block::Heading { level: 2, spans });
        } else {
            blocks.push(Block::Paragraph {
                spans,
                align: Align::Normal,
            });
        }
    }
    Ebook {
        metadata: Metadata::default(),
        blocks,
        chapters,
        images: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// MOBI / AZW3 (Kindle sans DRM).

fn mobi(bytes: Vec<u8>) -> Result<Ebook, String> {
    let m = mobi::Mobi::new(bytes).map_err(|_| "Livre Kindle illisible".to_string())?;
    if !matches!(m.encryption(), mobi::headers::Encryption::No) {
        return Err("Ce livre est protégé par DRM : Serj ne peut pas l'ouvrir".into());
    }
    let html = m.content_as_string_lossy();
    let first_image = m.metadata.mobi.first_image_index as usize;
    let records = m.raw_records();
    let records = records.records();
    let mut images = HashMap::new();
    let mut image = |src: &str| {
        // `recindex:00012` (MOBI) ou `kindle:embed:000C?mime=…` (KF8), à
        // partir de 1, depuis la première image.
        let n = if let Some(r) = src.strip_prefix("recindex:") {
            r.trim().parse::<usize>().ok()?
        } else {
            let code = src
                .strip_prefix("kindle:embed:")?
                .split(['?', '#'])
                .next()?;
            usize::from_str_radix(code, 32).ok()?
        };
        let key = format!("mobi:{n}");
        if !images.contains_key(&key) {
            let record = records.get(first_image + n.checked_sub(1)?)?;
            images.insert(key.clone(), record.content.to_vec());
        }
        Some(key)
    };
    let mut blocks = Vec::new();
    let mut breaks = vec![0];
    Html::new(&mut blocks, &Css::default()).read(&html, &mut image, &mut |at| breaks.push(at));
    // Un chapitre à chaque saut de page qui commence par un titre.
    let mut chapters = Vec::new();
    for &at in &breaks {
        if let Some(Block::Heading { spans, .. }) = blocks.get(at) {
            chapters.push(Chapter {
                title: tidy(&spans.iter().map(|s| s.text.as_str()).collect::<String>()),
                start: at,
            });
        }
    }
    if chapters.is_empty() {
        chapters = blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match b {
                Block::Heading {
                    level: 1 | 2,
                    spans,
                } => Some(Chapter {
                    title: tidy(&spans.iter().map(|s| s.text.as_str()).collect::<String>()),
                    start: i,
                }),
                _ => None,
            })
            .collect();
    }
    Ok(Ebook {
        metadata: mobi_metadata(&m),
        blocks,
        chapters,
        images,
    })
}

fn mobi_metadata(m: &mobi::Mobi) -> Metadata {
    let clean = |s: Option<String>| s.map(|s| tidy(&s)).filter(|s| !s.is_empty());
    Metadata {
        title: clean(Some(m.title())),
        authors: m
            .author()
            .map(|a| {
                a.split(['&', ';'])
                    .map(person)
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        description: clean(m.description()).map(|d| crate::meta::strip_html(&d)),
        publisher: clean(m.publisher()),
        year: m.publish_date().and_then(|d| d.get(..4)?.parse().ok()),
        ..Metadata::default()
    }
}

fn mobi_cover(m: &mobi::Mobi) -> Option<Vec<u8>> {
    let first_image = m.metadata.mobi.first_image_index as usize;
    let offset = m
        .metadata
        .exth
        .get_record(mobi::headers::ExthRecord::CoverOffset)
        .and_then(|r| r.first())
        .filter(|b| b.len() == 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .unwrap_or(0);
    let records = m.raw_records();
    records
        .records()
        .get(first_image + offset)
        .map(|r| r.content.to_vec())
        .filter(|c| image_like(c))
}

fn image_like(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8]) || bytes.starts_with(b"\x89PNG") || bytes.starts_with(b"GIF8")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(source: &str) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut css = Css::default();
        css.read(".centre, p.x { text-align: center } span.it{font-style:italic}");
        Html::new(&mut blocks, &css).read(source, &mut |s| Some(s.to_string()), &mut |_| {});
        blocks
    }

    fn para(text: &str) -> Block {
        Block::Paragraph {
            spans: vec![Span {
                text: text.into(),
                style: Style::default(),
            }],
            align: Align::Normal,
        }
    }

    #[test]
    fn reads_paragraphs_and_styles() {
        let blocks = html(
            "<html><head><title>x</title><style>p{}</style></head><body>\
             <h1>Chapitre 1</h1>\n<p>Il   était\n une <i>fois</i>&nbsp;!</p>\
             <p class=\"centre\">* * *</p><div><p>Un</p>deux</div><hr/>\
             <p>&#233;t&eacute; &amp; <b>gras</b></p></body></html>",
        );
        assert_eq!(
            blocks[0],
            Block::Heading {
                level: 1,
                spans: vec![Span {
                    text: "Chapitre 1".into(),
                    style: Style::default()
                }]
            }
        );
        assert_eq!(
            blocks[1],
            Block::Paragraph {
                spans: vec![
                    Span {
                        text: "Il était une ".into(),
                        style: Style::default()
                    },
                    Span {
                        text: "fois".into(),
                        style: Style {
                            bold: false,
                            italic: true
                        }
                    },
                    Span {
                        text: "\u{a0}!".into(),
                        style: Style::default()
                    },
                ],
                align: Align::Normal,
            }
        );
        assert!(matches!(
            &blocks[2],
            Block::Paragraph {
                align: Align::Center,
                ..
            }
        ));
        assert_eq!(blocks[3], para("Un"));
        assert_eq!(blocks[4], para("deux"));
        assert_eq!(blocks[5], Block::Break);
        assert!(matches!(&blocks[6], Block::Paragraph { spans, .. } if spans[0].text == "été & "));
    }

    #[test]
    fn resolves_paths() {
        assert_eq!(
            resolve("OEBPS/Text", "../Images/a%20b.jpg#x"),
            "OEBPS/Images/a b.jpg"
        );
        assert_eq!(resolve("", "chap.xhtml"), "chap.xhtml");
        assert_eq!(person("Rowling, J. K."), "J. K. Rowling");
        assert_eq!(person("Joanne K. Rowling"), "Joanne K. Rowling");
    }

    #[test]
    fn reads_plain_text() {
        let book = txt("Chapitre 1\n\nIl était\nune fois.\n\nFin.".as_bytes());
        assert_eq!(book.chapters.len(), 1);
        assert_eq!(book.blocks.len(), 3);
        assert_eq!(book.blocks[1], para("Il était une fois."));
    }

    #[test]
    fn reads_fb2() {
        let xml = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns:l="http://www.w3.org/1999/xlink"><description><title-info>
<author><first-name>Jules</first-name><last-name>Verne</last-name></author>
<book-title>Le Tour du monde</book-title><sequence name="Voyages" number="3"/>
<coverpage><image l:href="#c.jpg"/></coverpage></title-info></description>
<body><section><title><p>Chapitre I</p></title><p>Phileas <emphasis>Fogg</emphasis>.</p>
<empty-line/><p>Fin</p></section></body><binary id="c.jpg" content-type="image/jpeg">/9j/</binary></FictionBook>"##;
        let book = fb2(xml.as_bytes()).unwrap();
        assert_eq!(book.metadata.title.as_deref(), Some("Le Tour du monde"));
        assert_eq!(book.metadata.authors, vec!["Jules Verne"]);
        assert_eq!(book.metadata.series.as_deref(), Some("Voyages"));
        assert_eq!(book.metadata.series_index, Some(3));
        assert_eq!(book.chapters[0].title, "Chapitre I");
        assert_eq!(book.blocks.len(), 4);
        assert!(book.images.contains_key(FB2_COVER));
    }
}
