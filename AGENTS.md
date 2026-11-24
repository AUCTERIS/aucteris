# AGENTS.md

## Skill-Driven Workflow

This project uses [Superpowers](https://github.com/obra/superpowers) skills for structured development workflows.

### Core Rules

- If a task matches a skill, invoke it using the `skill` tool
- Superpowers skills are auto-discovered via the plugin; project skills live in `.opencode/skills/`
- Never implement directly if a skill applies
- Always follow the skill instructions exactly (do not partially apply them)

### Intent → Skill Mapping

- Any conversation start / unsure → `using-superpowers`
- Creative work, features, components → `brainstorming`, then `writing-plans`, then `subagent-driven-development`
- Planning / multi-step tasks → `writing-plans`
- Executing a written plan → `executing-plans` or `subagent-driven-development`
- Bug / failure / unexpected behavior → `systematic-debugging`
- Code review (requesting) → `requesting-code-review`
- Code review (receiving) → `receiving-code-review`
- Implementing any feature or bugfix → `test-driven-development`
- Parallel independent tasks → `dispatching-parallel-agents`
- About to claim work complete → `verification-before-completion`
- Finishing a branch / deciding next step → `finishing-a-development-branch`
- Git isolation / worktrees → `using-git-worktrees`
- Creating or editing skills → `writing-skills`

### Lifecycle (Implicit Commands)

- DEFINE → `brainstorming` or `writing-plans`
- PLAN → `writing-plans`
- BUILD → `subagent-driven-development` + `test-driven-development`
- VERIFY → `systematic-debugging` + `verification-before-completion`
- REVIEW → `requesting-code-review` + `receiving-code-review`
- SHIP → `finishing-a-development-branch`

### Execution Model

For every request:
1. Determine if any skill applies
2. Invoke the appropriate skill using the `skill` tool
3. Follow the skill workflow strictly
4. Only proceed to implementation after required steps are complete

## Icon Sourcing

- When adding icons for AI providers, models, or connected services, consult [LobeHub Icons](https://lobehub.com/icons/skill.md) as an optional catalog and visual reference. Check the icon table of contents for the exact ID and available variants; do not guess asset names.
- In React surfaces, use `@lobehub/icons` components where appropriate. In the native GPUI desktop app, do not add the React package: use a pinned `@lobehub/icons-static-svg` asset when a matching mark exists, embed it under `apps/desktop/assets/icons/`, and register it in `apps/desktop/src/presentation/assets.rs`.
- Preserve the upstream license notice for copied assets. Prefer a color variant for a recognizable provider or service brand, and a monochrome/current-color variant when the existing interface tints icons.
- Keep using the existing Aucteris icon set for generic actions. Use a brand mark only when the action belongs to that service; if LobeHub has no matching mark, choose a relevant existing generic icon instead of an unrelated logo.

### Anti-Rationalization

The following thoughts are incorrect:
- "This is too small for a skill"
- "I can just quickly implement this"
- "I'll gather context first"

Correct behavior: Always check for and use skills first.
