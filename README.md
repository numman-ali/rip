<div align="center">

<img src="./assets/logo.svg" alt="rip" width="420" />

<br/>
<br/>

**The fastest coding agent harness. Built in Rust.**

A modular, pluggable runtime for autonomous coding agents.

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

[Philosophy](#-philosophy) · [Architecture](#-architecture) · [Surfaces](#-surfaces) · [Documentation](#-documentation)

</div>

---

## Coming Soon

**rip** is under active development. The codebase and documentation are open for exploration, but the project is not yet ready for production use.

What you can do now:
- Read the docs to understand the methodology and thinking
- Explore the architecture and design decisions
- Follow along as we build

---

## Philosophy

rip is built on a few core beliefs:

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   Performance is non-negotiable                             │
│   Modularity enables evolution                              │
│   Contracts enforce correctness                             │
│   Replay enables debugging                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Why rip?**

- **Extreme performance** — Sub-millisecond overhead on streaming, parsing, tool dispatch
- **Modular by design** — Every component is replaceable via strict contracts
- **Deterministic replay** — Event log + snapshots enable full session replay
- **Multiple surfaces** — CLI, TUI, server, SDK — all powered by one core runtime

---

## Architecture

One core runtime powers every surface:

```
[ rip ]        (interactive TUI) ─────┐
[ rip run ]    (headless CLI)    ─────┼──▶ [ ripd core ] ──▶ [ providers ]
[ rip-sdk-ts ] (TypeScript SDK)  ─────┘         │
                                                 ├──▶ scheduler + subagent manager
                                                 ├──▶ tool runtime + registry
                                                 ├──▶ context compiler
                                                 ├──▶ workspace engine
                                                 └──▶ event log + snapshots
```

Key constraints:
- All inter-module traffic is structured events, not raw text
- Every module is replaceable via strict contracts
- Determinism: event log + snapshots enable full replay

---

## Surfaces

| Surface | Description | Status |
|---------|-------------|--------|
| `rip` | Interactive fullscreen TUI | Phase 1 |
| `rip run --headless` | Machine-friendly JSON output | Phase 1 |
| `ripd` | Server with HTTP/SSE API | Phase 1 |
| `rip-sdk-ts` | TypeScript SDK | Phase 1 |
| `rip-tui` | Rich terminal UI | Phase 2 |
| `rip-mcp` | MCP capability exposure | Phase 2 |

All surfaces are thin adapters over the same core session + capability API. No surface owns business logic.

---

## Documentation

The docs are written for agents, not humans. Short summaries first, details below.

**Start here:**

| Doc | Description |
|-----|-------------|
| [docs/00_index.md](docs/00_index.md) | Entry point and navigation |
| [docs/01_north_star.md](docs/01_north_star.md) | Vision, non-negotiables, performance budgets |
| [docs/02_architecture/](docs/02_architecture/) | Component map, surfaces, capability matrix |
| [docs/03_contracts/](docs/03_contracts/) | Module contracts and event schemas |
| [docs/06_decisions/](docs/06_decisions/) | Architecture decision records (ADRs) |
| [docs/07_tasks/roadmap.md](docs/07_tasks/roadmap.md) | Now / Next / Later roadmap |

---

## Project Structure

```
crates/
├── rip-cli/      # CLI entrypoint
├── rip-kernel/   # Core runtime + event frames
├── rip-server/   # HTTP/SSE server
├── rip-tools/    # Built-in tool implementations
├── rip-tui/      # Terminal UI rendering
└── ripd/         # Agent runtime daemon

sdk/
└── typescript/   # TypeScript SDK

docs/             # Source of truth for scope, architecture, contracts
```

---

## Status

**Phase 1** (foundation): Core runtime, provider adapters, tool runtime, workspace engine, CLI, server, benchmarks.

**Phase 2** (planned): Rich TUI, MCP surface, search/memory, context compiler, background workers.

---

## License

Apache 2.0
