import { spawn } from "node:child_process";
import readline from "node:readline";

import type { RipEventFrame } from "./frames.js";
import type { RipFetch, RipHttpConfig } from "./http.js";
import { httpJson, httpRequest, sseDataMessages } from "./http.js";
import {
  buildRipRunArgs,
  buildRipThreadBranchArgs,
  buildRipThreadCompactionCheckpointArgs,
  buildRipThreadCompactionAutoArgs,
  buildRipThreadCompactionAutoScheduleArgs,
  buildRipThreadCompactionCutPointsArgs,
  buildRipThreadCompactionStatusArgs,
  buildRipThreadEnsureArgs,
  buildRipThreadEventsArgs,
  buildRipThreadGetArgs,
  buildRipThreadHandoffArgs,
  buildRipThreadListArgs,
  buildRipThreadPostMessageArgs,
  buildRipThreadContextSelectionStatusArgs,
  buildRipThreadProviderCursorRotateArgs,
  buildRipThreadProviderCursorStatusArgs,
} from "./util.js";

export type RipTransport = "exec" | "http";

export type RipOptions = {
  executablePath?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  transport?: RipTransport;
  server?: string;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export type RipRunOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
  extraArgs?: string[];
  transport?: RipTransport;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export type RipTaskOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
  transport?: RipTransport;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export type RipThreadOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
  transport?: RipTransport;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export type RipTurn = {
  frames: RipEventFrame[];
  finalOutput: string;
  exitCode: number;
};

export type RipThreadEnsureResponse = {
  thread_id: string;
};

export type RipThreadMeta = {
  thread_id: string;
  created_at_ms: number;
  title: string | null;
  archived: boolean;
};

export type RipThreadPostMessageRequest = {
  content: string;
  actor_id?: string;
  origin?: string;
};

export type RipThreadPostMessageResponse = {
  thread_id: string;
  message_id: string;
  session_id: string;
};

export type RipThreadBranchRequest = {
  title?: string;
  from_message_id?: string;
  from_seq?: number;
  actor_id?: string;
  origin?: string;
};

export type RipThreadBranchResponse = {
  thread_id: string;
  parent_thread_id: string;
  parent_seq: number;
  parent_message_id?: string;
};

export type RipThreadHandoffRequest = {
  title?: string;
  summary_markdown?: string;
  summary_artifact_id?: string;
  from_message_id?: string;
  from_seq?: number;
  actor_id?: string;
  origin?: string;
};

export type RipThreadHandoffResponse = {
  thread_id: string;
  from_thread_id: string;
  from_seq: number;
  from_message_id?: string;
};

export type RipThreadCompactionCheckpointRequest = {
  summary_markdown?: string;
  summary_artifact_id?: string;
  to_message_id?: string;
  to_seq?: number;
  stride_messages?: number;
  actor_id?: string;
  origin?: string;
};

export type RipThreadCompactionCheckpointResponse = {
  thread_id: string;
  checkpoint_id: string;
  cut_rule_id: string;
  summary_artifact_id: string;
  to_seq: number;
  to_message_id: string;
};

export type RipThreadCompactionCutPointsRequest = {
  stride_messages?: number;
  limit?: number;
};

export type RipThreadCompactionCutPoint = {
  target_message_ordinal: number;
  to_seq: number;
  to_message_id: string;
  already_checkpointed: boolean;
  latest_checkpoint_id: string | null;
};

export type RipThreadCompactionCutPointsResponse = {
  thread_id: string;
  stride_messages: number;
  message_count: number;
  cut_rule_id: string;
  cut_points: RipThreadCompactionCutPoint[];
};

export type RipThreadCompactionStatusRequest = {
  stride_messages?: number;
};

export type RipThreadCompactionStatusCheckpoint = {
  checkpoint_id: string;
  cut_rule_id: string;
  summary_kind: string;
  summary_artifact_id: string;
  to_seq: number;
  to_message_id: string | null;
};

export type RipThreadCompactionStatusScheduleDecision = {
  decision_id: string;
  policy_id: string;
  decision: string;
  execute: boolean;
  stride_messages: number;
  max_new_checkpoints: number;
  block_on_inflight: boolean;
  message_count: number;
  cut_rule_id: string;
  planned: Array<{ target_message_ordinal: number; to_seq: number; to_message_id: string }>;
  job_id: string | null;
  job_kind: string | null;
  actor_id: string;
  origin: string;
  seq: number;
  timestamp_ms: number;
};

export type RipThreadCompactionStatusJobOutcome = {
  job_id: string;
  job_kind: string;
  status: string;
  error: string | null;
  created: RipThreadCompactionAutoResultCheckpoint[];
  actor_id: string;
  origin: string;
  seq: number;
  timestamp_ms: number;
};

export type RipThreadCompactionStatusResponse = {
  thread_id: string;
  stride_messages: number;
  message_count: number;
  latest_checkpoint: RipThreadCompactionStatusCheckpoint | null;
  next_cut_point: { target_message_ordinal: number; to_seq: number; to_message_id: string } | null;
  inflight_job_id: string | null;
  last_schedule_decision: RipThreadCompactionStatusScheduleDecision | null;
  last_job_outcome: RipThreadCompactionStatusJobOutcome | null;
};

export type RipThreadProviderCursorStatusCursor = {
  cursor_event_id: string;
  provider: string;
  endpoint: string | null;
  model: string | null;
  cursor: unknown | null;
  action: string;
  reason: string | null;
  run_session_id: string | null;
  actor_id: string;
  origin: string;
  seq: number;
  timestamp_ms: number;
};

export type RipThreadProviderCursorStatusResponse = {
  thread_id: string;
  active: RipThreadProviderCursorStatusCursor | null;
  cursors: RipThreadProviderCursorStatusCursor[];
};

export type RipThreadProviderCursorRotateRequest = {
  reason?: string;
  actor_id?: string;
  origin?: string;
};

export type RipThreadProviderCursorRotateResponse = {
  thread_id: string;
  rotated: boolean;
  provider: string | null;
  endpoint: string | null;
  model: string | null;
  cursor_event_id: string | null;
};

export type RipThreadContextSelectionStatusRequest = {
  limit?: number;
};

export type RipThreadContextSelectionStatusCheckpoint = {
  checkpoint_id: string;
  summary_kind: string;
  summary_artifact_id: string;
  to_seq: number;
};

export type RipThreadContextSelectionStatusReset = {
  input: string;
  action: string;
  reason: string;
  ref?: unknown | null;
};

export type RipThreadContextSelectionStatusDecision = {
  decision_event_id: string;
  run_session_id: string;
  message_id: string;
  compiler_id: string;
  compiler_strategy: string;
  limits: unknown;
  compaction_checkpoint: RipThreadContextSelectionStatusCheckpoint | null;
  resets: RipThreadContextSelectionStatusReset[];
  reason: unknown | null;
  actor_id: string;
  origin: string;
  seq: number;
  timestamp_ms: number;
};

export type RipThreadContextSelectionStatusResponse = {
  thread_id: string;
  decisions: RipThreadContextSelectionStatusDecision[];
};

export type RipThreadCompactionAutoRequest = {
  stride_messages?: number;
  max_new_checkpoints?: number;
  dry_run?: boolean;
  actor_id?: string;
  origin?: string;
};

export type RipThreadCompactionAutoResultCheckpoint = {
  checkpoint_id: string;
  summary_artifact_id: string;
  to_seq: number;
  to_message_id: string;
  cut_rule_id: string;
};

export type RipThreadCompactionAutoResponse = {
  thread_id: string;
  job_id: string | null;
  job_kind: string | null;
  status: string;
  stride_messages: number;
  message_count: number;
  cut_rule_id: string;
  planned: Array<{ target_message_ordinal: number; to_seq: number; to_message_id: string }>;
  result: RipThreadCompactionAutoResultCheckpoint[];
  error: string | null;
};

export type RipThreadCompactionAutoScheduleRequest = {
  stride_messages?: number;
  max_new_checkpoints?: number;
  allow_inflight?: boolean;
  no_execute?: boolean;
  dry_run?: boolean;
  actor_id?: string;
  origin?: string;
};

export type RipThreadCompactionAutoScheduleResponse = {
  thread_id: string;
  decision_id: string | null;
  policy_id: string;
  decision: string;
  execute: boolean;
  stride_messages: number;
  max_new_checkpoints: number;
  block_on_inflight: boolean;
  message_count: number;
  cut_rule_id: string;
  planned: Array<{ target_message_ordinal: number; to_seq: number; to_message_id: string }>;
  job_id: string | null;
  job_kind: string | null;
  result: RipThreadCompactionAutoResultCheckpoint[];
  error: string | null;
};

export type RipTaskSpawnRequest = {
  tool: string;
  args: unknown;
  title?: string;
  execution_mode?: "pipes" | "pty";
};

export type RipTaskCreated = {
  task_id: string;
};

export type RipTaskStatus = {
  task_id: string;
  status: "queued" | "running" | "exited" | "cancelled" | "failed";
  tool: string;
  title: string | null;
  execution_mode: "pipes" | "pty";
  exit_code: number | null;
  started_at_ms: number | null;
  ended_at_ms: number | null;
  artifacts: unknown;
  error: string | null;
};

export type RipTaskOutput = {
  task_id: string;
  stream: "stdout" | "stderr" | "pty";
  content: string;
  offset_bytes: number;
  bytes: number;
  total_bytes: number;
  truncated: boolean;
  artifact_id: string;
  path: string;
};

export class RipExecError extends Error {
  readonly exitCode: number | null;
  readonly signal: NodeJS.Signals | null;
  readonly stderr: string;

  constructor(message: string, detail: { exitCode: number | null; signal: NodeJS.Signals | null; stderr: string }) {
    super(message);
    this.name = "RipExecError";
    this.exitCode = detail.exitCode;
    this.signal = detail.signal;
    this.stderr = detail.stderr;
  }
}

export class Rip {
  private base: RipOptions;

  constructor(options: RipOptions = {}) {
    this.base = options;
  }

  async run(prompt: string, options: RipRunOptions = {}): Promise<RipTurn> {
    const { result } = await this.runStreamed(prompt, options);
    return await result;
  }

  async runStreamed(
    prompt: string,
    options: RipRunOptions = {},
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipTurn> }> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      return await this.runStreamedHttp(prompt, options);
    }

    const executablePath = options.executablePath ?? this.base.executablePath ?? "rip";
    const cwd = options.cwd ?? this.base.cwd;
    const env = mergeEnv(process.env, this.base.env, options.env);
    unsetEnvVars(env, this.base.unsetEnv);
    unsetEnvVars(env, options.unsetEnv);
    const args = buildRipRunArgs(prompt, { server: options.server, extraArgs: options.extraArgs });

    const child = spawn(executablePath, args, {
      cwd,
      env,
      signal: options.signal,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let spawnError: unknown | null = null;
    child.once("error", (err) => (spawnError = err));

    if (!child.stdout) {
      child.kill();
      throw new Error("rip child process has no stdout");
    }

    const stderrChunks: Buffer[] = [];
    if (child.stderr) {
      child.stderr.on("data", (chunk) => stderrChunks.push(Buffer.from(chunk)));
    }

    const exitPromise = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
      child.once("exit", (code, signal) => resolve({ code, signal }));
    });

    const rl = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });

    const frames: RipEventFrame[] = [];
    let finalOutput = "";

    const queue: RipEventFrame[] = [];
    let wake: (() => void) | null = null;
    let ended = false;
    let streamError: unknown | null = null;

    const wakeWaiters = () => {
      if (wake) {
        const resolve = wake;
        wake = null;
        resolve();
      }
    };

    const result = (async (): Promise<RipTurn> => {
      try {
        for await (const line of rl) {
          const trimmed = line.trim();
          if (!trimmed) continue;

          let frame: RipEventFrame;
          try {
            frame = JSON.parse(trimmed) as RipEventFrame;
          } catch (err) {
            throw new Error(`rip JSONL parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
          }

          frames.push(frame);
          queue.push(frame);
          if (frame.type === "output_text_delta" && typeof frame.delta === "string") {
            finalOutput += frame.delta;
          }
          wakeWaiters();
        }

        if (spawnError) throw spawnError;

        const { code, signal } = await exitPromise;
        const stderr = Buffer.concat(stderrChunks).toString("utf8");
        if (code !== 0 || signal) {
          throw new RipExecError(`rip exited with ${signal ? `signal ${signal}` : `code ${code ?? 1}`}`, {
            exitCode: code,
            signal,
            stderr,
          });
        }

        return { frames, finalOutput, exitCode: code ?? 0 };
      } finally {
        ended = true;
        wakeWaiters();
        rl.close();
        child.removeAllListeners();
      }
    })().catch((err) => {
      streamError = err;
      ended = true;
      wakeWaiters();
      throw err;
    });

    async function* events(): AsyncGenerator<RipEventFrame> {
      while (true) {
        if (queue.length > 0) {
          yield queue.shift()!;
          continue;
        }
        if (streamError) throw streamError;
        if (ended) return;
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
      }
    }

    return { events: events(), result };
  }

  private async runStreamedHttp(
    prompt: string,
    options: RipRunOptions,
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipTurn> }> {
    const server = options.server ?? this.base.server;
    if (!server) throw new Error("runStreamed with http transport requires server");

    const controller = new AbortController();
    const abort = () => controller.abort();
    if (options.signal) {
      if (options.signal.aborted) abort();
      else options.signal.addEventListener("abort", abort, { once: true });
    }

    const config = this.httpConfig(server, options);
    const created = (await httpJson(config, "/sessions", { method: "POST", signal: controller.signal })) as {
      session_id?: unknown;
    };
    const sessionId = typeof created?.session_id === "string" ? created.session_id : null;
    if (!sessionId) throw new Error("http session create response missing session_id");

    const eventsResponse = await httpRequest(config, `/sessions/${sessionId}/events`, {
      method: "GET",
      signal: controller.signal,
      headers: { accept: "text/event-stream" },
    });

    const inputBody = JSON.stringify({ input: prompt });
    await httpRequest(config, `/sessions/${sessionId}/input`, {
      method: "POST",
      signal: controller.signal,
      headers: { "content-type": "application/json" },
      body: inputBody,
    });

    const frames: RipEventFrame[] = [];
    let finalOutput = "";

    const queue: RipEventFrame[] = [];
    let wake: (() => void) | null = null;
    let ended = false;
    let streamError: unknown | null = null;
    let reason: string | null = null;

    const wakeWaiters = () => {
      if (wake) {
        const resolve = wake;
        wake = null;
        resolve();
      }
    };

    const result = (async (): Promise<RipTurn> => {
      try {
        for await (const data of sseDataMessages(eventsResponse)) {
          const trimmed = data.trim();
          if (!trimmed) continue;
          if (trimmed === "ping") continue;

          let frame: RipEventFrame;
          try {
            frame = JSON.parse(trimmed) as RipEventFrame;
          } catch (err) {
            throw new Error(`http SSE JSON parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
          }

          frames.push(frame);
          queue.push(frame);
          if (frame.type === "output_text_delta" && typeof frame.delta === "string") {
            finalOutput += frame.delta;
          }
          if (frame.type === "session_ended") {
            reason = typeof frame.reason === "string" ? frame.reason : null;
            break;
          }
          wakeWaiters();
        }

        return { frames, finalOutput, exitCode: reason === "completed" || reason === null ? 0 : 1 };
      } catch (err) {
        if (controller.signal.aborted && reason !== null) {
          return { frames, finalOutput, exitCode: reason === "completed" ? 0 : 1 };
        }
        throw err;
      } finally {
        ended = true;
        abort();
        wakeWaiters();
      }
    })().catch((err) => {
      streamError = err;
      ended = true;
      abort();
      wakeWaiters();
      throw err;
    });

    async function* events(): AsyncGenerator<RipEventFrame> {
      while (true) {
        if (queue.length > 0) {
          yield queue.shift()!;
          continue;
        }
        if (streamError) throw streamError;
        if (ended) return;
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
      }
    }

    return { events: events(), result };
  }

  async threadEnsure(options: RipThreadOptions = {}): Promise<RipThreadEnsureResponse> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadEnsure with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, "/threads/ensure", { method: "POST", signal: options.signal });
      return out as RipThreadEnsureResponse;
    }
    const out = await this.execJson(buildRipThreadEnsureArgs({ server: options.server }), options);
    return out as RipThreadEnsureResponse;
  }

  async threadList(options: RipThreadOptions = {}): Promise<RipThreadMeta[]> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadList with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, "/threads", { method: "GET", signal: options.signal });
      return out as RipThreadMeta[];
    }
    const out = await this.execJson(buildRipThreadListArgs({ server: options.server }), options);
    return out as RipThreadMeta[];
  }

  async threadGet(threadId: string, options: RipThreadOptions = {}): Promise<RipThreadMeta> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadGet with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, `/threads/${threadId}`, { method: "GET", signal: options.signal });
      return out as RipThreadMeta;
    }
    const out = await this.execJson(buildRipThreadGetArgs(threadId, { server: options.server }), options);
    return out as RipThreadMeta;
  }

  async threadBranch(
    parentThreadId: string,
    request: RipThreadBranchRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadBranchResponse> {
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadBranch with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        title: request.title ?? null,
        from_message_id: request.from_message_id ?? null,
        from_seq: request.from_seq ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${parentThreadId}/branch`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadBranchResponse;
    }
    const out = await this.execJson(
      buildRipThreadBranchArgs(parentThreadId, {
        server: options.server,
        title: request.title,
        fromMessageId: request.from_message_id,
        fromSeq: request.from_seq,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadBranchResponse;
  }

  async threadHandoff(
    fromThreadId: string,
    request: RipThreadHandoffRequest,
    options: RipThreadOptions = {},
  ): Promise<RipThreadHandoffResponse> {
    if (!request.summary_markdown && !request.summary_artifact_id) {
      throw new Error("threadHandoff requires summary_markdown and/or summary_artifact_id");
    }
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadHandoff with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        title: request.title ?? null,
        summary_markdown: request.summary_markdown ?? null,
        summary_artifact_id: request.summary_artifact_id ?? null,
        from_message_id: request.from_message_id ?? null,
        from_seq: request.from_seq ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${fromThreadId}/handoff`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadHandoffResponse;
    }
    const out = await this.execJson(
      buildRipThreadHandoffArgs(fromThreadId, {
        server: options.server,
        title: request.title,
        summaryMarkdown: request.summary_markdown,
        summaryArtifactId: request.summary_artifact_id,
        fromMessageId: request.from_message_id,
        fromSeq: request.from_seq,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadHandoffResponse;
  }

  async threadPostMessage(
    threadId: string,
    request: RipThreadPostMessageRequest,
    options: RipThreadOptions = {},
  ): Promise<RipThreadPostMessageResponse> {
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadPostMessage with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({ content: request.content, actor_id: actorId, origin });
      const out = await httpJson(config, `/threads/${threadId}/messages`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadPostMessageResponse;
    }
    const out = await this.execJson(
      buildRipThreadPostMessageArgs(threadId, request.content, {
        server: options.server,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadPostMessageResponse;
  }

  async threadCompactionCheckpoint(
    threadId: string,
    request: RipThreadCompactionCheckpointRequest,
    options: RipThreadOptions = {},
  ): Promise<RipThreadCompactionCheckpointResponse> {
    if (!request.summary_markdown && !request.summary_artifact_id) {
      throw new Error("threadCompactionCheckpoint requires summary_markdown and/or summary_artifact_id");
    }
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadCompactionCheckpoint with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        summary_markdown: request.summary_markdown ?? null,
        summary_artifact_id: request.summary_artifact_id ?? null,
        to_message_id: request.to_message_id ?? null,
        to_seq: request.to_seq ?? null,
        stride_messages: request.stride_messages ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${threadId}/compaction-checkpoint`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadCompactionCheckpointResponse;
    }
    const out = await this.execJson(
      buildRipThreadCompactionCheckpointArgs(threadId, {
        server: options.server,
        summaryMarkdown: request.summary_markdown,
        summaryArtifactId: request.summary_artifact_id,
        toMessageId: request.to_message_id,
        toSeq: request.to_seq,
        strideMessages: request.stride_messages,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadCompactionCheckpointResponse;
  }

  async threadCompactionCutPoints(
    threadId: string,
    request: RipThreadCompactionCutPointsRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadCompactionCutPointsResponse> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadCompactionCutPoints with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        stride_messages: request.stride_messages ?? null,
        limit: request.limit ?? null,
      });
      const out = await httpJson(config, `/threads/${threadId}/compaction-cut-points`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadCompactionCutPointsResponse;
    }
    const out = await this.execJson(
      buildRipThreadCompactionCutPointsArgs(threadId, {
        server: options.server,
        strideMessages: request.stride_messages,
        limit: request.limit,
      }),
      options,
    );
    return out as RipThreadCompactionCutPointsResponse;
  }

  async threadCompactionStatus(
    threadId: string,
    request: RipThreadCompactionStatusRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadCompactionStatusResponse> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadCompactionStatus with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        stride_messages: request.stride_messages ?? null,
      });
      const out = await httpJson(config, `/threads/${threadId}/compaction-status`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadCompactionStatusResponse;
    }
    const out = await this.execJson(
      buildRipThreadCompactionStatusArgs(threadId, {
        server: options.server,
        strideMessages: request.stride_messages,
      }),
      options,
    );
    return out as RipThreadCompactionStatusResponse;
  }

  async threadProviderCursorStatus(
    threadId: string,
    options: RipThreadOptions = {},
  ): Promise<RipThreadProviderCursorStatusResponse> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadProviderCursorStatus with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, `/threads/${threadId}/provider-cursor-status`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({}),
      });
      return out as RipThreadProviderCursorStatusResponse;
    }
    const out = await this.execJson(
      buildRipThreadProviderCursorStatusArgs(threadId, { server: options.server }),
      options,
    );
    return out as RipThreadProviderCursorStatusResponse;
  }

  async threadProviderCursorRotate(
    threadId: string,
    request: RipThreadProviderCursorRotateRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadProviderCursorRotateResponse> {
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadProviderCursorRotate with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        provider: null,
        endpoint: null,
        model: null,
        reason: request.reason ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${threadId}/provider-cursor-rotate`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadProviderCursorRotateResponse;
    }
    const out = await this.execJson(
      buildRipThreadProviderCursorRotateArgs(threadId, {
        server: options.server,
        reason: request.reason,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadProviderCursorRotateResponse;
  }

  async threadContextSelectionStatus(
    threadId: string,
    request: RipThreadContextSelectionStatusRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadContextSelectionStatusResponse> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadContextSelectionStatus with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        limit: request.limit ?? null,
      });
      const out = await httpJson(config, `/threads/${threadId}/context-selection-status`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadContextSelectionStatusResponse;
    }
    const out = await this.execJson(
      buildRipThreadContextSelectionStatusArgs(threadId, {
        server: options.server,
        limit: request.limit,
      }),
      options,
    );
    return out as RipThreadContextSelectionStatusResponse;
  }

  async threadCompactionAuto(
    threadId: string,
    request: RipThreadCompactionAutoRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadCompactionAutoResponse> {
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadCompactionAuto with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        stride_messages: request.stride_messages ?? null,
        max_new_checkpoints: request.max_new_checkpoints ?? null,
        dry_run: request.dry_run ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${threadId}/compaction-auto`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadCompactionAutoResponse;
    }
    const out = await this.execJson(
      buildRipThreadCompactionAutoArgs(threadId, {
        server: options.server,
        strideMessages: request.stride_messages,
        maxNewCheckpoints: request.max_new_checkpoints,
        dryRun: request.dry_run,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadCompactionAutoResponse;
  }

  async threadCompactionAutoSchedule(
    threadId: string,
    request: RipThreadCompactionAutoScheduleRequest = {},
    options: RipThreadOptions = {},
  ): Promise<RipThreadCompactionAutoScheduleResponse> {
    const actorId = request.actor_id ?? "user";
    const origin = request.origin ?? "sdk-ts";
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadCompactionAutoSchedule with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        stride_messages: request.stride_messages ?? null,
        max_new_checkpoints: request.max_new_checkpoints ?? null,
        block_on_inflight: !(request.allow_inflight ?? false),
        execute: !(request.no_execute ?? false),
        dry_run: request.dry_run ?? null,
        actor_id: actorId,
        origin,
      });
      const out = await httpJson(config, `/threads/${threadId}/compaction-auto-schedule`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipThreadCompactionAutoScheduleResponse;
    }
    const out = await this.execJson(
      buildRipThreadCompactionAutoScheduleArgs(threadId, {
        server: options.server,
        strideMessages: request.stride_messages,
        maxNewCheckpoints: request.max_new_checkpoints,
        allowInflight: request.allow_inflight,
        noExecute: request.no_execute,
        dryRun: request.dry_run,
        actorId,
        origin,
      }),
      options,
    );
    return out as RipThreadCompactionAutoScheduleResponse;
  }

  async threadEventsStreamed(
    threadId: string,
    options: RipThreadOptions = {},
    query: { maxEvents?: number } = {},
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipEventFrame[]> }> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("threadEventsStreamed with http transport requires server");
      const config = this.httpConfig(server, options);
      const controller = new AbortController();
      const abort = () => controller.abort();
      if (options.signal) {
        if (options.signal.aborted) abort();
        else options.signal.addEventListener("abort", abort, { once: true });
      }

      const response = await httpRequest(config, `/threads/${threadId}/events`, {
        method: "GET",
        signal: controller.signal,
        headers: { accept: "text/event-stream" },
      });

      const frames: RipEventFrame[] = [];
      const queue: RipEventFrame[] = [];
      let wake: (() => void) | null = null;
      let ended = false;
      let streamError: unknown | null = null;

      const wakeWaiters = () => {
        if (wake) {
          const resolve = wake;
          wake = null;
          resolve();
        }
      };

      const result = (async (): Promise<RipEventFrame[]> => {
        try {
          for await (const data of sseDataMessages(response)) {
            const trimmed = data.trim();
            if (!trimmed) continue;
            if (trimmed === "ping") continue;

            let frame: RipEventFrame;
            try {
              frame = JSON.parse(trimmed) as RipEventFrame;
            } catch (err) {
              throw new Error(`http SSE JSON parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
            }

            frames.push(frame);
            queue.push(frame);
            wakeWaiters();

            if (typeof query.maxEvents === "number" && frames.length >= query.maxEvents) break;
          }
          return frames;
        } finally {
          ended = true;
          abort();
          wakeWaiters();
        }
      })().catch((err) => {
        streamError = err;
        ended = true;
        abort();
        wakeWaiters();
        throw err;
      });

      async function* events(): AsyncGenerator<RipEventFrame> {
        while (true) {
          if (queue.length > 0) {
            yield queue.shift()!;
            continue;
          }
          if (streamError) throw streamError;
          if (ended) return;
          await new Promise<void>((resolve) => {
            wake = resolve;
          });
        }
      }

      return { events: events(), result };
    }
    const args = buildRipThreadEventsArgs(threadId, {
      server: options.server,
      maxEvents: query.maxEvents,
    });
    const { events, result } = await this.execJsonlFrames(args, options);
    return { events, result };
  }

  async taskSpawn(request: RipTaskSpawnRequest, options: RipTaskOptions): Promise<RipTaskCreated> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskSpawn with http transport requires server");
      const config = this.httpConfig(server, options);
      const body = JSON.stringify({
        tool: request.tool,
        args: request.args ?? null,
        title: request.title ?? null,
        execution_mode: request.execution_mode ?? "pipes",
      });
      const out = await httpJson(config, "/tasks", {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body,
      });
      return out as RipTaskCreated;
    }
    const executionMode = request.execution_mode ?? "pipes";
    const out = await this.execJson(
      buildRipTaskArgs(
        options.server,
        "spawn",
        "--tool",
        request.tool,
        "--args",
        JSON.stringify(request.args),
        ...(request.title ? ["--title", request.title] : []),
        "--execution-mode",
        executionMode,
      ),
      options,
    );
    return out as RipTaskCreated;
  }

  async taskList(options: RipTaskOptions): Promise<RipTaskStatus[]> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskList with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, "/tasks", { method: "GET", signal: options.signal });
      return out as RipTaskStatus[];
    }
    const out = await this.execJson(buildRipTaskArgs(options.server, "list"), options);
    return out as RipTaskStatus[];
  }

  async taskStatus(taskId: string, options: RipTaskOptions): Promise<RipTaskStatus> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskStatus with http transport requires server");
      const config = this.httpConfig(server, options);
      const out = await httpJson(config, `/tasks/${taskId}`, { method: "GET", signal: options.signal });
      return out as RipTaskStatus;
    }
    const out = await this.execJson(buildRipTaskArgs(options.server, "status", taskId), options);
    return out as RipTaskStatus;
  }

  async taskCancel(taskId: string, options: RipTaskOptions, reason?: string): Promise<void> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskCancel with http transport requires server");
      const config = this.httpConfig(server, options);
      await httpRequest(config, `/tasks/${taskId}/cancel`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ reason: reason ?? null }),
      });
      return;
    }
    await this.execRaw(
      buildRipTaskArgs(options.server, "cancel", taskId, ...(reason ? ["--reason", reason] : [])),
      options,
    );
  }

  async taskOutput(
    taskId: string,
    options: RipTaskOptions,
    query: { stream?: "stdout" | "stderr" | "pty"; offsetBytes?: number; maxBytes?: number } = {},
  ): Promise<RipTaskOutput> {
    const stream = query.stream ?? "stdout";
    const offset = query.offsetBytes ?? 0;
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskOutput with http transport requires server");
      const config = this.httpConfig(server, options);
      const qs = new URLSearchParams();
      qs.set("stream", stream);
      qs.set("offset_bytes", String(offset));
      if (typeof query.maxBytes === "number") qs.set("max_bytes", String(query.maxBytes));
      const out = await httpJson(config, `/tasks/${taskId}/output?${qs.toString()}`, { method: "GET", signal: options.signal });
      return out as RipTaskOutput;
    }
    const args = buildRipTaskArgs(options.server, "output", taskId, "--stream", stream, "--offset-bytes", String(offset));
    if (typeof query.maxBytes === "number") args.push("--max-bytes", String(query.maxBytes));
    const out = await this.execJson(args, options);
    return out as RipTaskOutput;
  }

  async taskWriteStdin(taskId: string, options: RipTaskOptions, chunk: Uint8Array): Promise<void> {
    const chunkB64 = Buffer.from(chunk).toString("base64");
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskWriteStdin with http transport requires server");
      const config = this.httpConfig(server, options);
      await httpRequest(config, `/tasks/${taskId}/stdin`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ chunk_b64: chunkB64 }),
      });
      return;
    }
    await this.execRaw(buildRipTaskArgs(options.server, "stdin", taskId, "--chunk-b64", chunkB64), options);
  }

  async taskWriteStdinText(
    taskId: string,
    options: RipTaskOptions,
    text: string,
    opts: { noNewline?: boolean } = {},
  ): Promise<void> {
    const payload = opts.noNewline ? text : `${text}\n`;
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskWriteStdinText with http transport requires server");
      const config = this.httpConfig(server, options);
      const chunkB64 = Buffer.from(payload).toString("base64");
      await httpRequest(config, `/tasks/${taskId}/stdin`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ chunk_b64: chunkB64 }),
      });
      return;
    }
    await this.execRaw(buildRipTaskArgs(options.server, "stdin", taskId, "--text", payload, "--no-newline"), options);
  }

  async taskResize(taskId: string, options: RipTaskOptions, size: { rows: number; cols: number }): Promise<void> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskResize with http transport requires server");
      const config = this.httpConfig(server, options);
      await httpRequest(config, `/tasks/${taskId}/resize`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ rows: size.rows, cols: size.cols }),
      });
      return;
    }
    await this.execRaw(
      buildRipTaskArgs(options.server, "resize", taskId, "--rows", String(size.rows), "--cols", String(size.cols)),
      options,
    );
  }

  async taskSignal(taskId: string, options: RipTaskOptions, signal: string): Promise<void> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskSignal with http transport requires server");
      const config = this.httpConfig(server, options);
      await httpRequest(config, `/tasks/${taskId}/signal`, {
        method: "POST",
        signal: options.signal,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ signal }),
      });
      return;
    }
    await this.execRaw(buildRipTaskArgs(options.server, "signal", taskId, signal), options);
  }

  async taskEventsStreamed(
    taskId: string,
    options: RipTaskOptions,
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipEventFrame[]> }> {
    const transport = resolveTransport(options.transport ?? this.base.transport);
    if (transport === "http") {
      const server = options.server ?? this.base.server;
      if (!server) throw new Error("taskEventsStreamed with http transport requires server");
      const config = this.httpConfig(server, options);
      const controller = new AbortController();
      const abort = () => controller.abort();
      if (options.signal) {
        if (options.signal.aborted) abort();
        else options.signal.addEventListener("abort", abort, { once: true });
      }

      const response = await httpRequest(config, `/tasks/${taskId}/events`, {
        method: "GET",
        signal: controller.signal,
        headers: { accept: "text/event-stream" },
      });

      const frames: RipEventFrame[] = [];
      const queue: RipEventFrame[] = [];
      let wake: (() => void) | null = null;
      let ended = false;
      let streamError: unknown | null = null;

      const wakeWaiters = () => {
        if (wake) {
          const resolve = wake;
          wake = null;
          resolve();
        }
      };

      const result = (async (): Promise<RipEventFrame[]> => {
        try {
          for await (const data of sseDataMessages(response)) {
            const trimmed = data.trim();
            if (!trimmed) continue;
            if (trimmed === "ping") continue;

            let frame: RipEventFrame;
            try {
              frame = JSON.parse(trimmed) as RipEventFrame;
            } catch (err) {
              throw new Error(`http SSE JSON parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
            }

            frames.push(frame);
            queue.push(frame);
            wakeWaiters();
          }
          return frames;
        } finally {
          ended = true;
          abort();
          wakeWaiters();
        }
      })().catch((err) => {
        streamError = err;
        ended = true;
        abort();
        wakeWaiters();
        throw err;
      });

      async function* events(): AsyncGenerator<RipEventFrame> {
        while (true) {
          if (queue.length > 0) {
            yield queue.shift()!;
            continue;
          }
          if (streamError) throw streamError;
          if (ended) return;
          await new Promise<void>((resolve) => {
            wake = resolve;
          });
        }
      }

      return { events: events(), result };
    }
    const args = buildRipTaskArgs(options.server, "events", taskId);
    const { events, result } = await this.execJsonlFrames(args, options);
    return { events, result };
  }

  private async execRaw(
    args: string[],
    options: RipTaskOptions | RipRunOptions | RipThreadOptions,
  ): Promise<{ stdout: string; stderr: string }> {
    const executablePath = options.executablePath ?? this.base.executablePath ?? "rip";
    const cwd = options.cwd ?? this.base.cwd;
    const env = mergeEnv(process.env, this.base.env, options.env);
    unsetEnvVars(env, this.base.unsetEnv);
    unsetEnvVars(env, options.unsetEnv);

    const child = spawn(executablePath, args, {
      cwd,
      env,
      signal: options.signal,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let spawnError: unknown | null = null;
    child.once("error", (err) => (spawnError = err));

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    if (child.stdout) child.stdout.on("data", (chunk) => stdoutChunks.push(Buffer.from(chunk)));
    if (child.stderr) child.stderr.on("data", (chunk) => stderrChunks.push(Buffer.from(chunk)));

    const exitPromise = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
      child.once("exit", (code, signal) => resolve({ code, signal }));
    });

    const { code, signal } = await exitPromise;
    if (spawnError) throw spawnError;

    const stdout = Buffer.concat(stdoutChunks).toString("utf8");
    const stderr = Buffer.concat(stderrChunks).toString("utf8");
    if (code !== 0 || signal) {
      throw new RipExecError(`rip exited with ${signal ? `signal ${signal}` : `code ${code ?? 1}`}`, { exitCode: code, signal, stderr });
    }
    return { stdout, stderr };
  }

  private async execJson(args: string[], options: RipTaskOptions | RipRunOptions | RipThreadOptions): Promise<unknown> {
    const { stdout } = await this.execRaw(args, options);
    const trimmed = stdout.trim();
    if (!trimmed) return null;
    return JSON.parse(trimmed) as unknown;
  }

  private async execJsonlFrames(
    args: string[],
    options: RipTaskOptions | RipRunOptions | RipThreadOptions,
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipEventFrame[]> }> {
    const executablePath = options.executablePath ?? this.base.executablePath ?? "rip";
    const cwd = options.cwd ?? this.base.cwd;
    const env = mergeEnv(process.env, this.base.env, options.env);
    unsetEnvVars(env, this.base.unsetEnv);
    unsetEnvVars(env, options.unsetEnv);

    const child = spawn(executablePath, args, {
      cwd,
      env,
      signal: options.signal,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let spawnError: unknown | null = null;
    child.once("error", (err) => (spawnError = err));

    if (!child.stdout) {
      child.kill();
      throw new Error("rip child process has no stdout");
    }

    const stderrChunks: Buffer[] = [];
    if (child.stderr) {
      child.stderr.on("data", (chunk) => stderrChunks.push(Buffer.from(chunk)));
    }

    const exitPromise = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
      child.once("exit", (code, signal) => resolve({ code, signal }));
    });

    const rl = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });

    const frames: RipEventFrame[] = [];
    const queue: RipEventFrame[] = [];
    let wake: (() => void) | null = null;
    let ended = false;
    let streamError: unknown | null = null;

    const wakeWaiters = () => {
      if (wake) {
        const resolve = wake;
        wake = null;
        resolve();
      }
    };

    const result = (async (): Promise<RipEventFrame[]> => {
      try {
        for await (const line of rl) {
          const trimmed = line.trim();
          if (!trimmed) continue;

          let frame: RipEventFrame;
          try {
            frame = JSON.parse(trimmed) as RipEventFrame;
          } catch (err) {
            throw new Error(`rip JSONL parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
          }

          frames.push(frame);
          queue.push(frame);
          wakeWaiters();
        }

        if (spawnError) throw spawnError;

        const { code, signal } = await exitPromise;
        const stderr = Buffer.concat(stderrChunks).toString("utf8");
        if (code !== 0 || signal) {
          throw new RipExecError(`rip exited with ${signal ? `signal ${signal}` : `code ${code ?? 1}`}`, {
            exitCode: code,
            signal,
            stderr,
          });
        }

        return frames;
      } finally {
        ended = true;
        wakeWaiters();
        rl.close();
        child.removeAllListeners();
      }
    })().catch((err) => {
      streamError = err;
      ended = true;
      wakeWaiters();
      throw err;
    });

    async function* events(): AsyncGenerator<RipEventFrame> {
      while (true) {
        if (queue.length > 0) {
          yield queue.shift()!;
          continue;
        }
        if (streamError) throw streamError;
        if (ended) return;
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
      }
    }

    return { events: events(), result };
  }

  private httpConfig(
    server: string,
    options: { headers?: Record<string, string>; fetch?: RipFetch },
  ): RipHttpConfig {
    return {
      server,
      headers: { ...(this.base.headers ?? {}), ...(options.headers ?? {}) },
      fetch: options.fetch ?? this.base.fetch,
    };
  }
}

function mergeEnv(...envs: Array<NodeJS.ProcessEnv | undefined>): Record<string, string> {
  const merged: Record<string, string> = {};
  for (const env of envs) {
    if (!env) continue;
    for (const [key, value] of Object.entries(env)) {
      if (value !== undefined) merged[key] = value;
    }
  }
  return merged;
}

function buildRipTaskArgs(server: string | undefined, ...rest: string[]): string[] {
  const args: string[] = ["tasks"];
  if (server) {
    args.push("--server", server);
  }
  args.push(...rest);
  return args;
}

function unsetEnvVars(env: Record<string, string>, unset: readonly string[] | undefined) {
  if (!unset?.length) return;
  for (const key of unset) {
    delete env[key];
  }
}

function resolveTransport(transport: RipTransport | undefined): RipTransport {
  return transport ?? "exec";
}
