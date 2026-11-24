# ROADMAP — Aucteris

> Dernière mise à jour : 2026-11-24 · Positionnement : **Linux-first** · Permissions et UX
> d'abord, features ensuite

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
   privé. Il peut lire les fichiers de l'ordinateur sans devoir ajouter chaque dossier ;
   les écritures suivent le niveau de permission choisi et les règles du runtime. Le
   full-access reste une élévation explicite.
6. Toute feature « quotidien agent » est validée par des tests d'intégration sur les
   fixtures mock existantes.
7. **Construit de bout en bout** : tout le code est écrit dans ce dépôt, hors dépendances
   déclarées (GPUI, komet-harness) ; aucune reprise de code produit tiers.

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
- [ ] **Centraliser l'état UI et la persistance** : un modèle d'actions unique, une
      écriture disque par mutation contrôlée (remplacer le compteur de génération).
- [ ] **Tests de composants** : text_input, widgets, dialog — au moins les chemins
      critiques testés.

**Redesign**

- [ ] **Design tokens** centralisés (espacements, rayons, typographie, couleurs,
      animations) dans [theme.rs](apps/desktop/src/presentation/theme.rs), consommés
      partout — plus de valeurs magiques.
- [ ] **Layout workspace** : shell redessiné — sidebar chats/rooms, zone de conversation,
      panneau latéral du bot, barre de statut ; hiérarchie visuelle claire et densité
      maîtrisée.
- [ ] **Identité visuelle propre** : palette signature, jeu d'icônes cohérent, mascottes
      réutilisées systématiquement, accents par bot.
- [ ] **Onboarding premier lancement** : création du premier bot guidée, états vides
      soignés.
- [ ] **Micro-interactions** : transitions douces (GPUI), focus clavier complet,
      caret/scroll soignés, feedbacks d'état (working/waiting/resting).
- [ ] **Accessibilité** : navigation clavier sans souris, contrastes validés en
      dark/light.

## Phase 2 — Permissions et quotidien agent

Priorité produit : rendre les accès compréhensibles, prévisibles et révocables avant
d'étendre les capacités des bots.

- [ ] **Modèle d'accès aux fichiers par bot** : créer automatiquement un espace privé
      pour chaque bot, sans demander de dossier lors de sa création. La lecture des
      fichiers de l'ordinateur est libre par défaut ; un dossier supplémentaire ne doit
      pas être requis pour lire.
- [ ] **Écritures pilotées par le niveau de permission** : `ReadOnly` interdit les
      modifications ; `WorkspaceWrite` autorise l'espace privé et les dossiers ajoutés
      par l'utilisateur, puis demande une approbation pour les autres écritures ;
      `DangerFullAccess` autorise les accès étendus uniquement après un choix explicite.
      Afficher le niveau effectif et les limites propres à chaque runtime.
- [ ] **Centre des permissions dans le panneau latéral du bot** : lister les dossiers
      autorisés, les ajouter manuellement, les retirer et révoquer leur accès. Une
      approbation d'écriture peut proposer d'ajouter le dossier concerné ; après accord,
      le dossier apparaît immédiatement dans cette liste.
- [ ] **Cartes d'approbation cohérentes** : présenter l'action et le chemin ciblé, avec
      des choix explicites pour autoriser une fois, autoriser et ajouter le dossier, ou
      refuser. L'approbation persistante ne doit pas être confondue avec l'autorisation
      d'une seule écriture.
- [ ] **Matrice de capacités des runtimes** : appliquer les dossiers ciblés seulement
      lorsque le runtime les prend réellement en charge ; signaler les limites et ne
      jamais afficher une autorisation persistante inactive. Garder OpenCode bloqué pour
      l'accès ciblé tant que son sandbox ne permet pas de sélectionner des chemins ;
      suivre séparément le support requis par l'intégration Codex/Komet.
- [ ] **Révocation et confinement** : répercuter un retrait sur les prochains runs,
      interrompre un run si nécessaire, vérifier les chemins canoniques et les liens
      symboliques, et empêcher qu'un dossier approuvé élargisse silencieusement son accès.
- [ ] **Persistance et migration** : sauvegarder les espaces privés et les autorisations
      par bot, avec des valeurs par défaut compatibles avec les profils existants.
- [ ] **Vérification des permissions** : couvrir chaque niveau d'accès, les demandes
      d'approbation, l'ajout/retrait de dossiers, les chemins invalides et les différences
      entre runtimes avec des tests d'intégration.

Une fois cette base établie, poursuivre le quotidien agent :

- [ ] **Files FIFO par bot** : pause/reprise/annulation, persistance crash-safe, reprise
      après redémarrage.
- [ ] **Compaction de contexte automatique** avant saturation de la fenêtre modèle
      (komet expose déjà `context_usage()`) + indicateur dans le chat.
- [ ] **Mémoires d'agent** sur le chemin sessions : brancher le MCP aucteris-memory
      (déjà injecté côté missions) à tous les chats + revue de mémoires.
- [ ] **Harnais ACP supplémentaires** : komet fournit déjà `AcpHarness` (cursor, cline,
      kimi, copilot, mistral, hermes) — les ajouter au catalogue + ProviderPicker.
- [ ] **Endpoints OpenAI-compatibles custom** + détection des serveurs locaux
      (Ollama, LM Studio) et des CLIs installés.
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
- [ ] **Transferts de fichiers** entre bots : manifest + dossier partagé supervisé.
- [ ] **Files de délégation** cross-bots : une file de tâches consommée par plusieurs
      bots, avec reprise et priorités.

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

## Phase 5 — Écosystème

- [ ] **MCP complet par bot** : serveurs multiples, store de config, OAuth (au-delà du
      seul aucteris-memory actuel).
- [ ] **Skills** : bibliothèque locale partagée, révisions versionnées, création
      assistée.
- [ ] **Navigateur embarqué contrôlable** : CDP depuis Rust (chromiumoxide ou
      équivalent), profils par bot — sans Electron.
- [ ] **Routines planifiées + webhooks** entrants/sortants.
- [ ] **Daemon headless** : `aucteris serve` évolue en hôte de bots sans fenêtre
      (l'orchestrator durable en est la base).
- [ ] **Marketplace/plugins** : seulement après des skills stables.

## Phase 6 — Ouverture (plus loin)

- [ ] **Client web / accès distant** : le moteur embarquable permet d'exposer le core ;
      le port orchestrator est le point de départ.
- [ ] **Distribution Linux** : AppImage/Flatpak/deb, profil AppArmor si besoin.
- [ ] **macOS/Windows dégradés** : sans bwrap/cgroups → avertissement d'isolation
      explicite ; sandbox-exec (seatbelt) sur macOS.
- [ ] **i18n**.

## Hors périmètre volontaire (pour l'instant)

Comptes cloud, serveurs hébergés et facturation · app mobile · voix · CRM ·
messageries tierces (Slack, Discord, Telegram, WhatsApp) · remote desktop / computer use.
Aucteris reste local-first pur, sans backend.

## Suivi

- **Phase 0**
  ([#2](https://github.com/AUCTERIS/aucteris/issues/2)) : un seul chemin d'exécution,
  persistance fidèle, CI avec tests d'intégration verts.
- **Phase 1** : crate runtime extrait, plus aucun fichier UI > 600 lignes, tokens design
  utilisés partout, shell redessiné.
- **Phase 2** : un bot peut enchaîner des prompts en file, avec compaction et mémoires,
  sur ≥ 8 harnais, sans perdre ses cartes au redémarrage.
- Chaque item se vérifie via `cargo test --workspace` + les commandes de
  [ARCHITECTURE.md](ARCHITECTURE.md).
