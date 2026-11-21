# Architecture documentation

| Document | Content |
|---|---|
| [VERTICAL-SLICE-01.md](VERTICAL-SLICE-01.md) | The implemented mission slice: schema, execution flow, statuses, evidence, persistence |
| [../adr/](../adr/) | Architecture Decision Records (0001 runtime layer, 0003 sandbox/isolation) |

The full numbered architecture set (00–13) lives in the companion repository
[`AUCTERIS/aucteris-architecture`](https://github.com/AUCTERIS/aucteris-architecture/tree/main/docs/architecture).
Code comments cite it as `aucteris-architecture/<doc> §<section>`.

## Diagrams

Canonical diagrams are Mermaid blocks inside the documents above and in
[ARCHITECTURE.md](../../ARCHITECTURE.md) — they render on GitHub and stay
versioned with the code they describe. Generated HTML/archify exports are not
committed; regenerate them from the Mermaid sources when needed.
