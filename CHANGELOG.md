# Changelog

## [0.4.0]

- Marquer un album comme lu ou pas encore lu (clic droit sur sa carte, ou sa fiche), même sans l'avoir lu dans Serj ; « Tout marquer comme lu » sur la page d'une série.
- Tomes manquants d'une série : les trous dans la numérotation et les tomes après le dernier (le nombre de tomes vient de l'infobox Wikipédia, ou d'AniList pour un manga), en cartes sur la page de la série (couverture, titre, année et éditeur trouvés en ligne ; auteurs et résumé au survol, la notice au clic) et en compte sur sa tuile. Les infos déjà en cache sont recherchées une fois de plus pour l'apprendre.
- Corriger la recherche : directement depuis le clic droit sur une carte ; on peut y dire si c'est une BD ou un manga. Le titre et les auteurs corrigés priment sur ceux du catalogue.
- « À propos » (le numéro de version dans l'en-tête) : vérifier les mises à jour, liens vers GitHub, les versions et les tickets, raccourcis, nouveautés de chaque version.
- Les liens des fiches (BnF, Wikipédia, Google Books, AniList) s'ouvrent dans le navigateur.
- Un seul bouton « Ajouter des BD » : plusieurs PDF et dossiers d'un coup (sur macOS ; ailleurs des PDF, les dossiers se glissent dans la fenêtre). Glisser plusieurs PDF ou des dossiers les ajoute à la bibliothèque.

## [0.3.0]

- Mises à jour depuis Serj : une nouvelle version est cherchée au lancement (puis toutes les six heures) ; « Mettre à jour » télécharge l'archive de ton système, la vérifie (empreinte SHA-256 publiée par GitHub), remplace l'appli et propose de redémarrer. Installé par Ronnie, Serj lui dit sa nouvelle version.
- Thèmes : Nuit, Encre bleue, Manga, Pop et Papier (clair), en pastilles dans l'en-tête de l'accueil ; retenu d'une fois sur l'autre.
- Accueil plus vivant : recherche dans l'en-tête, trame de points et lueur aux couleurs du thème, titres de rangée marqués, album à la une avec un halo.
- Écran de démarrage : la planche de l'icône se dessine, la caméra plonge dans la case allumée, « SERJ » y éclate comme une onomatopée, une bulle dit la devise, puis l'image se ferme en iris. Un clic ou une touche l'abrège.
- Accueil retravaillé : en-tête avec l'icône et le compte des albums, séries et lectures en cours ; l'album en cours à la une (« Reprendre »), sur sa couverture agrandie ; chaque série en une tuile (une pile de tomes, combien sont lus) qui s'ouvre sur ses albums et sa présentation (Échap pour revenir).
- Les fichiers reconnus sont renommés d'après leurs infos (« Lanfeust de Troy - Tome 2 - Thanos l'incongru.pdf ») : seulement si le catalogue a reconnu l'album ou si ses infos ont été corrigées, jamais par-dessus un autre fichier. Progression, récents, couverture et infos suivent le fichier.
- Barre de recherche dans la bibliothèque (⌘ F) : titre, série, auteur, éditeur ou genre ; Échap pour effacer.
- Infos des albums, automatiquement : série, tome, titre et auteurs tirés du nom du fichier (même hors ligne), puis complétés en ligne — titre exact, auteurs et leur rôle, éditeur, année de première parution, ISBN (catalogue de la BnF), présentation de la série (Wikipédia), résumé de l'album (Google Books, quand son quota le permet ; sinon nouvel essai le lendemain), et pour les mangas résumé, genres et auteurs (AniList). Chaque album n'est cherché qu'une fois (cache).
- La bibliothèque se range par série, dans l'ordre des tomes ; les cartes portent le vrai titre, comme la barre de lecture et la fenêtre.
- Fiche de l'album (bouton « i » au survol d'une couverture, ou clic droit) : résumé, la série, auteurs, éditeur, sources ; « Corriger » pour rectifier série, tome, titre ou auteurs mal reconnus, et relancer la recherche.
- La fenêtre s'ouvre agrandie, sur tout l'écran ; elle reste redimensionnable (F pour le vrai plein écran).
- Zoom (page entière comme case par case) : pincement du trackpad, ⌘ molette, ⌘ + / ⌘ −, ou Z ; la vue se déplace en glissant ou à la molette ; ⌘ 0, Z ou Échap pour revenir. Le zoom revient à 100 % en changeant de case, et la lecture automatique attend pendant qu'on est zoomé.

## [0.2.1]

- Les boutons de la barre de lecture se lisent sur les planches claires : fond sombre, liseré, dégradé plus marqué.

## [0.2.0]

- Lecture automatique (A, ou ▶ dans la barre d'outils) : passe à la case suivante (ou à la page suivante en mode page entière) toutes les 5 s ; délai réglable de 2 à 30 s (− / +), retenu d'une fois sur l'autre. Un anneau en bas à droite montre le temps restant ; elle s'arrête à la fin de l'album.
- Mode manga (M, bouton « Manga », ou clic droit sur l'album dans la bibliothèque) : l'album se lit de la dernière page à la première, les cases de droite à gauche ; la barre d'avancement se remplit de droite à gauche. Retenu pour chaque album.
- Nouvelle interface : barre d'outils en haut pendant la lecture (accueil, titre, lecture auto, cases ou page, manga, plein écran) qui apparaît en fondu ; curseur caché en plein écran. Bibliothèque en deux rangées (« En cours », « Bibliothèque »), cartes qui s'élargissent, ombre et survol animés, étiquettes Nouveau / Terminé / Manga, avancement sur la couverture.

## [0.1.0]

Première version de Serj, le lecteur de BD qui lit case par case.

- Ouvre les BD en PDF (bouton, ⌘ O, ou PDF glissé dans la fenêtre).
- Détecte les cases de chaque planche et les montre une à une, dans l'ordre de lecture : la caméra glisse de case en case et assombrit le reste de la planche.
- Les couvertures, pages de titre et galeries de personnages sans cases s'affichent entières.
- Mode page entière (P), plein écran (F).
- Barre d'avancement en bas : un clic ou un glissement pour aller à une page ; pourcentage lu.
- Retient où tu t'es arrêté dans chaque album.
- L'accueil est une bibliothèque : les albums ouverts et ceux des dossiers ajoutés (« Ajouter un dossier… », sous-dossiers compris), avec leur couverture, leur titre et où tu en es (Nouveau, pourcentage, Terminé). Clic droit : recommencer au début, retirer des récents.
- Bouton « Accueil » en haut à gauche pendant la lecture, et Échap pour revenir à la bibliothèque.
