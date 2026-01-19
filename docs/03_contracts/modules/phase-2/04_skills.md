# Contract: Skills (Agent Skills / “OpenSkills”) (Phase 2)

Summary
- Skills are on-demand capability packages discovered from disk and loaded progressively (frontmatter first, full content on activation).
- We adopt the Agent Skills standard for `SKILL.md` format and validation rules.
- Skills are a **policy-aware orchestration layer**: they do not bypass tool permissions, replay, or surface parity.

Primary references
- Agent Skills notes (external evidence): `temp/docs/agentskills/notes_2026-01-19.md`
- Related ecosystem notes: `temp/docs/pi-agent/docs/skills.md`, `temp/docs/claude-code/docs/en/skills.md`, `temp/docs/opencode/skills.mdx`, `temp/docs/amp/manual/agent-skills.md`

Related capabilities
- `skill.discovery`, `skill.load`, `skill.invoke`, `skill.manage`, `skill.commands`, `skill.repos`, `skill.metadata`, `skill.overrides`, `skill.context`
- `command.custom_dirs`, `command.allowed_tools`, `hooks.matching`, `context.compile`, `policy.permissions.rules`

Goals
- Discover skills quickly and deterministically (no heavy I/O on the hot path).
- Progressive disclosure by default:
  - only `name` + `description` are in baseline context
  - full `SKILL.md` is loaded only when needed
  - referenced files (scripts/assets/references) are loaded on demand
- Keep security and replay intact:
  - skills cannot implicitly widen permissions
  - every skill load/invocation is logged as structured events
  - skill-driven tool activity produces the same frames as any other tool activity
- Surface parity:
  - CLI/server/SDK/TUI/MCP see the same “skill events” and tool events (rendering differs, not meaning).

Non-goals (initial)
- Remote “marketplace install” without explicit policy (Phase 2+).
- Auto-executing scripts on discovery (never).

Definitions
- Skill: a directory containing `SKILL.md` with YAML frontmatter + Markdown body.
- Skill catalog: the list of all discovered skills’ `{name, description, optional metadata}` used for baseline prompting.
- Skill activation: the act of loading full `SKILL.md` and applying its workflow (may include running scripts/tools).

Architecture (Phase 2)

1) Discovery
- Scan configured directories for `**/SKILL.md` (recursive), parse **frontmatter only**.
- Validate required fields and constraints per Agent Skills standard.
- On name collision:
  - deterministically pick a winner (by explicit precedence order), and
  - emit a warning event containing both candidates (so replay is explainable).
- Never execute skill code during discovery.

2) Catalog injection (baseline prompting)
- The context compiler injects a compact catalog:
  - `name`
  - `description`
  - optionally: `compatibility` and `allowed-tools` (policy-gated; see below)
- The full Markdown body stays out of baseline context.

3) Activation and progressive disclosure
- Activation triggers loading the full `SKILL.md` content and (optionally) additional referenced files.
- The runtime must support:
  - reading `references/*` documents on demand
  - invoking `scripts/*` helpers via the tool runtime (sandboxed and logged)
- Any additional file reads or script/tool runs are explicit tool calls, not implicit behavior.

4) Policy + permissions
- `allowed-tools` (frontmatter):
  - treated as a **hint**, not an authority
  - combined with policy to produce an effective allowlist for the skill’s execution window
- Skills may narrow permissions by default (safe), but must not widen permissions unless policy explicitly allows it.
- “Full auto mode” is a policy profile that can allow broader defaults, but still must be explicit and replay-logged.

5) Skills as commands
- Skills can be exposed as commands (Phase 2):
  - `/skill:<name>` loads a skill and appends user args to the skill context
  - command execution is logged and deterministic

6) Skill-defined overrides (planned)
- Skills may propose:
  - tool defaults/aliases
  - model/provider suggestions
  - context scoping rules
- Overrides are always mediated by policy and recorded as frames (surface parity).

Determinism & replay rules
- Discovery results (catalog) must be stable for a given filesystem snapshot:
  - normalize paths and ordering
  - capture precedence decisions in events
- Skill loading must record:
  - resolved skill directory
  - parsed frontmatter (including unknown fields)
  - content hash/digest of `SKILL.md` and loaded referenced files (or an artifact reference)
- If a skill runs scripts/tools, outputs and artifacts follow standard tool frames and artifact storage rules.

Planned event frames (Phase 2)
- `skill_catalog_updated`: `{count, roots, collisions?}`
- `skill_loaded`: `{name, path, digest, frontmatter, warnings?}`
- `skill_invoked`: `{name, args?, mode:manual|auto, effective_allowed_tools?}`
- `skill_warning`: `{name?, kind, detail}`

Tests (required)
- Contract tests:
  - frontmatter parsing and validation (incl unknown field preservation)
  - deterministic collision handling and ordering
  - progressive disclosure invariant (catalog vs full load)
- Replay tests:
  - catalog creation + skill load/invoke produce identical snapshots and equivalent event streams
  - skill execution that runs tools is replayable end-to-end
- Benchmarks:
  - catalog scan on medium repo (bounded I/O and allocations)
  - frontmatter parse throughput

