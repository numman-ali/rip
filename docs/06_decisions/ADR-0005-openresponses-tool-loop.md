# ADR-0005: OpenResponses provider-driven tool loop uses previous_response_id and function_call promotion

Status
- Accepted

Context
- Phase 1 needs a real agent loop: provider tool calls -> local tools -> follow-up requests.
- OpenResponses is the provider boundary; we must preserve full SSE fidelity (`provider_event` frames) while still executing tools.
- We need deterministic replay and stable event ordering across surfaces.
- OpenResponses supports multiple tool-call item types; not all map 1:1 to RIP’s built-in tools.
- Spec refs: `temp/openresponses/src/pages/specification.mdx` (`previous_response_id`, `tool_choice`, item streaming; `response.output_item.*`, `response.function_call_arguments.*`).

Decision
- Preserve all OpenResponses SSE events as `provider_event` frames (no drops).
- Promote only **executable** tool calls to first-class internal tool execution:
  - Phase 1 promotion target: `function_call` output items.
  - Tool execution is triggered when the corresponding output item reaches `response.output_item.done`.
- Assemble tool call arguments by observing:
  - `response.output_item.added` / `response.output_item.done` (for `function_call` items), and
  - `response.function_call_arguments.delta` / `.done` (argument stream, keyed by `item_id`).
- Execute tools **sequentially in output order** (by `output_index`) for determinism.
- Follow-ups use OpenResponses continuity via `previous_response_id`:
  - After completing tool execution for a response, send a new CreateResponse request with:
    - `previous_response_id` set to the last response id observed, and
    - `input` set to tool output items (Phase 1: `function_call_output` with `call_id`).
- Tool availability for the provider is declared explicitly via `tools` in each request (Phase 1: function tools for built-in RIP tools + aliases).
- Tool call limits (Phase 1):
  - Requests set `parallel_tool_calls: false`.
  - Requests set `max_tool_calls` and ripd enforces a global cap to avoid infinite loops.
- Tool output encoding (Phase 1):
  - `function_call_output.output` is a JSON string of a stable output object (stdout/stderr/exit_code/artifacts).
  - Exclude nondeterministic runtime fields (e.g., tool ids, durations) from the model-facing output.

Consequences
- We can support OpenResponses tool-calling without losing provider fidelity; derived internal frames are additive.
- The loop is replayable: provider stream + tool frames + follow-up requests can be captured deterministically.
- Other OpenResponses tool-call item types remain provider-only until explicitly promoted/mapped.
- Parallel tool calls are not enabled in Phase 1; later work can introduce parallel execution with explicit determinism rules and budgets.
