# ADR-0001: Une couche runtime/session unique (`crates/harness`)

**Date**: 2026-11-20
**Status**: accepted
**Deciders**: jomvick
**Issue**: [AUCTERIS/aucteris#3](https://github.com/AUCTERIS/aucteris/issues/3)

## Context

Deux chemins d'exécution coexistent :

- **Chats desktop** : `apps/desktop/src/application/session.rs` pilote
  `komet-harness` — sessions persistantes/resumables, steering, approvals et
  questions interactives. Gemini n'a pas de driver komet et retombe sur un
  chemin dédié (`Engine::run_mission`) qui soumet une mission core à un
  `RuntimeAdapter`.
- **Missions core** : `RuntimeAdapter::run` (`crates/core`) spawn un processus
  CLI nu, streame des `RuntimeEvent` bruts vers un fichier JSONL et retourne un
  `RunOutcome`. Pas de session id, pas d'approvals par outil, pas de steering.

Chaque feature (approvals, steering, sessions, nouveaux runtimes) doit être
implémentée deux fois, et les capacités divergent silencieusement selon le
chemin emprunté.

## Decision

Extraire une crate workspace **`aucteris-harness`** (`crates/harness`) qui
constitue la couche runtime/session unique, au-dessus de `aucteris-core` et de
`komet-harness` :

- **`AgentBackend`** : notre trait unifié (`id`, `supports_steering`,
  `models`, `run(SessionRequest, SessionControls) -> stream d'AgentEvent`).
- **`KometBackend`** : enveloppe `Arc<dyn komet_harness::Harness>` (claude-code,
  codex, opencode, devin, grok, et tout futur driver komet).
- **`CliBackend`** : enveloppe un `aucteris_core::RuntimeAdapter` pour les
  runtimes sans driver komet (gemini, mock). Il synthétise des `AgentEvent`
  depuis les `RuntimeEvent` et rapporte honnêtement ses capacités dégradées
  (pas de resume, pas de steering, pas d'approvals par outil).
- **`Sessions`** : la machinerie parked permissions/questions/steering/interrupt
  déplacée depuis le desktop, émettant le vocabulaire d'événements de session.
- **`HarnessMissionAdapter`** : implémente `aucteris_core::RuntimeAdapter` par-
  dessus un `AgentBackend`, pour que les missions puissent consommer la même
  couche. Enregistré explicitement via `RuntimeRegistry::register` par les
  applications qui veulent la sémantique session.

`aucteris-core` reste libre de dépendance vers komet : le port
`RuntimeAdapter`/`RuntimeRegistry` est inchangé structurellement, enrichi de
champs optionnels (`resume_session`, `session_id`, interactions, steering).

### Conséquences sur les missions

- `Attempt.session_id` et `TaskRun.session_id` sont persistés ; une nouvelle
  tentative d'une même tâche reprend la session agent (`resume_session`).
- `RunRequest` gagne un port `interactions` (approvals par outil) et un canal
  de steering. `HarnessMissionAdapter` les traduit vers `RunControls` komet.
- **Trade-off assumé** : une mission exécutée via `HarnessMissionAdapter` perd
  la supervision moteur (bwrap, cgroup, proxy egress, pid_file) — komet-harness
  possède le spawn du CLI. Les adapters CLI historiques restent disponibles
  dans le registry pour les missions qui exigent l'isolation moteur ; les
  limitations sont enregistrées dans `attempt.limitations`, jamais silencieuses.

## Alternatives Considered

### Alternative A : adapter harness dans `crates/core`
- **Pros** : une seule crate de plus, registre unique par défaut.
- **Cons** : `aucteris-core` (utilisé par le CLI, les workers restate/temporal,
  et conçu headless) hériterait de la dépendance git `komet-harness` ; la
  frontière « moteur vs drivers » disparaît ; le registry par défaut devrait
  choisir entre supervision moteur et sessions — un choix qu'aucun code ne peut
  faire honnêtement pour tous les usages.
- **Why not** : la séparation est déjà dans la ROADMAP (Phase 1 prévoit
  `crates/harness`) et garde core réutilisable sans komet.

### Alternative B : tout passer par le chemin mission (supprimer komet côté chat)
- **Pros** : zéro duplication, une seule implémentation.
- **Cons** : les chats perdent sessions persistantes, steering, approvals et
  questions interactives — le cœur de l'expérience desktop. Les `RuntimeEvent`
  core sont trop pauvres pour l'UI (pas de diffs, pas de ToolProgress, pas de
  questions).
- **Why not** : régression produit majeure.

### Alternative C : ne rien faire / deux chemins assumés
- **Cons** : chaque feature payée deux fois, capacités divergentes, Gemini sur
  un third chemin.
- **Why not** : c'est exactement la dette que l'issue #3 demande de payer.

## Consequences

### Positive
- Chats et missions consomment la même couche session ; un nouveau runtime
  komet profite aux deux sans code dupliqué.
- Gemini n'a plus de chemin UI dédié : `CliBackend` le présente comme n'importe
  quel backend, avec capacités honnêtes.
- Les missions obtiennent des session ids persistés, des approvals par outil
  (port `interactions`, opt-in par spec) et le steering (canal par tentative).
- `apps/desktop` ne dépend plus directement de `komet-harness`.

### Negative
- Double surface à maintenir : `RuntimeAdapter` (processus supervisés) et
  `AgentBackend` (sessions) coexistent ; `HarnessMissionAdapter` est le pont.
- Missions via harness : pas de bwrap/cgroup/egress moteur (voir trade-off).
- `CliBackend` pour gemini reste une session « dégradée » : single-shot, sans
  resume/steer/approvals — Gemini n'a tout simplement pas de protocole
  interactif exploitable en headless.

### Risks
- **Dérive des deux ports** : mitigé par `HarnessMissionAdapter` qui prouve que
  `AgentBackend` → `RuntimeAdapter` est possible, et par les tests d'intégration
  mock des deux côtés.
- **Kompiation des capacités** : `CliBackend` doit mentir sur ses limites →
  `capabilities()` explicite et `limitations` propagées aux tentatives.
