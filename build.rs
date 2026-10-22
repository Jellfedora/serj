// La cible de compilation (« x86_64-unknown-linux-gnu »…), pour retrouver
// l'archive de ce système dans les releases.
fn main() {
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    // Windows : unrar (les CBR) appelle le registre et la cryptographie
    // d'advapi32, que rien d'autre n'amène dans les exemples.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
