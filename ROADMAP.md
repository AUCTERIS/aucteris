# ROADMAP — Aucteris

> Dernière mise à jour : 2026-11-24 · Positionnement : **Linux-first** · MVP fonctionnel
> avant les extensions avancées

Aucteris est un espace de travail local-first en Rust (GPUI + CLI) pour créer et faire
tourner des bots agents via des CLIs installés. Le produit cible : un quotidien pour des
agents persistants, différencié par **l'isolation réelle des exécutions**, des **missions
vérifiées** et un **moteur embarquable** — le desktop et le CLI sont deux clients du même
core, sans service distant.

## État actuel (2026-11-19)

- **Desktop GPUI** (`apps/desktop`) : bots, chats, sessions persistantes/resumables,
  steering, cartes d'approbation et de questions, dark/light, persistance JSON atomique.
- **Core** (`crates/core`) : moteur de missions YAML (`verify_cmd`, evidence append-only),
  sandbox bwrap, cgroups v2, proxy egress (médiation), injection MCP mémoire,
  orchestrators restate/temporal/local derrière un port neutre.
- **Runtimes** : 6 adapters mission (claude-code, codex, opencode, gemini, devin, mock) +
  komet-harness (claude, codex, opencode, devin, grok) sur le chemin desktop uniquement.

## Principes

1. Conserver les règles d'[ARCHITECTURE.md](ARCHITECTURE.md) : GPUI confiné à
   `apps/desktop`, parsing CLI dans les adapters, moteur appelable sans fenêtre.
2. **Un seul chemin d'exécution à terme** : une couche runtime unique alimente les chats
   et les missions.
3. **Linux-first** : l'isolation (bwrap, cgroups, egress) est garantie sur Linux ;
   ailleurs elle est dégradée et signalée.
4. **Sécurité par défaut** : jamais de full-access implicite ; les élévations sont
   toujours des choix explicites de l'utilisateur.
5. **Accès fichiers des bots** : chaque bot reçoit automatiquement son propre espace
   privé. Le niveau `WorkspaceWrite` est le défaut. Les droits de lecture et d'écriture
   sur les dossiers supplémentaires sont accordés séparément. `DangerFullAccess` reste
   une élévation explicite.
6. Toute feature « quotidien agent » est validée par des tests d'intégration sur les
   fixtures mock existantes.
7. **Construit de bout en bout** : tout le code est écrit dans ce dépôt, hors dépendances
   déclarées (GPUI, komet-harness) ; aucune reprise de code produit tiers.

## Objectif de livraison

Les phases 0 à 5 construisent et livrent un **MVP fonctionnel**. Une personne doit
pouvoir installer Aucteris sur Linux, configurer un bot, utiliser un runtime local ou
un endpoint compatible, travailler sur ses fichiers avec des permissions visibles,
connecter ses outils, reprendre ses tâches après redémarrage et vérifier le résultat
d'une mission depuis l'application. Les permissions annoncées doivent être appliquées
par le runtime, y compris pour les commandes, pas seulement par les outils de fichiers.
Le produit reste utilisable sans compte Aucteris, service hébergé ni synchronisation cloud.

La phase 6 commence après validation de ce parcours de bout en bout. Elle regroupe les
extensions avancées — navigateur contrôlable, synchronisation multi-appareils, accès
distant, voix et computer use — qui ne conditionnent pas le MVP.

Les benchmarks fonctionnels ayant guidé les ajouts sont consignés dans
[`docs/product/feature-parity-grok-bot.md`](docs/product/feature-parity-grok-bot.md) et
[`docs/product/feature-parity-openbot.md`](docs/product/feature-parity-openbot.md).

## Phase 0 — Consolidation (prérequis)

La plus grosse dette avant d'ajouter des features. Suivi global : [#2](https://github.com/AUCTERIS/aucteris/issues/2).

- [x] **Unifier les chemins d'exécution**
      ([#3](https://github.com/AUCTERIS/aucteris/issues/3)) — une seule couche runtime
      pour les chats et les missions ; les missions obtiennent sessions persistantes,
      approvals et steering ; gemini ne requiert plus de chemin dédié ; ADR de décision
      requis.
- [x] **Persistance fidèle**
      ([#4](https://github.com/AUCTERIS/aucteris/issues/4)) — sérialiser les `Part`
      riches (Approval, Question, Tool, Files, Thought) sans aplatissement en texte,
      avec migration douce du format existant
      ([apps/desktop/src/infrastructure/storage.rs](apps/desktop/src/infrastructure/storage.rs)).
- [x] **CI réelle** ([#5](https://github.com/AUCTERIS/aucteris/issues/5)) —
      `cargo test --workspace` + `cargo clippy -D warnings` dans la CI ; transformer les
      3 fixtures mock orphelines (`crates/core/tests/fixtures/`) en tests d'intégration
      réels ; couvrir `executor.rs` et l'orchestrator `local`.
- [x] **Quick fixes** — grok dans le catalogue de runtimes
      ([#6](https://github.com/AUCTERIS/aucteris/issues/6)), réglages persistés
      ([#7](https://github.com/AUCTERIS/aucteris/issues/7)), usage tokens réels et
      timestamps réels ([#8](https://github.com/AUCTERIS/aucteris/issues/8)).
- [x] **Docs** ([#9](https://github.com/AUCTERIS/aucteris/issues/9)) — créer `docs/`
      (VERTICAL-SLICE-01, ADR-0003) ; ranger les diagrammes archify (`examples/`,
      `web-app-rendered.html`) ; plus aucune référence à un document inexistant.

## Phase 1 — Refonte & redesign UI

Restructurer la présentation et donner au desktop une identité visuelle propre avant
d'empiler des features.

**Refacto**

- [x] **Extraire la couche runtime/session en crate dédié** (`crates/harness`) — le
      desktop ne dépend plus directement de komet-harness ; API interne stable
      (RunControls, événements, approvals, questions). S'appuie sur #3.
- [x] **Découper les fichiers monolithiques** :
      [dialog.rs](apps/desktop/src/presentation/ui/dialog.rs) (599 l. + modules par vue),
      [chat.rs](apps/desktop/src/presentation/ui/chat.rs) (236 l. + sous-modules),
      [shell.rs](apps/desktop/src/presentation/ui/shell.rs) (366 l. + sous-modules).
- [x] **Centraliser l'état UI et la persistance** : un modèle d'actions unique, une
      écriture disque par mutation contrôlée (remplacer le compteur de génération).
- [x] **Tests de composants** : chemins critiques de saisie (caret/placeholder),
      widgets et dialogues testés.

**Redesign**

- [x] **Design tokens** centralisés (espacements, rayons, typographie, couleurs,
      animations) dans [theme.rs](apps/desktop/src/presentation/theme.rs), consommés
      partout — plus de valeurs magiques.
- [x] **Layout workspace** : shell redessiné — sidebar chats/rooms, zone de conversation,
      panneau latéral du bot, barre de statut ; hiérarchie visuelle claire et densité
      maîtrisée.
- [x] **Identité visuelle propre** : palette signature, jeu d'icônes cohérent, mascottes
      réutilisées systématiquement, accents par bot.
- [x] **Onboarding premier lancement** : création du premier bot guidée, états vides
      soignés.
- [x] **Micro-interactions** : transitions douces (GPUI), focus clavier complet,
      caret/scroll soignés, feedbacks d'état (working/waiting/resting).
- [x] **Accessibilité** : navigation clavier sans souris, contrastes validés en
      dark/light.

**Clôture Phase 1 (2026-11-24)** : les critères de structure, d'actions/persistance,
de tokens, d'interactions et d'accessibilité sont validés par compilation, Clippy,
tests et calculs de contraste. La base graphique reste l'interface Aucteris actuelle.
La revue visuelle dark/light dans l'application est conservée dans la grille de
`docs/design/2026-11-23-ui-ux-phase-matrix.md` comme vérification manuelle. Le module
expérimental `text_input.rs` reste non branché : son API GPUI est obsolète et son
remplacement complet du composer n'est pas nécessaire à la clôture UI de cette phase.

## Phase 2 — Permissions et quotidien agent

Priorité produit : rendre les accès compréhensibles, prévisibles et révocables avant
d'étendre les capacités des bots.

- [ ] **Espace privé et droits par bot** : créer automatiquement un espace privé pour
      chaque bot, sans demander un dossier lors de sa création. `WorkspaceWrite` est le
      défaut : lecture/écriture dans cet espace. `ReadOnly` interdit toute écriture.
      `DangerFullAccess` étend l'accès en lecture et écriture à l'ordinateur après un
      choix explicite.
- [ ] **Droits distincts par dossier** : chaque dossier ajouté porte des droits séparés
      `Read` et `Write`. Un dossier peut être lisible sans être modifiable. Une demande
      d'écriture persistante ne transforme pas silencieusement un dossier en accès
      complet ; l'utilisateur choisit les droits à retenir.
- [ ] **Centre des permissions dans le panneau latéral du bot** : afficher le niveau
      effectif, l'espace privé, les dossiers ajoutés, leurs droits `Read`/`Write`, les
      exclusions et les capacités/limites du runtime. Permettre d'ajouter, modifier et
      retirer les droits, avec effet sur les prochains runs et révocation immédiate.
- [ ] **Approbations de fichiers explicites** : montrer l'action et le chemin ciblé, avec
      « Autoriser une fois », « Ajouter ce dossier avec les droits choisis » et « Refuser ».
      Une autorisation ponctuelle expire après l'action ; un droit persistant est affiché
      séparément dans le centre des permissions.
- [ ] **Matrice de capacités des runtimes** : appliquer les dossiers ciblés seulement
      lorsque le runtime les prend réellement en charge ; signaler les limites et ne
      jamais afficher une autorisation persistante inactive. Garder OpenCode bloqué pour
      l'accès ciblé tant que son sandbox ne permet pas de sélectionner des chemins ;
      suivre séparément le support requis par l'intégration Codex/Komet.
- [ ] **Confinement au niveau du runtime** : appliquer les droits aux outils de fichiers
      et aux processus/commandes, afin qu'un shell ne contourne pas les règles du bot.
      Résoudre les chemins canoniques, gérer les liens symboliques et borner tout accès
      temporaire. Ne jamais annoncer une permission comme confinée si le runtime ne la
      garantit pas ; masquer ou désactiver le niveau concerné et expliquer la limite.
- [ ] **Révocation** : répercuter un retrait sur les prochains runs et arrêter un run actif
      si nécessaire. Un dossier approuvé ne doit jamais élargir silencieusement son accès.
- [ ] **Persistance et migration** : sauvegarder les espaces privés et les autorisations
      par bot, avec des valeurs par défaut compatibles avec les profils existants.
- [ ] **Vérification des permissions** : couvrir les trois niveaux, les droits de lecture
      et d'écriture, les approbations uniques/persistantes, l'ajout/modification/retrait,
      la révocation en cours de run, les tentatives de contournement par commandes, les
      chemins invalides/liens symboliques et les différences entre runtimes.

Une fois cette base établie, poursuivre le quotidien agent :

- [ ] **Files FIFO par bot** : pause/reprise/annulation, persistance crash-safe, reprise
      après redémarrage ; afficher quel chat possède le run actif.
- [ ] **Pièces jointes réelles** : sélecteur de fichiers, aperçu, persistance dans le
      transcript et transmission au runtime ; retirer le contrôle factice actuel.
- [ ] **Compaction de contexte automatique** avant saturation de la fenêtre modèle
      (komet expose déjà `context_usage()`) + indicateur dans le chat.
- [ ] **Mémoires d'agent** sur le chemin sessions : brancher le MCP aucteris-memory
      (déjà injecté côté missions) à tous les chats + revue de mémoires.
- [ ] **Harnais ACP supplémentaires** : komet fournit déjà `AcpHarness` (cursor, cline,
      kimi, copilot, mistral, hermes) — les ajouter au catalogue + ProviderPicker.
- [ ] **Endpoints OpenAI-compatibles custom** + détection des serveurs locaux
      (Ollama, LM Studio) et des CLIs installés.
- [ ] **Catalogue de modèles** : afficher les modèles fournis par les runtimes et
      endpoints configurés, leurs capacités connues et leurs limites.
- [ ] **Thèmes externes** : palettes JSON surveillées à chaud dans
      `~/.config/aucteris/themes/`, sans redémarrage.
- [ ] **Steering UX** : indiquer visuellement les tours steered, busy-message.
- [ ] **Branching de conversation** : fork une conversation depuis n'importe quel
      message.

## Phase 3 — Multi-agent

- [ ] **Messages agent-à-agent** : envoi, réponses, réactions, images, notifications.
- [ ] **Canaux partagés complets** : un owner de tâche, délégation explicite, historique
      partagé, contrôles Stop/Resume/Reassign/Archive (les rooms `create_team` actuelles
      sont la base).
- [ ] **Tâches secondaires persistantes** : chaque tâche déléguée a son fil, état,
      progression et résultat ; reprendre, arrêter, fermer et rouvrir depuis l'équipe.
- [ ] **Transferts de fichiers** entre bots : manifest + dossier partagé supervisé.
- [ ] **Files de délégation** cross-bots : une file de tâches consommée par plusieurs
      bots, avec reprise et priorités.
- [ ] **Revue des travaux d'équipe** : montrer le propriétaire, les changements de
      fichiers, les résultats et les erreurs avant de reprendre ou réassigner une tâche.

## Phase 4 — Différenciation (sécurité & missions)

Ce qui distingue Aucteris d'un simple client de chats agent — la valeur propre du
produit, Linux-first.

- [ ] **Egress enforcement réel** : netns dédié + nftables pour remplacer la médiation
      proxy (explicitement notée « future work » dans
      [crates/core/src/infrastructure/egress.rs](crates/core/src/infrastructure/egress.rs)).
- [ ] **Isolation par défaut à tous les niveaux UI**, et niveaux d'isolation par tâche de
      mission exposés dans l'UI (aujourd'hui seulement le cycle sandbox du bot).
- [ ] **Vagues parallèles avec workspace par tâche** (« like make -j », noté dans
      [crates/core/src/application/executor.rs](crates/core/src/application/executor.rs)).
- [ ] **Approval gates quotidiens** : missions interactives avec gates dans le chat,
      pas seulement via `run-durable`.
- [ ] **Evidence viewer** : parcourir `evidence.jsonl` et les rapports dans l'UI desktop
      (assertions agent vs observations runtime vs vérification indépendante).
- [ ] **Usage réel par bot/mission** : quotas, historique, alertes.

## Phase 5 — MVP fonctionnel et première livraison Linux

Cette phase ferme les parcours produit essentiels et prépare une version installable.
Les intégrations restent locales ou configurées par l'utilisateur ; elles ne supposent
pas un compte Aucteris ni un service distant.

- [ ] **MCP complet par bot** : serveurs multiples, store de config, OAuth (au-delà du
      seul aucteris-memory actuel), secrets protégés, état de connexion et révocation.
- [ ] **Skills** : bibliothèque locale partagée, révisions versionnées, création
      assistée, activation par bot et permissions requises affichées.
- [ ] **Routines planifiées locales** : créer, modifier, mettre en pause, consulter
      l'historique et reprendre après redémarrage.
- [ ] **Daemon headless** : `aucteris serve` évolue en hôte de bots sans fenêtre
      pour exécuter les routines et les missions ; démarrage et arrêt documentés.
- [ ] **CLI/API locale d'automatisation** : déclencher un run ou une routine, suivre
      son résultat et répondre aux questions/approbations ; écoute loopback, jeton
      révocable, limites de débit par bot et refus des appels web cross-origin.
- [ ] **Distribution Linux** : AppImage/Flatpak/deb, prérequis d'isolation expliqués,
      profil AppArmor si nécessaire.
- [ ] **Parcours MVP complet** : onboarding, création de bot, accès fichiers, connexion
      d'outils, exécution interactive, équipe avec tâches persistantes, routine locale,
      CLI/API locale, mission vérifiée, reprise après redémarrage et export/sauvegarde
      locale.
- [ ] **Critère de sortie MVP** : chaque parcours ci-dessus fonctionne sur une
      installation Linux propre, sans configuration développeur ni service hébergé ;
      le défaut est `WorkspaceWrite`, les droits dossier distinguent lecture et écriture,
      les commandes respectent les permissions effectives et les runtimes incompatibles
      n'affichent jamais une fausse garantie de confinement. Les erreurs de runtime,
      permission, connecteur et reprise sont explicites.

## Phase 6 — Extensions avancées (après le MVP)

Ces chantiers élargissent les surfaces, la mobilité et l'automatisation au-delà du
poste Linux local. Ils commencent une fois le critère de sortie de la phase 5 validé.

- [ ] **Client web / accès distant** : le moteur embarquable permet d'exposer le core ;
      le port orchestrator est le point de départ.
- [ ] **Client mobile** pour notifications et contrôle léger, relié à un hôte Aucteris
      distant ; l'hôte reste la source de vérité des chats, fichiers et exécutions.
- [ ] **Synchronisation multi-appareils** : définir un protocole chiffré et contrôlé
      par l'utilisateur pour bots, chats et pièces jointes ; décider d'abord si cela
      signifie cache local d'un hôte ou réplication, puis spécifier conflits, révocation
      et fonctionnement hors-ligne.
- [ ] **Navigateur embarqué contrôlable** : CDP depuis Rust (chromiumoxide ou
      équivalent), profils par bot et autorisations explicites.
- [ ] **Webhooks entrants/sortants** pour déclencher ou notifier des routines.
- [ ] **Connecteurs de messagerie tiers** (par exemple Slack, Discord ou WhatsApp),
      avec routage, secrets et permissions documentés.
- [ ] **Marketplace/plugins** : seulement après stabilisation des skills et de leur
      modèle de permissions.
- [ ] **Voix et réunions** : appels vocaux, dictée et enregistrement/transcription
      locale, avec consentement et indicateurs de capture visibles.
- [ ] **Computer use / capture d'écran** : contrôle explicite, par session, borné par
      les permissions du bot.
- [ ] **Agents cloud** : évaluer seulement avec un modèle d'hébergement, de données et
      de permissions compatible avec le positionnement local-first.
- [ ] **macOS/Windows dégradés** : sans bwrap/cgroups → avertissement d'isolation
      explicite ; sandbox-exec (seatbelt) sur macOS.
- [ ] **i18n** et autres plateformes après la première livraison Linux.

## Hors périmètre du MVP

Comptes cloud Aucteris, serveurs hébergés et facturation · app mobile · CRM ·
messageries tierces (Slack, Discord, Telegram, WhatsApp). La voix, la synchronisation,
le navigateur, l'accès distant et le computer use sont des extensions post-MVP de la
phase 6. Aucteris reste utilisable localement et sans backend dans toutes les phases.

## Suivi

- **Phase 0**
  ([#2](https://github.com/AUCTERIS/aucteris/issues/2)) : un seul chemin d'exécution,
  persistance fidèle, CI avec tests d'intégration verts.
- **Phase 1** : crate runtime extrait, plus aucun fichier UI > 600 lignes, tokens design
  utilisés partout, shell redessiné.
- **Phase 2** : permissions fichier vérifiables, pièces jointes réelles, file persistante,
  compaction et mémoire branchées sur les chats.
- **Phase 5 / MVP** : parcours de bout en bout installé sur Linux, avec providers
  configurables, queues durables, équipes, MCP, skills, routines et API locales, missions
  vérifiées et reprise après redémarrage ; aucun compte ni service hébergé requis.
- **Phase 6** : aucun chantier avancé ne bloque la livraison MVP ; navigateur,
  synchronisation, accès distant et voix ont leurs propres critères de sécurité et de
  disponibilité.
- Chaque item se vérifie via `cargo test --workspace` + les commandes de
  [ARCHITECTURE.md](ARCHITECTURE.md).
