import assert from "node:assert/strict";
import test from "node:test";

import { Rip } from "../index.js";

function sseResponse(frames: unknown[]): Response {
  const chunks: string[] = [": ping\n\n"];
  for (const frame of frames) {
    chunks.push(`data: ${JSON.stringify(frame)}\n\n`);
  }
  return new Response(chunks.join(""), { status: 200, headers: { "content-type": "text/event-stream" } });
}

test("Rip SDK http transport runs sessions and parses SSE frames", async () => {
  const calls: Array<{ method: string; path: string; body: string | null }> = [];

  const fakeFetch: typeof fetch = async (input, init = {}) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
    const { pathname } = new URL(url);
    const method = (init.method ?? "GET").toUpperCase();
    const body = typeof init.body === "string" ? init.body : null;
    calls.push({ method, path: pathname, body });

    if (method === "POST" && pathname === "/sessions") {
      return new Response(JSON.stringify({ session_id: "s1" }), { status: 201, headers: { "content-type": "application/json" } });
    }
    if (method === "GET" && pathname === "/sessions/s1/events") {
      return sseResponse([
        { type: "session_started", input: "hello" },
        { type: "output_text_delta", delta: "ack: hello" },
        { type: "session_ended", reason: "completed" },
      ]);
    }
    if (method === "POST" && pathname === "/sessions/s1/input") {
      assert.equal(
        body,
        JSON.stringify({
          input: "hello",
          openresponses: {
            include: ["reasoning.encrypted_content"],
          },
        }),
      );
      return new Response("", { status: 202 });
    }

    return new Response("not found", { status: 404 });
  };

  const rip = new Rip({ transport: "http", server: "http://rip.test", fetch: fakeFetch });
  const turn = await rip.run("hello", { include: ["reasoning.encrypted_content"] });
  assert.equal(turn.exitCode, 0);
  assert.equal(turn.finalOutput, "ack: hello");
  assert.ok(turn.frames.some((frame) => frame.type === "session_started"));
  assert.ok(turn.frames.some((frame) => frame.type === "output_text_delta"));
  assert.ok(turn.frames.some((frame) => frame.type === "session_ended"));

  assert.deepEqual(
    calls.map((call) => `${call.method} ${call.path}`),
    ["POST /sessions", "GET /sessions/s1/events", "POST /sessions/s1/input"],
  );
});

test("Rip SDK http transport exposes configDoctor", async () => {
  const fakeFetch: typeof fetch = async (input, init = {}) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
    const { pathname } = new URL(url);
    const method = (init.method ?? "GET").toUpperCase();

    if (method === "GET" && pathname === "/config/doctor") {
      return new Response(
        JSON.stringify({
          sources: [],
          openresponses: {
            endpoint: "https://openrouter.ai/api/v1/responses",
            has_api_key: true,
            headers: [],
            stateless_history: true,
            parallel_tool_calls: false,
            include: ["reasoning.encrypted_content"],
            compat: {
              active_conversation_strategy: "stateless_history",
              conversation: {
                requested: "previous_response_id",
                effective: "stateless_history",
                support: {
                  previous_response_id: "unsupported",
                  stateless_history: "native",
                  recommended: "stateless_history",
                },
                warnings: ["coerced"],
              },
              effective_validation: {
                missing_item_ids: true,
                missing_response_user: true,
                reasoning_text_events: true,
                missing_reasoning_summary: true,
              },
              provider: {
                version: "2026-04-21.v1",
                provider_id: "openrouter",
                label: "OpenRouter Responses API Beta",
                stream_shape: "compat",
                conversation: {
                  previous_response_id: "unsupported",
                  stateless_history: "native",
                  recommended: "stateless_history",
                },
                request: {
                  background: "unknown",
                  store: "unsupported",
                  service_tier: "unknown",
                  response_include: "compat",
                  reasoning_parameter: "native",
                },
                tools: {
                  function_calling: "native",
                  tool_choice: "native",
                  allowed_tools: "unknown",
                  hosted_tools: "compat",
                  mcp_servers: "unknown",
                  mcp_headers: "unknown",
                },
                input_modalities: {
                  input_text: "native",
                  input_image: "unknown",
                  input_file: "unknown",
                  input_video: "unknown",
                },
                validation: {
                  missing_item_ids: true,
                  missing_response_user: true,
                  reasoning_text_events: true,
                  missing_reasoning_summary: true,
                },
              },
              include: {
                requested: ["reasoning.encrypted_content", "message.output_text.logprobs"],
                effective: ["reasoning.encrypted_content"],
                support: {
                  request: "compat",
                  native_values: ["reasoning.encrypted_content"],
                  compat_values: ["file_search_call.results", "code_interpreter_call.outputs"],
                  unknown_values: [],
                  unsupported_values: ["message.output_text.logprobs"],
                },
                warnings: ["message.output_text.logprobs omitted"],
              },
              reasoning: {
                support: {
                  parameter: "native",
                  effort: "native",
                  summary: "unknown",
                  supported_efforts: ["minimal", "low", "medium", "high"],
                  supported_summaries: [],
                },
                warnings: [],
              },
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }

    return new Response("not found", { status: 404 });
  };

  const rip = new Rip({ transport: "http", server: "http://rip.test", fetch: fakeFetch });
  const doctor = await rip.configDoctor();

  assert.equal(doctor.openresponses?.compat?.include.requested.length, 2);
  assert.equal(doctor.openresponses?.compat?.include.effective.length, 1);
  assert.equal(doctor.openresponses?.compat?.include.support.unsupported_values[0], "message.output_text.logprobs");
});

test("Rip SDK http transport supports thread.* and task.* surfaces", async () => {
  const fakeFetch: typeof fetch = async (input, init = {}) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
    const parsed = new URL(url);
    const pathname = parsed.pathname;
    const method = (init.method ?? "GET").toUpperCase();
    const bodyText = typeof init.body === "string" ? init.body : "";

    if (method === "POST" && pathname === "/threads/ensure") {
      return new Response(JSON.stringify({ thread_id: "t1" }), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (method === "GET" && pathname === "/threads") {
      return new Response(
        JSON.stringify([{ thread_id: "t1", created_at_ms: 0, title: null, archived: false }]),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (method === "GET" && pathname === "/threads/t1") {
      return new Response(JSON.stringify({ thread_id: "t1", created_at_ms: 0, title: null, archived: false }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (method === "POST" && pathname === "/threads/t1/messages") {
      const body = JSON.parse(bodyText) as { content: string; actor_id: string; origin: string };
      assert.equal(body.content, "hello");
      assert.equal(body.actor_id, "user");
      assert.equal(body.origin, "sdk-ts");
      return new Response(JSON.stringify({ thread_id: "t1", message_id: "m1", session_id: "s1" }), {
        status: 202,
        headers: { "content-type": "application/json" },
      });
    }
    if (method === "POST" && pathname === "/threads/t1/context-selection-status") {
      return new Response(JSON.stringify({ thread_id: "t1", decisions: [] }), { status: 200, headers: { "content-type": "application/json" } });
    }
    if (method === "GET" && pathname === "/threads/t1/events") {
      return sseResponse([{ type: "continuity_created" }, { type: "continuity_message_appended" }, { type: "continuity_run_spawned" }]);
    }

    if (method === "POST" && pathname === "/tasks") {
      const body = JSON.parse(bodyText) as { tool: string; execution_mode: string };
      assert.equal(body.tool, "bash");
      assert.equal(body.execution_mode, "pipes");
      return new Response(JSON.stringify({ task_id: "task1" }), { status: 201, headers: { "content-type": "application/json" } });
    }
    if (method === "GET" && pathname === "/tasks") {
      return new Response(
        JSON.stringify([
          {
            task_id: "task1",
            status: "queued",
            tool: "bash",
            title: null,
            execution_mode: "pipes",
            exit_code: null,
            started_at_ms: null,
            ended_at_ms: null,
            artifacts: null,
            error: null,
          },
        ]),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (method === "GET" && pathname === "/tasks/task1") {
      return new Response(
        JSON.stringify({
          task_id: "task1",
          status: "queued",
          tool: "bash",
          title: null,
          execution_mode: "pipes",
          exit_code: null,
          started_at_ms: null,
          ended_at_ms: null,
          artifacts: null,
          error: null,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (method === "GET" && pathname === "/tasks/task1/output") {
      assert.equal(parsed.searchParams.get("stream"), "stdout");
      assert.equal(parsed.searchParams.get("offset_bytes"), "0");
      return new Response(
        JSON.stringify({
          task_id: "task1",
          stream: "stdout",
          content: "",
          offset_bytes: 0,
          bytes: 0,
          total_bytes: 0,
          truncated: false,
          artifact_id: "a1",
          path: "logs/stdout",
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (method === "GET" && pathname === "/tasks/task1/events") {
      return sseResponse([{ type: "tool_task_status" }]);
    }

    return new Response("not found", { status: 404 });
  };

  const rip = new Rip({ transport: "http", server: "http://rip.test", fetch: fakeFetch });

  const ensured = await rip.threadEnsure();
  assert.equal(ensured.thread_id, "t1");
  const list = await rip.threadList();
  assert.equal(list.length, 1);
  const meta = await rip.threadGet("t1");
  assert.equal(meta.thread_id, "t1");
  const posted = await rip.threadPostMessage("t1", { content: "hello" });
  assert.equal(posted.message_id, "m1");
  const selection = await rip.threadContextSelectionStatus("t1", { limit: 1 });
  assert.equal(selection.thread_id, "t1");

  const { result: threadStream } = await rip.threadEventsStreamed("t1", {}, { maxEvents: 2 });
  const frames = await threadStream;
  assert.equal(frames.length, 2);

  const created = await rip.taskSpawn({ tool: "bash", args: { cmd: "echo hi" } }, { server: "http://rip.test" });
  assert.equal(created.task_id, "task1");
  const tasks = await rip.taskList({ server: "http://rip.test" });
  assert.equal(tasks.length, 1);
  const status = await rip.taskStatus("task1", { server: "http://rip.test" });
  assert.equal(status.task_id, "task1");
  const out = await rip.taskOutput("task1", { server: "http://rip.test" });
  assert.equal(out.task_id, "task1");
  const { result: taskStream } = await rip.taskEventsStreamed("task1", { server: "http://rip.test" });
  const taskFrames = await taskStream;
  assert.equal(taskFrames.length, 1);
});

test("Rip SDK http transport taskEventsStreamed throws when server is missing", async () => {
  const rip = new Rip({
    transport: "http",
    fetch: async () => {
      throw new Error("fetch should not be called");
    },
  });

  await assert.rejects(
    async () => {
      await rip.taskEventsStreamed("task1", {});
    },
    (err: unknown) => {
      assert.ok(err instanceof Error);
      assert.equal(err.message, "taskEventsStreamed with http transport requires server");
      return true;
    },
  );
});
