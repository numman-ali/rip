<div align="center">

<img src="./assets/logo.svg" alt="rip" width="420" />

<br/>
<br/>

**A Continuity OS for coding agents. Built in Rust.**

One continuity that never ends. Local-first by default. Remote when you want it.

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

[Quickstart](#quickstart) · [Mental Model](#continuity-os-the-mental-model) · [What Exists Today](#what-exists-today) · [Docs](#documentation)

</div>

---

## RIP, in one paragraph

RIP is a high-performance continuity runtime and control plane for autonomous coding agents. Instead of treating each interaction as a disposable chat session, RIP treats work as a long-lived **continuity** backed by an append-only event log. Runs are just compute jobs attached to that continuity. The result is a system that can stay coherent across terminals, surfaces, and devices while keeping replayability, determinism, and low-latency execution as first-class constraints.

## Why This Repo Exists

Most agent products still inherit the wrong shape from chat apps:

- session history is the product
- provider state becomes the source of truth
- local and remote modes behave differently
- UI surfaces quietly accumulate business logic
- debugging means guessing what happened instead of replaying it

RIP is trying to be the opposite:

- **Continuity-first**: one chat forever; runs are temporary, continuities are durable
- **Fast on the hot path**: streaming, parsing, tool dispatch, repo work, and patch loops are all performance-critical
- **Deterministic by default**: the event log and artifacts are the truth; replay is a product feature, not a debugging afterthought
- **Surface-parity driven**: CLI, TUI, server, and SDK are adapters over the same runtime and capability contract
- **Open Responses at the boundary only**: providers speak Open Responses; internally RIP uses its own compact frames and continuity model

## What Exists Today

RIP is no longer just a sketch of the architecture. The current repo already has:

- a fullscreen **TUI** (`rip`) built around a continuity canvas rather than disposable chat panes
- a local-first **headless CLI** (`rip run`) for automation and scripting
- a **control plane** (`rip serve` / `ripd`) that exposes agent sessions over HTTP/SSE and serves OpenAPI
- **continuity commands** (`rip threads ...`) for continuity-first operations like ensure, list, get, post, branch, and handoff
- **background task control** (`rip tasks ...`) including PTY-backed task execution and event streaming
- a **TypeScript SDK** that defaults to local exec and can also attach to a remote server
- a **provider compatibility layer** for Open Responses so downstream quirks are handled declaratively at the provider boundary instead of leaking into UI or session code
- **config diagnostics** (`rip config doctor`) that explain the resolved provider/model route and effective compatibility posture

This is the current product shape:

| Surface | Role |
| --- | --- |
| `rip` | Fullscreen continuity UI for local-first interactive use |
| `rip run` | Headless automation and JSON/event streaming |
| `rip threads` | Continuity management and continuity-level event access |
| `rip tasks` | Background tool/process control, including PTY flows |
| `rip serve` / `ripd` | Remote control plane over HTTP/SSE |
| `sdk/typescript` | Programmatic adapter over the same runtime surfaces |

## Continuity OS (the mental model)

If you remember one thing, it should be this:

**The continuity event log is the source of truth. Provider conversation state is a cache.**

That one rule changes the whole system design.

- A **continuity** is the durable, user-facing identity of the work.
- A **run** is one execution job attached to that continuity.
- A **task** is background work with its own event stream.
- A **provider cursor** such as `previous_response_id` is an optimization, not durable truth.

This lets RIP support "one chat forever" without pretending provider-side threads are canonical. It also means provider cursors can be rotated, rebuilt, or discarded without losing continuity truth.

## Architecture (high level)

Every active surface talks to the same runtime model:

```text
[ rip ]           fullscreen TUI  ----\
[ rip run ]       headless CLI     ----+--> [ authority / control plane ] --> [ Open Responses providers ]
[ rip threads ]   continuity ops   ----/                |
[ rip tasks ]     task control     ----\                +--> tool runtime + registry
[ TS SDK ]        programmatic     ----+                +--> workspace engine
                                                   ---> +--> continuity store + artifacts
                                                        +--> event log + replay + snapshots
```

Important architectural constraints:

- all inter-module traffic is structured events, not raw text
- no surface owns business logic
- local use is **local-first** and auto-attaches to a per-store authority
- remote use should preserve the same semantics, not invent a second product
- Open Responses is used at the provider boundary, not as the system's internal truth model

## Open Responses, but only at the edge

RIP is committed to full Open Responses coverage at the provider boundary, but the server itself is **not** an Open Responses API. The server exposes the coding agent and its continuity/control-plane capabilities.

That distinction matters:

- RIP preserves Open Responses fidelity at ingress and egress
- RIP maps that fidelity into internal canonical frames instead of making provider payloads the product model
- RIP tracks provider/model differences through versioned compatibility profiles
- RIP keeps those quirks out of the TUI, SDK, and core continuity logic whenever possible

If you care about Open Responses coverage specifically, start with:

- [docs/03_contracts/openresponses_coverage.md](docs/03_contracts/openresponses_coverage.md)
- [docs/03_contracts/openresponses_capability_map.md](docs/03_contracts/openresponses_capability_map.md)
- [docs/03_contracts/openresponses_provider_profiles.md](docs/03_contracts/openresponses_provider_profiles.md)

## Quickstart

### Install from source

```bash
cargo install --path crates/rip-cli
```

### Run the fullscreen TUI

```bash
rip
```

RIP auto-starts or auto-attaches to a local authority for the current store, so local multi-terminal use works without manually starting a server first.

### Run headless

```bash
rip run "List the repo root files using the ls tool, then answer with just the filenames." --view output
```

Use raw event frames for automation:

```bash
rip run "..." --view raw
```

### Inspect the resolved provider/model route

```bash
rip config doctor
```

### Connect a provider

OpenAI:

```bash
OPENAI_API_KEY=... rip run "..." --provider openai --model gpt-5-nano-2025-08-07
```

OpenRouter:

```bash
OPENROUTER_API_KEY=... rip run "..." --provider openrouter --model openai/gpt-oss-20b --stateless-history
```

### Work with continuities directly

```bash
rip threads ensure
rip threads list
```

### Start a remote control plane

```bash
RIP_SERVER_ADDR=0.0.0.0:7341 rip serve
```

Then connect from another machine or terminal:

```bash
rip --server http://<host>:7341
rip run --server http://<host>:7341 "..."
```

## Current Posture

This repo is opinionated about what it is building:

- **fastest coding-agent harness possible**
- **modular, pluggable, testable by default**
- **designed for autonomous agent development, not human collaboration**
- **continuity OS, not chat app**
- **local-first, but remote-capable**
- **control-plane and SDK are first-class surfaces, not afterthoughts**

Near-term work continues in the same direction: richer continuity tooling, stronger provider/model compatibility coverage, more operator-grade TUI behavior, and deeper context compilation / memory / retrieval work without compromising replayability.

## Documentation

`docs/` is the source of truth for architecture, contracts, decisions, and roadmap.

Recommended starting points:

| Doc | Why read it |
| --- | --- |
| [docs/00_index.md](docs/00_index.md) | Entry point into the docs set |
| [docs/01_north_star.md](docs/01_north_star.md) | Product intent, non-negotiables, performance posture |
| [docs/02_architecture/continuity_os.md](docs/02_architecture/continuity_os.md) | The "one chat forever" operating model |
| [docs/02_architecture/surfaces.md](docs/02_architecture/surfaces.md) | Surface roles and parity rules |
| [docs/03_contracts/capability_registry.md](docs/03_contracts/capability_registry.md) | Canonical capability ids and support status |
| [docs/04_execution/cli.md](docs/04_execution/cli.md) | Current CLI and local/remote execution model |
| [docs/04_execution/sdk.md](docs/04_execution/sdk.md) | TypeScript SDK execution model |
| [docs/07_tasks/roadmap.md](docs/07_tasks/roadmap.md) | Now / Next / Later roadmap |

## Project Structure

```text
crates/
  rip-cli/                     `rip` binary, fullscreen TUI, headless CLI
  ripd/                        authority, runtime, server, OpenResponses boundary
  rip-kernel/                  canonical frames and runtime hooks
  rip-log/                     event log, snapshots, replay
  rip-workspace/               workspace engine and checkpoints
  rip-tools/                   built-in tools
  rip-openresponses/           Open Responses boundary helpers
  rip-provider-openresponses/  Open Responses provider adapter
  rip-tui/                     frame-driven terminal rendering
  rip-bench/                   benchmark harness

sdk/
  typescript/                  TypeScript SDK

docs/                          source of truth for architecture and contracts
```

## License

Apache 2.0
