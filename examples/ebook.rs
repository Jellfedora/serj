//! Montre ce que Serj lit d'un livre : `cargo run --example ebook -- livre.epub`.

fn main() {
    for path in std::env::args_os().skip(1) {
        let path = std::path::PathBuf::from(path);
        match serj::ebook::open(&path) {
            Ok(book) => {
                println!("{:#?}", book.metadata);
                println!("{} blocs, {} images", book.blocks.len(), book.images.len());
                for c in &book.chapters {
                    println!("  {:>5}  {}", c.start, c.title);
                }
                for b in book.blocks.iter().take(40) {
                    println!("{b:?}");
                }
            }
            Err(e) => println!("{}: {e}", path.display()),
        }
    }
}
