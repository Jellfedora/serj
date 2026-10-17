#!/usr/bin/env bash
# Construit l'archive de l'OS courant dans dist/ : le fichier publié dans les
# releases GitHub, que Ronnie télécharge depuis sa page Téléchargements.
# Les noms (serj-<cible>.tar.gz ou .zip) doivent correspondre à ceux que Ronnie
# cherche (src/modules.rs dans Ronnie).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rm -rf dist
mkdir -p dist/stage

case "$(uname -s)" in
Darwin)
    # Apple Silicon seulement, comme Ronnie.
    rustup target add aarch64-apple-darwin
    cargo build --release --locked --target aarch64-apple-darwin
    app=dist/stage/Serj.app
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp target/aarch64-apple-darwin/release/serj "$app/Contents/MacOS/serj"
    cp assets/icon/Serj.icns "$app/Contents/Resources/"
    sed "s/{{VERSION}}/$version/g" packaging/macos/Info.plist >"$app/Contents/Info.plist"
    # Signature ad hoc : assez pour lancer du code arm64. Serj ne demande
    # aucune autorisation à macOS, rien à garder d'une version à l'autre.
    codesign --force --deep --sign - "$app"
    tar -C dist/stage -czf dist/serj-aarch64-apple-darwin.tar.gz Serj.app
    ;;
Linux)
    target=$(rustc -vV | sed -n 's/^host: //p')
    cargo build --release --locked
    cp target/release/serj dist/stage/
    tar -C dist/stage -czf "dist/serj-$target.tar.gz" serj
    ;;
MINGW* | MSYS* | CYGWIN*)
    target=$(rustc -vV | sed -n 's/^host: //p')
    cargo build --release --locked
    cp target/release/serj.exe dist/stage/
    (cd dist/stage && 7z a -tzip "../serj-$target.zip" serj.exe >/dev/null)
    ;;
*)
    echo "OS non géré : $(uname -s)" >&2
    exit 1
    ;;
esac

rm -rf dist/stage
ls -l dist
