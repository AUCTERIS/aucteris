# Clôture design, UI et UX — matrice de référence

> Établie le 2026-11-23 sur la branche `feat/phase1-ui-refactor`.
> Portée : consolider le système, la structure et les interactions ; aucun redesign
> visuel. Les couleurs et la direction artistique seront retravaillées ensuite.

## 1. Méthode et sources

- Lecture du code de présentation : `apps/desktop/src/presentation/` (shell, sidebar,
  chat, composer, panneau bot, dialogues, widgets, thème).
- Lecture du modèle et de la persistance : `domain/model.rs`,
  `infrastructure/storage.rs`, `application/session.rs`.
- Références locales :
  - OpenBot : `../openbot/docs/ui-foundation.md`, `../openbot/.github/review/ui-consistency.md`,
    primitives et écrans sous `../openbot/packages/ui/src/`.
  - Grok : l'archive locale `../grok` ne contient que l'application empaquetée
    (`.deb`, Electron) et un `package.json`. Aucune source d'interface exploitable :
    les comportements Grok cités ici viennent de l'usage du produit, pas d'une
    lecture de son code.
- Les éléments marqués **À vérifier** demandent un test dans l'application réelle
  (dark et light) ; le code seul ne prouve pas le rendu.

Classification utilisée :

- **OK** : câblé de bout en bout, feedback présent.
- **Partiel** : existe mais il manque un état, un retour ou un cas d'erreur.
- **À concevoir** : absent ou seulement suggéré par l'interface.

## 2. Matrice « parcours × état × action × feedback »

### 2.1 Premier lancement et création d'un bot

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Ouverture sans donnée | aucun chat | lire l'écran vide | explication + action principale | écran vide avec bouton si un CLI est détecté ; sinon message bloquant sans action d'installation | Partiel |
| Création du premier bot | dialogue `New bot` | choisir runtime, nom, dossier, instructions | validation progressive, erreur explicite | formulaire complet, `Enter` crée, `Tab` circule ; pas de validation champ par champ, pas de test de disponibilité du runtime au moment du choix | Partiel |
| Runtime absent | CLI non installé | sélectionner un provider | prérequis affiché avec action utile | le provider reste sélectionnable ; l'échec n'apparaît qu'à l'envoi du premier prompt | À concevoir |
| Dossier de travail | champ dossier | cliquer « Browse… » | sélecteur natif, chemin affiché, possibilité d'annuler | sélecteur natif, chemin tronqué, `Esc` ferme le dialogue | OK |
| Données existantes | workspace peuplé | rouvrir l'application | aucun onboarding ne réapparaît | l'onboarding n'existe qu'à l'état « aucun chat » | OK |
| Reprise après erreur | formulaire invalide | valider | message relié au champ fautif | bouton grisé sans explication du champ manquant | Partiel |

### 2.2 Shell et navigation

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Changer de chat | repos | clic sur une ligne | ligne sélectionnée, transcript et composer du chat | sélection + fond `raised` ; brouillon global conservé et emporté vers l'autre chat | Partiel |
| Changer de chat | run en cours | clic sur une ligne | navigation autorisée ou refus expliqué | le clic est ignoré silencieusement (`select_chat` retourne tôt) | À corriger |
| Ouvrir le panneau droit | fermé | clic sur l'icône panneau | panneau visible, contexte du bot | ouverture immédiate, pas de transition | OK |
| Fermer le panneau | ouvert | icône `x` ou raccourci | focus rendu au composer | fermeture, focus non déplacé explicitement | Partiel |
| Fenêtre étroite | largeur < sidebar + chat + panneau | redimensionner | chat utilisable, panneau repliable | largeur minimale 900 px ; aucune règle de repli automatique, le panneau reste ouvert | À concevoir |
| Sidebar chargée | beaucoup de bots/rooms | parcourir | scroll, regroupement lisible | sections Bots/Rooms, scroll ; pas de compteur ni de regroupement | OK |
| Recherche | champ vide / sans résultat | taper | filtrage, état vide, effacer | filtre titre uniquement ; état vide présent ; `Esc` efface et rend le focus | OK |
| Menu « + » | fermé | ouvrir, choisir | menu ancré, fermeture au clic extérieur/Échap | les deux marchent ; pas de navigation clavier dans le menu | Partiel |

### 2.3 Conversation et run

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Envoi | repos | composer + envoyer | prompt affiché immédiatement, bot `Working` | message utilisateur + bulle bot vide `streaming`, statut « Starting… » | OK |
| Streaming | run actif | l'agent écrit | texte continu, ordre préservé | `push_text` agrège dans le dernier `Part::Text` | OK |
| Scroll pendant le stream | l'utilisateur remonte | lire un message précédent | ne pas être ramené en bas | chaque événement appelle `scroll_to_bottom()` | À corriger |
| Outil en cours | run actif | l'agent appelle un outil | action, état, détails | ligne outil + statut ; icône verte tant que `done == false` | Partiel |
| Outil terminé | settle | résultat | état « terminé » distinct | `done = true`, icône grise, résultat en petit ; pas de mot d'état | Partiel |
| Outil échoué | settle en erreur | lire l'échec | échec visible et distinct | seul `Part::Tool.done` existe ; « Run failed » est traité comme un résultat ordinaire | À concevoir |
| Approbation | attente | choisir Allow/Deny/Always | attente expliquée, choix hiérarchisés, résolution | carte complète, résolution en vert ; tous les choix « allow » ont le même poids visuel | Partiel |
| Question simple | attente | cliquer une option | réponse envoyée, carte résolue | envoi immédiat, résolution affichée | OK |
| Question multiple | attente | cocher puis envoyer | sélection visible, envoi explicite | bordure sur les choix, bouton Send ; envoi à vide = « Skipped » | OK |
| Réponse libre | attente | cliquer « Type an answer » puis écrire | composer en mode réponse, confirmation | le composer prend la main, la réponse résout la carte | OK |
| Réponse privée | `secret` | répondre | ne pas afficher la valeur | la carte affiche « Answered privately » | OK |
| Stop | run actif | bouton Stop ou Échap ? | arrêt confirmé, partiel conservé | bouton Stop dans statut et composer ; pas de raccourci Échap | Partiel |
| Run terminé | succès | lire | présence `Resting`, « Completed » | cartes non résolues fermées, statut effacé | OK |
| Run en échec | erreur | lire | message d'erreur distinct, reprise possible | texte dans la bulle, cartes fermées, bot `Resting` | Partiel |
| Reprise après réponse | approbation/question | répondre | bot repasse `Working` | `resume_after_answer` fait repasser en `Working` | OK |
| Historique | chat sélectionné | rouvrir | ordre et scroll prévisibles | parties riches persistées, scroll forcé en bas | OK |

### 2.4 Composer

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Envoi clavier | focus composer | `Enter` | envoie si le réglage l'autorise | `send_on_enter` respecté, `Shift+Enter` insère une ligne | OK |
| Envoi clavier | `send_on_enter = false` | `Ctrl/Cmd+Enter` | envoie | câblé | OK |
| Édition | texte présent | sélection, copier, coller, annuler | édition native complète | composer maison : pas de sélection, presse-papiers, IME ni annulation ; `text_input.rs` n'est pas compilé (`mod.rs` ne l'inclut pas) | À concevoir |
| Pièce jointe | repos | clic sur « + » | sélecteur de fichier ou action réelle | insère le texte factice `[attached: file]` | À corriger |
| Brouillon | changement de chat | revenir | brouillon du chat retrouvé | un seul brouillon global, écrasé au changement | Partiel |
| Focus | ouverture | clic ailleurs puis revenir | caret stable, focus visible | caret clignotant ; focus du composer non matérialisé par un anneau | Partiel |

### 2.5 Panneau droit et réglages

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Identité du bot | bot sélectionné | lire | mascotte, nom, rôle, présence | présent | OK |
| Changer de runtime | panneau | ouvrir le picker | sélection immédiate, fermeture | écrit dans le bot, pas de confirmation | OK |
| Sandbox | panneau | cliquer la ligne | niveau suivant, avertissement si dangereux | cycle immédiat ; libellé seul, pas de confirmation pour `DangerFullAccess` | Partiel |
| Usage | panneau | lire / reset | valeurs cohérentes | compteur tokens, reset ; limite de contexte inconnue = jauge vide | OK |
| Édition | panneau | icône crayon | dialogue pré-rempli, sauvegarde | formulaire partagé, sauvegarde immédiate | OK |
| Réglages | dialogue | changer un réglage | effet immédiat | `send_on_enter`, apparence, mode ; pas de navigation clavier interne | Partiel |
| Room | room sélectionnée | gérer les membres | ajout/retrait avec feedback | ajout/retrait immédiats ; pas d'état vide d'équipe actionnable | Partiel |

### 2.6 Clavier, focus et accessibilité

| Parcours | État | Action | Feedback attendu | Réalité | Classe |
|---|---|---|---|---|---|
| Raccourcis globaux | hors champ | `Ctrl+J/I/N/F` | action + focus cohérent | câblés sur des lettres seules avec modificateur ; `Ctrl+F` capture aussi la recherche alors qu'un dialogue est fermé | Partiel |
| Navigation des lignes | sidebar | flèches + Entrée | sélection au clavier | aucune ligne focusable | À concevoir |
| Cartes | approbation/question | Tab + Entrée | choix atteignables | uniquement cliquables | À concevoir |
| Dialogues | ouvert | Tab | piège de focus, Échap, retour du focus | Échap ferme et rend le focus au composer ; `Tab` n'est géré que dans le formulaire bot ; pas de piège de focus | Partiel |
| Focus visible | tout contrôle | Tab | anneau visible | seuls le champ de recherche et le focus `edge` des champs ; boutons icône sans anneau | À concevoir |
| États sans couleur | présence | lire | mot, icône ou texte | pastille couleur + activité textuelle dans la sidebar ; statut de run = pastille + texte | Partiel |
| Contrastes | dark/light | lire | AA pour texte et focus | tokens cohérents mais aucun test automatisé de contraste | À vérifier |

## 3. Écarts structurants constatés

1. **Navigation bloquée pendant un run.** `select_chat` retourne immédiatement si
   `running()`. Aucun message n'explique le blocage. Le statut et le bouton Stop
   s'affichent aussi sur un chat qui n'est pas celui du run.
2. **Brouillon global.** Un seul `draft: String` dans `Shell` : changer de chat
   emporte le texte vers un autre bot.
3. **Scroll non respectueux.** Chaque événement de run force le bas du transcript.
4. **Fausse action.** Le bouton de pièce jointe insère un texte factice.
5. **Aucune surface de saisie réutilisable.** `text_input.rs` est orphelin (référence
   un module `theme::current` inexistant, et n'est pas déclaré dans `mod.rs`).
   Le composer et les champs du dialogue réimplémentent saisie, caret et touches.
6. **État d'outil incomplet.** Pas de distinction échec/terminé, seulement `done`.
7. **Absence de navigation clavier hors dialogues.** Lignes de sidebar, cartes,
   menus et contrôles du panneau ne sont pas focusables.
8. **Réglages partiellement persistés.** Le mode passe par `dispatch`, mais
   l'apparence et `send_on_enter` changent sans écriture disque ; l'apparence
   n'est d'ailleurs pas dans `Settings` et repart de l'apparence OS au lancement.
9. **Pas de tokens de mouvement** ni de largeur de composer/menus : les valeurs
   ponctuelles subsistent (`48.0`, `200.0`, `150.0`, tailles d'icônes).

## 4. Décisions de phase (à appliquer dans cet ordre)

- **Shell** : conserver sidebar gauche et panneau droit. Le panneau droit reste
  fermé par défaut et n'apparaît que sur demande ; à moins de 1 100 px, il se ferme
  au lieu de comprimer le transcript. Le transcript garde `CHAT_COLUMN_WIDTH`.
- **Runs** : un run appartient à un chat. Tant que l'état de run n'est pas par chat,
  la navigation pendant un run reste bloquée, mais avec un retour explicite ; le
  statut n'est affiché que dans le chat du run.
- **Brouillons** : un brouillon par chat, en mémoire, restauré au retour.
- **Scroll** : suivre le bas uniquement si l'utilisateur y était déjà ; un envoi
  explicite ramène en bas. Une action « Revenir au dernier message » sera ajoutée
  si le besoin est confirmé en test.
- **Saisie** : ne pas brancher `text_input.rs` avant d'avoir décidé s'il remplace
  le composer maison ; en attendant, documenter les limites (pas d'IME/sélection).
- **Composer** : retirer le bouton de pièce jointe tant qu'aucun sélecteur de
  fichier n'existe.
- **Outils** : `Part::Tool` gagne `failed` (sérialisé avec `default`), rendu avec un
  mot d'état et une couleur sémantique.
- **Approbations** : un seul choix principal (« Allow »), les autres allow en
  secondaire, « Deny » en texte dangereux. Jamais trois boutons primaires.
- **Présence** : la pastille s'accompagne toujours d'un mot (`Working`,
  `Waiting on you`, `Resting`) dans le statut de run.
- **Tokens** : toute nouvelle vue consomme `theme.rs` ; un nouveau token doit être
  ajouté avec un rôle sémantique, pas une valeur locale.
- **Mouvement** : `Animation::with_easing(gpui::ease_out_quint())`, durées
  120/180/260 ms. `AnimationExt` respecte déjà `App::reduce_motion` : ne pas
  promettre davantage.

## 5. Grille de vérification de sortie

| Scénario | Dark | Light | Clavier | Fenêtre étroite | Preuve |
|---|---|---|---|---|---|
| Premier lancement sans CLI | | | | | manuel |
| Création du premier bot | | | | | manuel |
| Conversation vide | | | | | manuel |
| Run normal + streaming | | | | | manuel |
| Outil en cours / terminé / échoué | | | | | unités + manuel |
| Approbation (allow/deny/always) | | | | | manuel |
| Question simple / multiple / libre / privée | | | | | manuel |
| Stop puis reprise | | | | | manuel |
| Erreur de run puis nouvel envoi | | | | | manuel |
| Recherche sans résultat | | | | | manuel |
| Panneau fermé / ouvert | | | | | manuel |

Les cases restent vides tant que le scénario n'a pas été rejoué dans l'application
réelle. Les tests unitaires ne valident que l'état, pas le rendu.

## 6. Lot 1 appliqué (2026-11-23)

Les tableaux ci-dessus décrivent l'état constaté **avant** ce lot. Ce qui a été
corrigé et vérifié par compilation :

- **Brouillon par chat** : `Shell::drafts` (en mémoire) conserve le texte avec
  son chat ; `select_chat` sauve puis restaure. Créer un bot ou une room ne
  mélange plus les brouillons.
- **Navigation pendant un run** : un run appartient à son chat. Les événements
  écrivent dans le chat du run par identifiant, plus dans le chat sélectionné ;
  le statut et Stop ne s'affichent que dans ce chat. Envoyer depuis un autre
  chat affiche une ligne système explicite au lieu d'un échec silencieux.
- **Suivi du scroll** : le streaming ne ramène plus en bas si la personne a
  remonté le transcript ; un envoi et une demande d'approbation ramènent en bas.
- **Outils** : `Part::Tool.failed` (persisté, `serde(default)`), rendu « Running /
  Done / Failed » avec couleur sémantique et résultat coloré. Tests unitaires +
  test de compatibilité des fichiers sans le champ.
- **Approbations** : un seul bouton primaire (premier choix d'allow), les autres
  allow en secondaire, Deny en texte dangereux. Les options de question
  sélectionnées ont fond + bordure.
- **Présence** : le statut de run affiche le mot d'état (`Working`,
  `Waiting on you`, `Resting`) à côté de la pastille.
- **Composer** : bouton de pièce jointe factice retiré ; hauteur, gouttières,
  caret et tailles d'icônes passent par `theme.rs`.
- **Tokens** : ajout de `COMPOSER_HEIGHT`, `MENU_WIDTH`, `MENU_ENTRY_HEIGHT`,
  `SIDEBAR_ROW_HEIGHT`, `ROLE_FIELD_WIDTH`, `DIALOG_MAX_HEIGHT`,
  `PRESENCE_DOT`, `CARET_WIDTH`, tailles d'icônes/mascottes, commutateur, et du
  module `motion` (120/180/260 ms + `ease_out_quint`, réduit par GPUI via
  `AnimationExt`).
- **Documentation** : cette matrice et ses décisions servent de référence aux
  lots suivants.

Reste ouvert dans la phase : navigation clavier hors dialogues, focus visible
sur les boutons icône, piège de focus des dialogues, saisie riche/IME du
composer, persistance de l'apparence et de `send_on_enter`, règle de repli du
panneau droit en fenêtre étroite, validation du formulaire bot champ par champ,
et la passe visuelle dark/light de la grille de scénarios.
