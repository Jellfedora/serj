# Serj

*Une case à la fois.*

Lecteur de BD qui lit **case par case** : chaque case remplit l'écran,
la caméra glisse jusqu'à la suivante. Fait pour les albums franco-belges trop
denses pour un petit écran.

Serj lit aussi les mangas (de droite à gauche) et les livres : il met les
romans en page lui-même, sur une ou deux colonnes, et on tourne les pages.

- BD et mangas : PDF, CBZ, CBR.
- Livres : EPUB, MOBI, AZW3, FB2, TXT (sans DRM), et les PDF de texte, reconnus
  tout seuls.

La bibliothèque a trois onglets : BD, Mangas, Livres. Un clic droit sur une
carte (« Ranger dans ») change un album de rayon.

« Envoyer vers ma Kindle » (clic droit sur un livre) envoie l'EPUB, le PDF ou
le TXT par e-mail à la Kindle, depuis ton compte : l'adresse de la Kindle et le
serveur d'envoi se règlent dans Réglages → Kindle, avec l'historique des envois.

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
| − / + | moins / plus de temps par case en lecture automatique ; dans un livre, texte plus petit / plus grand |
| Glisser la page d'un livre | tourner la page |
| M | manga : de la dernière page à la première, cases de droite à gauche |
| Pincement, ⌘ molette, ⌘ + / ⌘ − | zoomer (vers le pointeur) ; glisser ou molette pour se déplacer |
| Z, ⌘ 0 | zoomer ×2,5 / revenir à la page entière (Échap aussi) |
| F | plein écran |
| ⌘ O | ouvrir une BD ou un livre |
| Clic droit sur un album | lu / pas lu, corriger la recherche, ranger dans BD, Mangas ou Livres… |
| Échap, ⌘ W | revenir à la bibliothèque |

## Développement

```
cargo run --release
cargo run --release --example detect -- album.pdf dossier [première] [dernière]
```

`serj --infos album.pdf…` affiche les infos trouvées en ligne pour des albums
(`SERJ_MANGA=1` pour des mangas), sans ouvrir la fenêtre.

`detect` dessine les cases détectées sur chaque page, pour régler la détection
(`src/panels.rs`). `cargo run --example ebook -- livre.epub` montre ce que Serj
lit d'un livre (infos, chapitres, blocs) ; `cargo run --example kind -- a.pdf…`
dit si des PDF sont des livres ou des BD.

Rendre des pages sans fenêtre (mise en page des livres, accueil) :

```
SERJ_LIVRE=livre.epub SERJ_RENDU=sortie cargo test --release --bin serj rend_des_pages
HOME=autre SERJ_RENDU=sortie cargo test --release --bin serj rend_l_accueil
``` Publier une version : `scripts/release.sh X.Y.Z` (après sa
section dans CHANGELOG.md).
