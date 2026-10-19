# Serj

Lecteur de BD en PDF qui lit **case par case** : chaque case remplit l'écran,
la caméra glisse jusqu'à la suivante. Fait pour les albums franco-belges trop
denses pour un petit écran.

Serj est un module de [Ronnie](https://github.com/Jellfedora/ronnie) : on
l'installe depuis sa page Téléchargements. Il marche aussi tout seul.

## Raccourcis

| Touche | Action |
| --- | --- |
| → ↓ Espace, clic | case suivante |
| ← ↑, clic dans le tiers gauche | case précédente |
| Barre du bas | aller à une page |
| P | page entière |
| A | lecture automatique (5 s par case par défaut) |
| − / + | moins / plus de temps par case en lecture automatique |
| M | manga : de la dernière page à la première, cases de droite à gauche |
| Pincement, ⌘ molette, ⌘ + / ⌘ − | zoomer (vers le pointeur) ; glisser ou molette pour se déplacer |
| Z, ⌘ 0 | zoomer ×2,5 / revenir à la page entière (Échap aussi) |
| F | plein écran |
| ⌘ O | ouvrir un PDF |
| Échap, ⌘ W | revenir à la bibliothèque |

## Développement

```
cargo run --release
cargo run --release --example detect -- album.pdf dossier [première] [dernière]
```

`serj --infos album.pdf…` affiche les infos trouvées en ligne pour des albums
(`SERJ_MANGA=1` pour des mangas), sans ouvrir la fenêtre.

`detect` dessine les cases détectées sur chaque page, pour régler la détection
(`src/panels.rs`). Publier une version : `scripts/release.sh X.Y.Z` (après sa
section dans CHANGELOG.md).
