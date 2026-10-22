//! Dit si des PDF sont des livres (surtout du texte) ou des BD :
//! `cargo run --example kind -- album.pdf…`.

fn main() {
    for path in std::env::args().skip(1) {
        let doc = serj::pdf::Document::open(std::fs::read(&path).unwrap()).unwrap();
        let kind = if doc.mostly_text() { "livre" } else { "BD" };
        println!("{kind:>5}  {path}");
    }
}
