// La cible de compilation (« x86_64-unknown-linux-gnu »…), pour retrouver
// l'archive de ce système dans les releases.
fn main() {
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
