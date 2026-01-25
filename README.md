<div align="center">

<img src="./assets/logo.svg" alt="rip" width="420" />

<br/>
<br/>

**A Continuity OS for coding agents. Built in Rust.**

One thread that never ends — run headless or interactive without managing sessions.

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

[Quickstart](#quickstart) · [Continuity OS](#continuity-os-the-mental-model) · [Surfaces](#surfaces) · [Docs](#documentation)

</div>

---

> Imagine never having to manage sessions for your coding agent.  
> Imagine a world where it always knew everything you’d discussed — headless or interactive.  
> That’s the core of Rip: the Continuity OS.

## Rip, in one paragraph

rip is a high-performance harness for autonomous coding agents. Instead of treating each run as a disposable “chat session”, rip treats your work as a long-lived **continuity** (a thread that never ends) backed by an append-only event log. Runs are just compute jobs attached to that continuity. The result is a system that stays coherent across terminals and surfaces while keeping determinism, replayability, and performance as first-class constraints.

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
- **Deterministic replay** — Event log + snapshots enable reproducible debugging
- **Continuity OS** — A single thread is the default UX; sessions are runs (compute jobs), not user-facing state
- **Surface parity** — CLI, TUI, server, SDK are adapters over the same runtime + canonical frames

---

## Continuity OS (the mental model)

If you remember one thing: **the continuity event log is the source of truth**. Providers are replaceable compute; provider conversation state is a cache.

Core concepts:
- **Continuity (thread)**: durable identity + history (“one chat forever”).
- **Run (session/turn)**: a single execution job spawned from a continuity.
- **Store**: the persistence boundary for truth + artifacts (`RIP_DATA_DIR`, default: `./data/`).
- **Authority**: the single sequencer/writer for truth writes for a store (ADR-0019).

Local multi-terminal safety (v0.1):
- When you run `rip` / `rip run` locally, rip will auto-start (or auto-attach to) a **per-store local authority** so multiple terminals can safely share one store without corrupting event ordering.

---

## Architecture (high level)

One runtime + one canonical event format powers every surface:

```
[ rip ]        (interactive TUI) ──┐
[ rip run ]    (headless CLI)     ├──▶ [ authority (rip serve / ripd) ] ──▶ [ providers (Open Responses) ]
[ TS SDK ]     (programmatic) ────┘                 │
                                                    ├──▶ tool runtime + registry
                                                    ├──▶ workspace engine
                                                    ├──▶ continuity store + artifacts
                                                    └──▶ event log + snapshots (replay)
```

Key constraints:
- All inter-module traffic is structured events, not raw text.
- Every behavior-changing decision must be logged (determinism is non-negotiable).
- Provider state is not truth: Open Responses `previous_response_id` (and vendor thread ids) are treated as caches that can be rotated/rebuilt.

---

## Quickstart

### Install (from source)

```bash
cargo install --path crates/rip-cli
```

### Run (interactive)

```bash
rip
```

### Run (headless automation)

```bash
rip run "List the repo root files using the ls tool, then answer with just the filenames." --view output
```

### Connect a provider (Open Responses)

OpenAI:

```bash
OPENAI_API_KEY=... rip run "..." --provider openai --model <model_id>
```

OpenRouter:

```bash
OPENROUTER_API_KEY=... rip run "..." --provider openrouter --model <model_id> --stateless-history
```

### Choose store + workspace

```bash
RIP_DATA_DIR=/path/to/store RIP_WORKSPACE_ROOT=/path/to/repo rip run "..."
```

### Remote / multi-device

Start an authority on a stable address:

```bash
RIP_SERVER_ADDR=0.0.0.0:7341 rip serve
```

Attach from another terminal/machine:

```bash
rip --server http://<host>:7341
rip run --server http://<host>:7341 "..."
```

---

## Surfaces

| Surface | Description |
|---------|-------------|
| `rip` | Interactive fullscreen terminal UI |
| `rip run` | Headless CLI (JSONL frames or rendered output) |
| `rip serve` (`ripd`) | HTTP/SSE control plane (serves OpenAPI at `/openapi.json`) |
| `sdk/typescript` | TypeScript SDK (spawns `rip` by default; ADR-0006) |

All surfaces are thin adapters over the same core runtime and canonical event frames. No surface owns business logic.

---

## Documentation

`docs/` is the source of truth for scope, architecture, contracts, and tasks.

**Start here:**

| Doc | Description |
|-----|-------------|
| [docs/00_index.md](docs/00_index.md) | Entry point and navigation |
| [docs/01_north_star.md](docs/01_north_star.md) | Vision, non-negotiables, performance budgets |
| [docs/02_architecture/continuity_os.md](docs/02_architecture/continuity_os.md) | Continuity OS (“one chat forever”) |
| [docs/02_architecture/](docs/02_architecture/) | Component map, surfaces, capability matrix |
| [docs/03_contracts/](docs/03_contracts/) | Module contracts and event schemas |
| [docs/06_decisions/](docs/06_decisions/) | Architecture decision records (ADRs) |
| [docs/07_tasks/roadmap.md](docs/07_tasks/roadmap.md) | Now / Next / Later roadmap |

---

## Project Structure

```
crates/
├── rip-cli/                    # `rip` binary (CLI + interactive terminal UI)
├── ripd/                       # Authority/server + runtime
├── rip-kernel/                 # Canonical event frames + runtime hooks/commands
├── rip-log/                    # Event log + snapshots + replay validation
├── rip-workspace/              # Workspace engine + checkpoints
├── rip-tools/                  # Built-in tool implementations
├── rip-openresponses/          # Open Responses boundary helpers
├── rip-provider-openresponses/ # Open Responses provider adapter
├── rip-tui/                    # Terminal UI rendering
└── rip-bench/                  # Benchmark harness (CI budgets)

sdk/
└── typescript/   # TypeScript SDK

docs/             # Source of truth for scope, architecture, contracts
```

---

## License

Apache 2.0
