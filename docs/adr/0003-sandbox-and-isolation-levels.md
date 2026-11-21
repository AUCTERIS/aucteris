# ADR-0003: Niveaux d'isolation — sandbox moteur vs sessions runtime-assistées

**Date**: 2026-11-20
**Status**: accepted
**Deciders**: jomvick
**Issue**: [AUCTERIS/aucteris#9](https://github.com/AUCTERIS/aucteris/issues/9)

## Context

Deux chemins d'exécution coexistent depuis ADR-0001, avec des frontières de
confiance différentes :

- **Missions core** (`RuntimeAdapter::run`) : le processus CLI tourne sous le
  contrôle du moteur — bubblewrap, cgroups, proxy egress, supervision PID.
  C'est le niveau le plus fort qu'Aucteris puisse garantir lui-même.
- **Sessions harness** (chats desktop, `HarnessMissionAdapter`) : le driver
  komet lance le CLI avec sa propre sandbox (`SandboxLevel` demandé par
  l'utilisateur). Aucteris **ne** supervise ni le filesystem ni le réseau au
  niveau OS — la sandbox effective dépend du driver CLI sous-jacent.

Le principe produit (« jamais de full-access implicite ») exige que l'écart
entre isolation *demandée* et isolation *effective* soit visible, jamais
silencieux.

## Decision

Trois niveaux coexistent, et le code les distingue explicitement :

1. **`IsolationLevel::EngineSupervised`** — missions core : bwrap + cgroups +
   egress proxy + supervision. Le moteur est l'autorité.
2. **`IsolationLevel::RuntimeAssisted`** — sessions harness : la sandbox est
   *demandée* via `SandboxOptions`/`SandboxLevel` et *appliquée* par le driver
   runtime (flags natifs claude/codex/grok, permissions ACP). Aucteris rapporte
   le niveau demandé et les `limitations` déclarées par l'adapter ; il ne
   prétend jamais à une isolation moteur qu'il n'exerce pas.
3. **`SandboxLevel::DangerFullAccess`** — explicite uniquement : `auto_approve`
   n'est activé que pour ce niveau, jamais par défaut.

Règles qui en découlent :

- Un adapter qui ne peut pas appliquer le niveau demandé doit le déclarer dans
  `capabilities().limitations` — pas de downgrade silencieux.
- `allow_network: true` dans `CliBackend` est un shortcut documenté : les CLI
  API ont besoin du réseau pour joindre leur provider à travers bwrap ;
  l'egress filtré (`EgressProxy`) n'est pas encore câblé sur ce chemin.
- L'UI affiche le niveau demandé (`sandbox_label`) ; la garantie effective
  reste celle du driver — la distinction est écrite ici pour que personne ne
  la découvre à la lecture du code.

## Consequences

- Le chemin le plus fréquent (chats desktop) est `RuntimeAssisted` : sa
  sécurité réelle dépend des drivers komet et des CLI sous-jacents, pas du
  moteur. C'est le prix de la couche session unifiée (ADR-0001).
- Renforcer l'isolation session = soit passer `CliBackend` sous le sandbox
  process-level du moteur, soit fiabiliser les drivers komet (ex. activation
  effective de la sandbox Claude — suivi dans `jomvick/komet`).
- Tout nouvel adapter doit déclarer honnêtement son `IsolationLevel` et ses
  limitations ; ajouter un niveau sans enforcement réel est un bug, pas une
  feature.

## Alternatives considered

- **Tout passer sous le sandbox moteur** — rejeté : les sessions komet ont
  besoin de la gestion de permissions interactive, du resume et du steering
  que le chemin `RuntimeAdapter::run` ne porte pas. L'unification se fait au
  niveau *contrat* (ADR-0001), pas au niveau confinement OS.
- **Prétendre que `SandboxLevel` = isolation effective** — rejeté : le niveau
  affiché est celui *demandé* ; l'application dépend du driver. L'honnêteté
  exige la distinction `RuntimeAssisted`.
