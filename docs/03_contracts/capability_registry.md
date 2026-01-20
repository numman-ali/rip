# Capability Registry (Source of Truth)

Summary
- Canonical list of capability ids, phases, and surface support status.
- Server and SDK API schemas must align to this registry.
- Implementation status is tracked in the roadmap and gaps list, not here.

Legend
- phase: P1, P2, P3
- surface keys:
  - cli_i: interactive CLI
  - cli_h: headless CLI
  - server: server API (HTTP/SSE + OpenAPI)
  - sdk: SDK surface
  - tui: TUI surface
  - mcp: MCP surface
- status values:
  - supported: implemented and exposed
  - planned: intended support in the listed phase
  - not_applicable: surface-specific capability by design

Rules
- Every capability must appear once with a stable id and version.
- Surface-specific capabilities must still list explicit surface statuses.
- Breaking changes require a version bump and an ADR.

## Sessions & Threads
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| session.create | v1 | P1 | planned | supported | supported | planned | planned | planned | Start a new session. |
| session.send_input | v1 | P1 | planned | supported | supported | planned | planned | planned | Send input to an active session. |
| session.set_model | v1 | P2 | planned | planned | planned | planned | planned | planned | Change the active session's provider/model selection (applies forward only). |
| session.stream_events | v1 | P1 | planned | supported | supported | planned | planned | planned | Stream session events. |
| session.cancel | v1 | P1 | planned | supported | supported | planned | planned | planned | Cancel an active session. |
| session.resume | v1 | P2 | planned | planned | planned | planned | planned | planned | Resume a prior session by id. |
| session.previous_response | v1 | P2 | planned | planned | planned | planned | planned | planned | Continue from a prior response id (previous_response_id semantics). |
| thread.branch | v1 | P2 | planned | planned | planned | planned | planned | planned | Branch/fork from a prior point. |
| thread.handoff | v1 | P2 | planned | planned | planned | planned | planned | planned | Handoff work to a new thread with curated context. |
| thread.reference | v1 | P2 | planned | planned | planned | planned | planned | planned | Reference another thread by id and extract context. |
| thread.archive | v1 | P2 | planned | planned | planned | planned | planned | planned | Archive threads without deleting history. |
| thread.tags | v1 | P2 | planned | planned | planned | planned | planned | planned | Thread labels/tags. |
| thread.search.text | v1 | P2 | planned | planned | planned | planned | planned | planned | Thread search by keyword. |
| thread.search.files | v1 | P2 | planned | planned | planned | planned | planned | planned | Thread search by files touched. |
| thread.share | v1 | P3 | planned | planned | planned | planned | planned | planned | Thread sharing with visibility levels. |
| thread.map | v1 | P2 | planned | planned | planned | planned | planned | planned | Thread map/graph view data. |
| thread.agents_panel | v1 | P2 | planned | not_applicable | planned | planned | planned | planned | Agents panel data for multi-thread management. |

## Session Storage & Replay
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| event_log.append | v1 | P1 | planned | planned | planned | planned | planned | planned | Append-only event log writes. |
| event_log.replay | v1 | P1 | planned | planned | planned | planned | planned | planned | Deterministic replay from the event log. |
| event_log.tree_links | v1 | P2 | planned | planned | planned | planned | planned | planned | Tree-structured entries with parent/child links. |
| event_log.branch_summaries | v1 | P2 | planned | planned | planned | planned | planned | planned | Branch summaries when switching branches. |
| event_log.compaction_entries | v1 | P2 | planned | planned | planned | planned | planned | planned | Compaction entries with cut points and file tracking. |
| event_log.session_metadata | v1 | P2 | planned | planned | planned | planned | planned | planned | Session metadata entries (name, labels, model changes). |
| event_log.extension_entries | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom entries for extension state. |
| event_log.message_entries | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom message entries affecting context. |

## Context & Guidance
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| context.guidance.load | v1 | P2 | planned | planned | planned | planned | planned | planned | Load project guidance files by path/subtree. |
| context.guidance.glob | v1 | P2 | planned | planned | planned | planned | planned | planned | Apply guidance by glob scope. |
| context.system_overrides | v1 | P2 | planned | planned | planned | planned | planned | planned | System prompt overrides from project/user scope. |
| context.compile | v1 | P2 | planned | planned | planned | planned | planned | planned | Deterministic context compilation + replayable snapshots. |
| context.refs.file | v1 | P2 | planned | planned | planned | planned | planned | planned | File references in prompts. |
| context.refs.thread | v1 | P2 | planned | planned | planned | planned | planned | planned | Thread references in prompts. |
| context.refs.artifact | v1 | P2 | planned | planned | planned | planned | planned | planned | Reference stored artifacts/tool outputs without inlining into context. |

## Configuration & Policy
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| config.scopes | v1 | P2 | planned | planned | planned | planned | planned | planned | Layered settings scopes with precedence. |
| config.merge | v1 | P2 | planned | planned | planned | planned | planned | planned | Non-destructive config merging. |
| config.json_schema | v1 | P2 | planned | planned | planned | planned | planned | planned | JSON/JSONC config + schema validation. |
| config.env_overrides | v1 | P2 | planned | planned | planned | planned | planned | planned | Environment variable overrides. |
| policy.permissions.rules | v1 | P2 | planned | planned | planned | planned | planned | planned | Permission rules with allow/ask/deny/delegate. |
| policy.request_identifiers | v1 | P1 | planned | planned | planned | planned | planned | planned | User/safety identifiers for policy/audit contexts. |
| policy.sandbox.profiles | v1 | P2 | planned | planned | planned | planned | planned | planned | Sandbox profiles and execution policies. |
| config.output_style_defaults | v1 | P2 | planned | planned | planned | planned | planned | planned | Output style and model defaults by scope. |
| config.extensions | v1 | P2 | planned | planned | planned | planned | planned | planned | Plugin/tool/subagent configuration. |
| policy.managed | v1 | P2 | planned | planned | planned | planned | planned | planned | Managed policies for plugins/hooks/marketplaces. |
| policy.enterprise | v1 | P3 | planned | planned | planned | planned | planned | planned | Enterprise scopes with audit logging. |

## Commands & Automation
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| command.registry | v1 | P1 | planned | planned | planned | planned | planned | planned | In-memory command registry. |
| command.slash | v1 | P2 | planned | planned | planned | planned | planned | planned | Slash commands for session control and model selection. |
| command.custom_dirs | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom commands loaded from directories. |
| command.alias | v1 | P2 | planned | planned | planned | planned | planned | planned | Command aliases. |
| command.templates | v1 | P2 | planned | planned | planned | planned | planned | planned | Prompt templates. |
| command.exec | v1 | P2 | planned | planned | planned | planned | planned | planned | Commands can run scripts or insert prompts. |
| command.allowed_tools | v1 | P2 | planned | planned | planned | planned | planned | planned | Allowed-tools lists per command. |
| command.scoped_hooks | v1 | P2 | planned | planned | planned | planned | planned | planned | Scoped hooks during command execution. |

## Execution Modes
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| execution.interactive_cli | v1 | P1 | planned | not_applicable | not_applicable | not_applicable | not_applicable | not_applicable | Interactive CLI mode. |
| execution.interactive_tui | v1 | P2 | not_applicable | not_applicable | not_applicable | not_applicable | planned | not_applicable | Interactive TUI mode. |
| execution.headless | v1 | P1 | not_applicable | supported | not_applicable | not_applicable | not_applicable | not_applicable | Headless execute mode. |
| execution.json_stream | v1 | P1 | planned | supported | supported | planned | planned | planned | Streaming JSON output. |
| execution.stream_options | v1 | P1 | planned | planned | planned | planned | planned | planned | Stream options for usage/extras emission. |
| execution.json_schema | v1 | P1 | planned | planned | planned | planned | planned | planned | JSONL output schema validation. |
| execution.print | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Formatted print mode. |
| execution.rpc | v1 | P2 | not_applicable | not_applicable | planned | planned | not_applicable | not_applicable | RPC mode for machine commands. |
| execution.json_input | v1 | P2 | planned | planned | planned | planned | not_applicable | not_applicable | Streaming JSON input mode. |
| execution.resume_session | v1 | P2 | planned | planned | planned | planned | planned | planned | Resume sessions by id. |
| execution.output_format | v1 | P2 | planned | planned | planned | planned | planned | planned | Output format control for text/JSON/streaming. |
| execution.structured_output | v1 | P2 | planned | planned | planned | planned | planned | planned | Structured output using JSON Schema. |
| execution.response_include | v1 | P1 | planned | planned | planned | planned | planned | planned | Response include/extras selection (logprobs/sources/tool outputs). |

## OpenResponses Boundary
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| openresponses.request_fidelity | v1 | P1 | planned | planned | planned | planned | planned | planned | Preserve full CreateResponseBody payloads at provider boundary. |
| openresponses.response_fidelity | v1 | P1 | planned | planned | planned | planned | planned | planned | Preserve full ResponseResource payloads + items. |
| openresponses.streaming_fidelity | v1 | P1 | planned | planned | planned | planned | planned | planned | SSE semantic events with event/type invariants + [DONE]. |
| openresponses.item_lifecycle | v1 | P1 | planned | planned | planned | planned | planned | planned | Item required fields + lifecycle status semantics. |
| openresponses.content_union | v1 | P1 | planned | planned | planned | planned | planned | planned | User/model content unions (multimodal input + output text/refusal/summary). |
| openresponses.reasoning_items | v1 | P1 | planned | planned | planned | planned | planned | planned | Reasoning item content/encrypted/summary handling. |
| openresponses.tools_union | v1 | P1 | planned | planned | planned | planned | planned | planned | Full OpenResponses tool union + tool call item variants. |
| openresponses.errors | v1 | P1 | planned | planned | planned | planned | planned | planned | Error payloads + streaming failure events. |
| openresponses.extensions | v1 | P2 | planned | planned | planned | planned | planned | planned | Vendor-prefixed items/events + schema extensions. |

## Tools & Tooling
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| tool.builtin_files | v1 | P1 | planned | planned | planned | planned | planned | planned | Built-in file tools + bash tool (shell alias). |
| tool.registry | v1 | P1 | planned | planned | planned | planned | planned | planned | Tool registry with dynamic enable/disable. |
| tool.toolbox | v1 | P1 | planned | planned | planned | planned | planned | planned | Toolboxes discovered from directory. |
| tool.schema | v1 | P1 | planned | planned | planned | planned | planned | planned | Tool schemas for structured I/O. |
| tool.output_limits | v1 | P1 | planned | planned | planned | planned | planned | planned | Tool output truncation + safety limits. |
| tool.output_store | v1 | P2 | planned | planned | planned | planned | planned | planned | Persist full tool outputs in an artifact store; frames carry references + previews. |
| tool.output_fetch | v1 | P2 | planned | planned | planned | planned | planned | planned | Retrieve stored tool outputs/artifacts by id with range support. |
| tool.plan_mode | v1 | P1 | planned | planned | planned | planned | planned | planned | Plan/read-only mode restricting tools. |
| tool.permissions | v1 | P2 | planned | planned | planned | planned | planned | planned | Tool permission policy engine. |
| tool.override | v1 | P2 | planned | planned | planned | planned | planned | planned | Override built-in tools with custom impls. |
| tool.remote | v1 | P2 | planned | planned | planned | planned | planned | planned | Remote tool execution backend. |
| tool.arg_rules | v1 | P2 | planned | planned | planned | planned | planned | planned | Per-tool argument rules. |
| tool.task_spawn | v1 | P2 | planned | planned | planned | planned | planned | planned | Run a tool invocation in the background (returns task id; streams events). |
| tool.task_status | v1 | P2 | planned | planned | planned | planned | planned | planned | Query background tool task status/metadata independent of the live stream. |
| tool.task_cancel | v1 | P2 | planned | planned | planned | planned | planned | planned | Cancel/kill a running tool task (best-effort, logged). |
| tool.task_stream_events | v1 | P2 | planned | planned | planned | planned | planned | planned | Subscribe to a background tool task event stream (SSE/JSONL) by `task_id`. |
| tool.task_write_stdin | v1 | P2 | planned | planned | planned | planned | planned | planned | Send stdin bytes/keys to an interactive tool task (PTY mode only). |
| tool.task_resize | v1 | P2 | planned | planned | planned | planned | planned | planned | Resize an interactive tool task terminal (rows/cols; PTY mode only). |
| tool.task_signal | v1 | P2 | planned | planned | planned | planned | planned | planned | Send a signal to a running tool task (SIGINT/SIGTERM/etc; platform-mapped). |
| tool.choice | v1 | P1 | planned | planned | planned | planned | planned | planned | Tool choice policy (auto/required/none/force). |
| tool.call_limits | v1 | P1 | planned | planned | planned | planned | planned | planned | Max tool calls + parallel tool call limits. |
| tool.allowed_tools | v1 | P1 | planned | planned | planned | planned | planned | planned | Allowed-tools lists for noninteractive runs. |
| tool.read_ranges | v1 | P1 | planned | planned | planned | planned | planned | planned | Read tools support line ranges. |
| tool.lsp | v1 | P2 | planned | planned | planned | planned | planned | planned | LSP tool integration. |

## Compaction & Summarization
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| compaction.auto | v1 | P2 | planned | planned | planned | planned | planned | planned | Auto-compaction when context exceeds threshold. |
| compaction.manual | v1 | P2 | planned | planned | planned | planned | planned | planned | Manual compaction with instructions. |
| compaction.split_turn | v1 | P2 | planned | planned | planned | planned | planned | planned | Split-turn handling for oversized turns. |
| compaction.truncation_policy | v1 | P2 | planned | planned | planned | planned | planned | planned | Truncation policy controls (auto vs disabled). |
| compaction.cut_points | v1 | P2 | planned | planned | planned | planned | planned | planned | Compaction cut-point rules. |
| compaction.branch_summary | v1 | P2 | planned | planned | planned | planned | planned | planned | Branch summarization. |
| compaction.file_ops | v1 | P2 | planned | planned | planned | planned | planned | planned | File-operation tracking in summaries. |
| compaction.hooks | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook-based custom compaction. |

## Policy & Steering
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| policy.budgets | v1 | P3 | planned | planned | planned | planned | planned | planned | Adaptive budget controls. |
| policy.rule_engine | v1 | P3 | planned | planned | planned | planned | planned | planned | Runtime rule engine for guardrails. |

## Extensions & Hooks
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| hooks.lifecycle | v1 | P1 | planned | planned | planned | planned | planned | planned | Session lifecycle hooks. |
| hooks.permission | v1 | P2 | planned | planned | planned | planned | planned | planned | Permission hooks. |
| hooks.notifications | v1 | P2 | planned | planned | planned | planned | planned | planned | Notification hooks. |
| hooks.pre_compaction | v1 | P2 | planned | planned | planned | planned | planned | planned | Pre-compaction hooks. |
| hooks.matching | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook matching rules. |
| hooks.outputs | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook outputs allow/deny/modify. |
| hooks.handlers | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook handlers (command/prompt). |
| hooks.events | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook event taxonomy. |
| hooks.types | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook types (command/prompt/agentic). |
| hooks.timeouts | v1 | P2 | planned | planned | planned | planned | planned | planned | Hook timeouts and once-only hooks. |
| extension.api | v1 | P2 | planned | planned | planned | planned | planned | planned | Extension API for messages and commands. |
| extension.state | v1 | P2 | planned | planned | planned | planned | planned | planned | Persistent extension state in session entries. |
| extension.ui | v1 | P2 | planned | not_applicable | planned | planned | planned | not_applicable | UI extension points. |
| extension.event_bus | v1 | P2 | planned | planned | planned | planned | planned | planned | Event bus for extensions. |
| extension.commands | v1 | P2 | planned | planned | planned | planned | planned | planned | Register commands/shortcuts/flags. |
| extension.tool_renderers | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom tool renderers. |
| extension.errors | v1 | P2 | planned | planned | planned | planned | planned | planned | Extension error reporting/recovery. |
| extension.session_control | v1 | P2 | planned | planned | planned | planned | planned | planned | Extension access to session control. |
| extension.agent_state | v1 | P2 | planned | planned | planned | planned | planned | planned | Extension access to agent state. |

## Skills
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| skill.discovery | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill discovery by SKILL.md. |
| skill.load | v1 | P2 | planned | planned | planned | planned | planned | planned | On-demand skill loading. |
| skill.invoke | v1 | P2 | planned | planned | planned | planned | planned | planned | User-invokable skill activation. |
| skill.manage | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill install/list/remove workflows. |
| skill.commands | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill command syntax. |
| skill.repos | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill repositories with scripts/assets. |
| skill.metadata | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill metadata for compatibility/allowed tools. |
| skill.overrides | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill-defined model/tool overrides. |
| skill.context | v1 | P2 | planned | planned | planned | planned | planned | planned | Skill-defined forked context or subagent invocation. |

## Subagents
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| subagent.spawn | v1 | P2 | planned | planned | planned | planned | planned | planned | Spawn subagents with independent context/tools. |
| subagent.profiles | v1 | P2 | planned | planned | planned | planned | planned | planned | Specialized subagent profiles. |
| subagent.execution_modes | v1 | P2 | planned | planned | planned | planned | planned | planned | Background/foreground subagent execution. |
| subagent.auto_delegate | v1 | P2 | planned | planned | planned | planned | planned | planned | Auto-delegation rules. |
| subagent.load | v1 | P2 | planned | planned | planned | planned | planned | planned | Subagent definitions loaded from config/markdown. |
| subagent.runtime | v1 | P2 | planned | planned | planned | planned | planned | planned | Runtime-defined subagents. |
| subagent.invoke | v1 | P2 | planned | planned | planned | planned | planned | planned | Manual subagent invocation. |

## Models & Providers
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| model.select | v1 | P1 | planned | planned | planned | planned | planned | planned | Select the upstream provider model id/string. |
| model.sampling_params | v1 | P1 | planned | planned | planned | planned | planned | planned | Temperature/top_p/penalties/seed sampling controls. |
| model.max_output_tokens | v1 | P1 | planned | planned | planned | planned | planned | planned | Max output tokens per response. |
| model.logprobs | v1 | P1 | planned | planned | planned | planned | planned | planned | Logprob emission for output tokens. |
| model.service_tier | v1 | P2 | planned | planned | planned | planned | planned | planned | Service tier hint for routing/priority. |
| model.prompt_cache | v1 | P2 | planned | planned | planned | planned | planned | planned | Prompt cache key + retention controls. |
| model.multi_provider | v1 | P2 | planned | planned | planned | planned | planned | planned | Multi-provider support. |
| model.catalog | v1 | P2 | planned | planned | planned | planned | planned | planned | Model catalog load + optional refresh hooks (versioned). |
| model.routing | v1 | P2 | planned | planned | planned | planned | planned | planned | Routing policy engine (advisory/authoritative) with recorded decisions. |
| model.thinking_levels | v1 | P2 | planned | planned | planned | planned | planned | planned | Reasoning levels with budgets. |
| model.registry | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom model registry and auth. |
| model.cycle | v1 | P2 | planned | planned | planned | planned | planned | planned | Runtime model cycling and availability queries. |
| model.profiles | v1 | P2 | planned | planned | planned | planned | planned | planned | Agent mode profiles (fast vs deep). |
| model.second_opinion | v1 | P2 | planned | planned | planned | planned | planned | planned | Second-opinion model/tool. |
| model.overrides | v1 | P2 | planned | planned | planned | planned | planned | planned | Per-run overrides for model/sandbox/approval policy. |

## Usage & Telemetry
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| usage.token_counts | v1 | P1 | planned | planned | planned | planned | planned | planned | Usage + token counts in responses. |

## Output Styles
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| output.style | v1 | P2 | planned | planned | planned | planned | planned | planned | Output style registry (system prompt modifiers). |
| output.style.load | v1 | P2 | planned | planned | planned | planned | planned | planned | Custom output styles loaded from files. |

## UI / Interaction
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ui.palette | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Command palette and shortcuts. |
| ui.editor | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | External editor for composing prompts. |
| ui.multiline | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Multi-line input and message queueing. |
| ui.autocomplete | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | File path autocomplete for @file. |
| ui.export | v1 | P2 | planned | not_applicable | planned | planned | planned | not_applicable | Session export (HTML/JSON) and stats. |
| ui.image | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Image input and generation/editing UI. |
| ui.undo | v1 | P2 | planned | not_applicable | planned | planned | planned | not_applicable | Undo/revert last agent action. |
| ui.thread_tree | v1 | P2 | planned | not_applicable | planned | planned | planned | not_applicable | Session tree UI with branch navigation. |
| ui.message_queue | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Message queue modes during streaming. |
| ui.review | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Agentic review panel for changes. |
| ui.media_analysis | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | PDF/image analysis UI. |
| ui.thread_map | v1 | P2 | planned | not_applicable | planned | planned | planned | not_applicable | Thread map visualization UI. |
| ui.edit_message | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Edit prior message to revert downstream changes. |
| ui.command_palette | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Custom commands via palette. |
| ui.shortcuts | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Keyboard shortcuts and integrations. |
| ui.keybindings | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Customizable keybindings (including key-event debugging). |
| ui.theme | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Theme selection and runtime switching. |
| ui.raw_events | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Raw frame/event viewing mode (rendered ↔ raw). |
| ui.clipboard | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Clipboard read/write for copy/paste affordances. |
| ui.background_tasks | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Background task status updates. |
| ui.shell_input | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Direct shell input mode. |
| ui.permission_modes | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Permission mode toggles. |
| ui.vim_mode | v1 | P2 | planned | not_applicable | not_applicable | not_applicable | planned | not_applicable | Vim-style editor mode. |

## Integrations
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| integrations.mcp_client | v1 | P2 | planned | planned | planned | planned | planned | planned | MCP client support (local/remote). |
| integrations.mcp_server | v1 | P2 | not_applicable | not_applicable | not_applicable | not_applicable | not_applicable | planned | Agent runs as MCP server. |
| integrations.cli_tools | v1 | P2 | planned | planned | planned | planned | planned | planned | CLI tool integrations. |
| integrations.ide | v1 | P2 | planned | planned | planned | planned | planned | planned | IDE/editor adapters via RPC. |
| integrations.mcp_lazy | v1 | P2 | planned | planned | planned | planned | planned | planned | On-demand MCP tool loading. |
| integrations.ci | v1 | P2 | planned | planned | planned | planned | planned | planned | CI automation via wrapper/action. |
| integrations.sdk | v1 | P2 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Programmatic SDK for sessions/events. |
| integrations.tui_attach | v1 | P2 | not_applicable | not_applicable | planned | planned | planned | not_applicable | TUI attaches to server instance. |

## Background Workers
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| workers.sync | v1 | P3 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Sync workers. |
| workers.analytics | v1 | P3 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Analytics workers. |
| workers.cache_warmers | v1 | P3 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Cache warmers. |
| workers.index_builders | v1 | P3 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Index builders. |
| workers.maintenance | v1 | P3 | not_applicable | not_applicable | planned | planned | not_applicable | planned | Scheduled maintenance tasks. |

## Checkpointing & Rewind
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| checkpoint.auto | v1 | P1 | planned | planned | supported | planned | planned | planned | Automatic checkpoints for file-edit tools. |
| checkpoint.rewind | v1 | P1 | planned | planned | supported | planned | planned | planned | Rewind conversation/workspace to checkpoint. |
| checkpoint.persist | v1 | P1 | planned | planned | supported | planned | planned | planned | Checkpoints persist across sessions. |

## Security & Safety
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| security.sandbox | v1 | P1 | planned | planned | planned | planned | planned | planned | Sandboxing profiles for untrusted execution. |
| security.redaction | v1 | P1 | planned | planned | planned | planned | planned | planned | Secret redaction in logs/outputs. |
| security.permissions | v1 | P2 | planned | planned | planned | planned | planned | planned | Permission policy engine. |
| security.loop_detection | v1 | P3 | planned | planned | planned | planned | planned | planned | Loop detection for repeated tool calls. |
| security.isolation | v1 | P3 | planned | planned | planned | planned | planned | planned | Per-tool isolation and resource limits. |

## Search, Index, Memory
| id | v | phase | cli_i | cli_h | server | sdk | tui | mcp | intent |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| search.index | v1 | P3 | planned | planned | planned | planned | planned | planned | Local index and retrieval. |
| search.retrieval | v1 | P3 | planned | planned | planned | planned | planned | planned | Retrieval with provenance. |
| memory.store | v1 | P3 | planned | planned | planned | planned | planned | planned | Persistent memory store. |
| memory.provenance | v1 | P3 | planned | planned | planned | planned | planned | planned | Memory provenance and attribution. |
