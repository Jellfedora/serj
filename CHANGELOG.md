# Changelog

## [0.7.0]

- Une bulle ou un cartouche à cheval sur deux cases n'est plus coupé : chaque case qui en porte au moins le tiers le montre en entier.
- Les pages de texte (crédits, listes de la collection) ne sont plus montrées entières, illisibles : elles se lisent par morceaux, colonne par colonne, assez grands pour lire. Une page de titre en gros caractères reste entière.
- Le dossier de Serj est rangé par rayon : « BD », « Mangas » et « Livres », puis par série. La bibliothèque déjà là s'y range d'elle-même, et un album qui change de rayon change de dossier.
- Une série porte le même nom pour tous ses tomes, écrit comme Wikipédia (accents, apostrophes, majuscules) ; les albums déjà là sont cherchés une fois de plus pour suivre.
- Les noms de fichiers des sites de téléchargement : « Tome 512 » se lit « tome 5 sur 12 », et les apostrophes perdues (« lOiseau », « LOeuf ») ne font plus échouer la recherche à la BnF et sur Wikipédia. Les notices « … : cycle de la quête : L'oeuf des ténèbres. 4 » donnent leur titre.
- Un album déplacé ou renommé hors de Serj est retrouvé au lancement, avec son avancement ; un fichier du dossier de Serj que la bibliothèque ne connaissait pas y entre.
- Développement : `cargo run` garde ses réglages, ses albums et ses caches dans `dev-data/`, sans toucher à la bibliothèque de la version installée.

## [0.6.0]

- Les livres : EPUB, MOBI, AZW3, FB2 et TXT (sans DRM), mis en page par Serj à la taille de la fenêtre — texte justifié, alinéas, titres de chapitre, images ; sur deux colonnes quand la fenêtre est large. On tourne les pages au clavier, au clic, à la molette ou en glissant la page, qui glisse un peu et se fond dans la suivante.
- Réglages de lecture, dans la barre du haut : taille du texte (− / +), police avec ou sans empattements (PT Serif, PT Sans), fond des pages (thème, blanc, sépia, nuit) ; retenus d'une fois sur l'autre. Le sommaire mène à chaque chapitre ; en marge, le chapitre en cours, la page et la part lue.
- Les PDF de texte (romans, essais) sont reconnus tout seuls et se lisent page par page, sans cases, avec la même page qui tourne.
- Les BD et mangas en CBZ et CBR.
- Trois onglets dans la bibliothèque : BD, Mangas, Livres, avec leur compte. « Ranger dans » (clic droit sur une carte) change un album de rayon ; la fiche aussi (« Corriger »). La recherche fouille tous les rayons.
- Les infos d'un livre viennent d'abord du livre lui-même (titre, auteurs, série, résumé, couverture), puis du catalogue de la BnF et de Wikipédia. Un livre sans couverture en reçoit une, à son titre.
- Les livres ajoutés sont copiés dans « Livres », dans le dossier de Serj, sous leur série ou leur auteur.
- Envoyer vers ma Kindle (clic droit sur un livre, ou sa fiche) : Serj envoie l'EPUB, le PDF ou le TXT par e-mail à l'adresse de la Kindle, depuis ton compte (Gmail, iCloud ou un autre serveur SMTP) ; le mot de passe reste dans le trousseau du système. Un bandeau suit l'envoi ; l'historique des envois (réussis ou non, avec « Renvoyer ») est dans les réglages.
- De vrais réglages (le bouton à roue dentée de l'en-tête, à la place du numéro de version) : thème, lecture des livres (taille, police, fond des pages), dossier de Serj ; l'envoi vers la Kindle ; « À propos » (version, mises à jour, raccourcis, nouveautés).
- La fiche montre le résumé en entier, à côté de la couverture ; elle ne défile que si l'écran est trop petit.
- La présentation d'une série ne vient plus de l'article sur son adaptation (« Harry Potter (série télévisée) ») : les infos déjà trouvées sont cherchées une fois de plus.
- Correction : la couverture d'un album copié pendant qu'elle se préparait restait en attente (« … »).

## [0.5.0]

- Les BD ajoutées (« Ajouter des BD », glissées dans la fenêtre, ⌘ O) sont copiées dans le dossier de Serj, rangées par série (« Lanfeust de Troy/Lanfeust de Troy - Tome 2 - Thanos l'incongru.pdf ») ; les originaux ne sont pas touchés. La bibliothèque déjà là est copiée au premier lancement, et les PDF des dossiers ajoutés avant aussi (ces dossiers ne sont plus surveillés).
- L'avancement des copies dans un bandeau de l'accueil : l'album en cours, la part copiée, ce qui reste.
- « Dossier des BD », dans l'en-tête de l'accueil, ouvre ce dossier dans le Finder (l'Explorateur sous Windows).
- « Supprimer l'album » (clic droit sur sa carte) met sa copie à la corbeille et l'oublie ; le dossier de la série, vide, est retiré.
- Plus de résumé d'album (Google Books, dont le quota était toujours épuisé) : la fiche montre la présentation de la série, de Wikipédia.

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
