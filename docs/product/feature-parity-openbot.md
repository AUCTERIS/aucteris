# Comparaison fonctionnelle — Aucteris et OpenBot

> Référence : le dépôt local OpenBot, audité à partir de son README et de sa
> documentation d'architecture. L'audit porte sur les fonctions décrites dans le code
> et la documentation, pas sur un essai manuel de chaque parcours.

## Résumé

OpenBot est un produit plus large qu'un client desktop d'agents : il combine de nombreux
fournisseurs, des queues durables, des tâches secondaires d'agents, des canaux partagés,
des routines, des connecteurs et des clients web/mobile reliés à un serveur hôte.

Aucteris possède déjà le socle runtime, les conversations persistantes, les missions
vérifiables et une architecture locale avec isolation. Sa roadmap couvre les principaux
écarts du MVP : permissions, queues, modèles et endpoints, pièces jointes, MCP, skills,
routines locales, équipes et revue des preuves. Le navigateur, l'accès distant et les
clients multi-appareils restent en phase 6.

## Matrice des capacités

| Domaine | OpenBot documenté | Aucteris aujourd'hui / roadmap | Priorité Aucteris |
|---|---|---|---|
| Fournisseurs et modèles | Plus de dix providers/harnesses, endpoints OpenAI compatibles, agents ACP personnalisés, détection d'Ollama et LM Studio | Claude, Codex, OpenCode, Gemini, Devin et Grok via plusieurs chemins ; ACP et endpoints custom prévus en phase 2 | **MVP** : finir ACP, endpoints locaux/custom, catalogue des modèles et état de disponibilité |
| Sessions et contexte | Session persistante par agent, reprise après redémarrage, suivi de contexte et compaction automatique | Chats et sessions persistants ; compaction, mémoire chat et queues durables planifiées en phase 2 | **MVP** : queue FIFO crash-safe, compaction, mémoire et statut de run visibles |
| Fichiers | Workspace par agent, pièces jointes et transfert de fichiers entre agents | Dossier de travail par bot ; permissions et pièces jointes réelles en phase 2 ; transfert supervisé en phase 3 | **MVP** : contrôle d'accès par bot et pièces jointes ; transfert d'équipe sous permissions |
| Multi-agent | Tâches secondaires avec état propre, messages, demandes/réponses, arrêt, reprise, reprise de tâche et fichiers | Rooms de base ; délégation, propriétaire, messages inter-agents, historique, files et transferts en phase 3 | **MVP** : owner, statut, messages, reprise et revue des changements dans les équipes |
| Canaux partagés | Conversations partagées avec membres humains et agents, et contrôles d'équipe | Rooms prévues comme base, pas encore un workflow complet | **MVP** : channels avec assignation et historique commun, sans présence distante requise |
| Automatisation | Routines planifiées ou déclenchées, historique, reprise et API locale de scripts | Routines locales et daemon headless prévus en phase 5 ; CLI actuel orienté missions | **MVP** : routines locales durables et interface CLI/API locale sécurisée pour les déclencher et suivre leurs résultats |
| MCP, skills et extensions | Plusieurs serveurs MCP, OAuth, skills multi-provider, plugins et catalogue | MCP de mémoire seulement ; MCP multi-serveurs et bibliothèque de skills prévus en phase 5 | **MVP** : MCP configurable et skills locales ; marketplace/plugins en phase 6 |
| Navigateur et contrôle ordinateur | Navigateur intégré et Computer Use documentés | Absents ; déjà reclassés en phase 6 | **Post-MVP** : navigateur contrôlé et Computer Use sous permissions explicites |
| Clients web et mobile | Client web et app mobile reliés à un hôte ; accès à distance aux conversations et équipes | Application desktop GPUI et CLI ; aucun client web/mobile prévu avant le MVP | **Post-MVP** : accès distant via hôte rejoint, clients web/mobile selon besoins |
| Hôte et données | Base SQLite sur l'hôte ; l'infrastructure de compte gère l'identité et les sessions, pas les conversations/fichiers. Le mobile conserve une copie chiffrée en lecture seule de métadonnées | Core local-first, sans compte ni backend obligatoire ; daemon headless prévu en phase 5 | **MVP** : sauvegarde/export local et reprise fiable. **Post-MVP** : accès distant et éventuellement réplication contrôlée |
| Sécurité | Le README indique que les agents démarrent en accès étendu après consentement initial ; les connecteurs et pages visitées utilisent le réseau | Permissions explicites, sandbox Linux, approbations, cgroups et médiation egress ; enforcement réseau complet encore planifié | **Avantage à préserver** : ne pas reprendre l'accès étendu par défaut ; rendre les capacités et limites de chaque runtime visibles |
| Missions et preuves | Agent de travail, workflows et queues documentés ; aucun équivalent exact au moteur de missions Aucteris identifié dans les sources auditées | Missions YAML, vérifications indépendantes, journal de preuves et orchestrateurs | **Avantage Aucteris** : exposer le cycle mission, les approbations et l'evidence viewer dans le desktop |

## Ce qu'il faut reprendre pour le MVP

1. **Rendre les runs fiables** : queue persistante par bot, annulation, pause/reprise,
   crash recovery, compaction et visibilité du contexte.
2. **Rendre les équipes utiles** : propriétaire de tâche, messages agent-à-agent,
   files cross-bots, historique commun, transfert supervisé et revue des changements.
3. **Relier le core aux usages quotidiens** : CLI/API locale sécurisée pour déclencher
   une routine, envoyer un prompt, recevoir un résultat et traiter les questions ou
   approbations permises.
4. **Compléter les capacités locales** : providers ACP, endpoints custom/local,
   disponibilité, pièces jointes, MCP multi-serveurs et skills par bot.
5. **Faire des missions une fonctionnalité desktop** : créer ou lancer une mission,
   suivre les tâches, traiter les gates, consulter les preuves et relancer une
   vérification.

Ces ajouts gardent le produit utilisable sur une seule machine, sans compte ni serveur
hébergé. Les clients web/mobile et les hôtes distants nécessitent une phase distincte.

## À garder pour la phase 6

- Navigateur intégré contrôlable et Computer Use.
- Client web/mobile et accès à un hôte rejoint ou auto-hébergé.
- Synchronisation ou réplication multi-appareils optionnelle. OpenBot garde SQLite sur
  l'hôte comme source de vérité ; son cache mobile documenté est une copie locale chiffrée
  et en lecture seule de certaines métadonnées, pas une synchronisation générale.
- Webhooks, marketplace/plugins, voix, agents hébergés et connecteurs de messagerie
  tiers, après cadrage des permissions et de l'exploitation.

## Base de l'audit et limites

- `README.md` : providers supportés, fonctionnement annoncé, sécurité, données locales
  et plateformes distribuées.
- `docs/architecture/agents.md` : sessions, queues, tâches secondaires, routines et
  scripts locaux.
- `docs/architecture/providers.md` et `docs/architecture/integrations.md` : providers,
  ACP, endpoints et parcours d'intégration/MCP.
- `docs/architecture/channels-and-messaging.md` : canaux, messageries, fonctions mobile.
- `docs/architecture/skills-and-plugins.md` et `docs/automation.md` : skills, plugins,
  routines et API locale.
- `docs/architecture/browser.md`, `docs/architecture/computer-use.md`,
  `docs/web-client.md` et `docs/hosted-servers.md` : navigateur, contrôle ordinateur,
  clients distants et hébergement.
- Les capacités OpenBot sont établies à partir des sources du dépôt. Elles n'ont pas
  toutes été validées par un essai utilisateur. Les choix de sécurité et d'architecture
  d'OpenBot ne sont pas automatiquement des objectifs Aucteris.
