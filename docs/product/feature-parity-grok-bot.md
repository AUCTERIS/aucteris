# Comparaison fonctionnelle — Aucteris et Grok Bot

> Référence : l'application desktop locale `Grok Bot` 1.2.3 du dossier `../grok`,
> extraite le 18 novembre 2026. Ce document compare cette build avec l'état du dépôt
> Aucteris au 24 novembre 2026. « Grok Bot » désigne ici cette application locale,
> pas le chatbot xAI disponible sur grok.com.

## Résumé

Aucteris a déjà un socle solide pour exécuter des agents de code : chats persistants,
plusieurs harnesses, contrôle interactif, et un moteur de missions avec sandbox et
vérification indépendante. Grok Bot expose une surface de produit plus large autour
des agents : connecteurs, outils, skills, voix, automatisations, expérience navigateur
et parcours d'agents cloud.

Pour construire une parité utile, terminer les permissions et les tâches quotidiennes
dans le desktop, puis ajouter des capacités d'extension et d'intégration configurables.

## Matrice des capacités

| Domaine | Grok Bot 1.2.3 observé dans le paquet | Aucteris aujourd'hui | Écart |
|---|---|---|---|
| Chat agent | Conversations d'agents, streaming, outils et commandes locales | Chats persistants, streaming, appels d'outils et résultats | **Proche** : stabiliser l'UX et l'état d'exécution par chat |
| Modèles et runtimes | Catalogue de modèles, agents locaux et cloud, choix de skills/plugins | Harnesses CLI Claude, Codex, OpenCode, Gemini, Devin, Grok ; pas de catalogue général de modèles | **Partiel** : modèles et endpoints custom manquent |
| Fichiers et contexte | Travail dans un workspace, pièces jointes et éditeur/contenu de fichiers | Un dossier de travail par bot ; pièce jointe non implémentée ; permissions par bot planifiées | **Écart prioritaire** : accès lisible, écritures graduées, pièces jointes réelles |
| Outils et intégrations | Connecteurs natifs, plugins, comptes de service, formulaires MCP personnalisés | Injection MCP limitée à la mémoire Aucteris ; pas de centre de connecteurs | **Écart fort** : gestion MCP multi-serveurs et secrets par bot |
| Skills | Bibliothèque et configuration de skills, sélection pendant la configuration d'agents | Pas de bibliothèque de skills | **Écart fort** : catalogue local versionné et activation par bot |
| Tâches récurrentes | Modules d'automatisation et panneaux de gestion d'automatisations présents dans l'interface | Pas de planification intégrée | **Écart fort** : routines locales avec historique, pause et reprise |
| Voix et réunions | Appels vocaux, mémos vocaux, enregistrement de réunions et contrôles de transcription visibles dans les bundles | Hors périmètre explicite de la roadmap actuelle | **Écart de périmètre** : à différer après les usages desktop essentiels |
| Navigateur / computer use | Modules navigateur et capture/interaction avec l'écran dans le paquet | Pas de navigateur embarqué ni de computer use ; remote desktop hors périmètre | **Écart fort**, mais coûteux et sensible aux permissions |
| Multi-agent | Équipes, groupes, délégation et agents cloud apparaissent dans les surfaces et modules | Rooms/équipes de base ; tâches partagées et délégation multi-agent non terminées | **Partiel** : compléter ownership, messages, transfert et reprise |
| Sécurité d'exécution | Daemon local, exécution de commandes et demandes de permissions dans le paquet | bwrap, cgroups, médiation egress ; enforcement réseau complet et matrice de capacités encore planifiés | **Aucteris a une base différenciante** ; rendre les garanties explicites et cohérentes |
| Missions vérifiables | Aucun équivalent de mission avec preuve de vérification identifié dans les surfaces examinées | YAML, tâches, retries, assertions `verify_cmd`, evidence append-only, orchestrateurs local/Temporal/Restate | **Avantage Aucteris** : transformer cette puissance en parcours desktop |
| Extensions et distribution | Marketplace/connecteurs visibles dans l'application | Pas de marketplace ; distribution desktop en phase future | **Écart**, à ne pas commencer avant MCP et skills stables |

## Ordre de rattrapage recommandé

### P0 — rendre le quotidien fiable

1. Terminer le modèle de permissions par bot : espace privé automatique, lecture,
   écritures approuvées, dossiers autorisés révocables, capacités réelles par runtime.
2. Remplacer l'action factice de pièce jointe par un vrai sélecteur et des pièces
   jointes persistées dans le transcript.
3. Ajouter les files d'exécution par bot, avec annulation, pause/reprise et reprise
   après redémarrage ; afficher clairement les runs et leur contexte.
4. Brancher mémoire, compaction de contexte et visibilité d'usage aux chats, pas
   seulement aux missions.

Ces éléments sont déjà inscrits dans les phases 1 et 2 de [`../../ROADMAP.md`](../../ROADMAP.md).

### P1 — ouvrir l'écosystème local

1. Ajouter un registre MCP par bot : plusieurs serveurs, configuration validée,
   secrets protégés, état de connexion et actions de révocation.
2. Créer une bibliothèque de skills locales avec version, activation par bot et
   explication des permissions requises.
3. Ajouter les endpoints compatibles OpenAI et la détection d'Ollama/LM Studio,
   sans imposer un compte cloud Aucteris.
4. Livrer les routines planifiées locales en phase 5. Garder le navigateur contrôlable
   pour la phase 6, après définition de ses permissions.

### P2 — compléter le travail en équipe

1. Compléter les rooms : owner de tâche, messages agent-à-agent, historique partagé,
   stop/reprise/réassignation/archivage et transfert de fichiers supervisé.
2. Ajouter des tâches parallèles avec un workspace isolé par tâche et un écran de
   revue des changements.
3. Exposer les missions dans le desktop : démarrer, approuver, suivre les étapes,
   consulter les preuves et relancer une vérification.

### P3 — élargir les surfaces

Voix, enregistrement de réunions, marketplace/plugins, accès distant et agents cloud
sont des projets séparés. Ils demandent des choix de confidentialité, d'architecture
et de distribution. Les planifier après les premières livraisons préserve les priorités
d'Aucteris : contrôle local, permissions explicites et résultats vérifiables.

## Base de l'audit et limites

- Aucteris : [`README.md`](../../README.md), [`ROADMAP.md`](../../ROADMAP.md),
  [`VERTICAL-SLICE-01.md`](../architecture/VERTICAL-SLICE-01.md), le modèle desktop
  et le moteur sous `apps/desktop/src/` et `crates/core/src/`.
- Grok Bot : README local, `source/package.json`, chunks du renderer nommés
  `chunk-browser-*`, `chunk-connect-*`, `chunk-custom-mcp-form-*`, `chunk-voice-*`,
  `chunk-automation-*`, `chunk-team-*`, plus les modules récupérés d'exécution
  locale et de coordination d'agents.
- Le paquet est compilé et obfusqué en partie. La présence d'un bundle ou d'un nom
  de composant prouve que la capacité est incluse dans cette build, mais pas que
  toutes les fonctions sont activées pour chaque compte, région ou configuration.
  Les lignes « Grok Bot observé » sont donc un inventaire de capacités présentes,
  pas une promesse de disponibilité universelle.
- La roadmap Aucteris décrit une cible encore en cours ; « aujourd'hui » distingue
  les fonctions déjà câblées de celles explicitement planifiées.
