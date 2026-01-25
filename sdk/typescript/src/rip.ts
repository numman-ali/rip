import { spawn } from "node:child_process";
import readline from "node:readline";

import type { RipEventFrame } from "./frames.js";
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

export type RipOptions = {
  executablePath?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
};

export type RipRunOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
  extraArgs?: string[];
};

export type RipTaskOptions = {
  server: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
};

export type RipThreadOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: readonly string[];
  executablePath?: string;
  signal?: AbortSignal;
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

  async threadEnsure(options: RipThreadOptions = {}): Promise<RipThreadEnsureResponse> {
    const out = await this.execJson(buildRipThreadEnsureArgs({ server: options.server }), options);
    return out as RipThreadEnsureResponse;
  }

  async threadList(options: RipThreadOptions = {}): Promise<RipThreadMeta[]> {
    const out = await this.execJson(buildRipThreadListArgs({ server: options.server }), options);
    return out as RipThreadMeta[];
  }

  async threadGet(threadId: string, options: RipThreadOptions = {}): Promise<RipThreadMeta> {
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
    const args = buildRipThreadEventsArgs(threadId, {
      server: options.server,
      maxEvents: query.maxEvents,
    });
    const { events, result } = await this.execJsonlFrames(args, options);
    return { events, result };
  }

  async taskSpawn(request: RipTaskSpawnRequest, options: RipTaskOptions): Promise<RipTaskCreated> {
    const executionMode = request.execution_mode ?? "pipes";
    const out = await this.execJson(
      [
        "tasks",
        "--server",
        options.server,
        "spawn",
        "--tool",
        request.tool,
        "--args",
        JSON.stringify(request.args),
        ...(request.title ? ["--title", request.title] : []),
        "--execution-mode",
        executionMode,
      ],
      options,
    );
    return out as RipTaskCreated;
  }

  async taskList(options: RipTaskOptions): Promise<RipTaskStatus[]> {
    const out = await this.execJson(["tasks", "--server", options.server, "list"], options);
    return out as RipTaskStatus[];
  }

  async taskStatus(taskId: string, options: RipTaskOptions): Promise<RipTaskStatus> {
    const out = await this.execJson(["tasks", "--server", options.server, "status", taskId], options);
    return out as RipTaskStatus;
  }

  async taskCancel(taskId: string, options: RipTaskOptions, reason?: string): Promise<void> {
    await this.execRaw(
      ["tasks", "--server", options.server, "cancel", taskId, ...(reason ? ["--reason", reason] : [])],
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
    const args = ["tasks", "--server", options.server, "output", taskId, "--stream", stream, "--offset-bytes", String(offset)];
    if (typeof query.maxBytes === "number") args.push("--max-bytes", String(query.maxBytes));
    const out = await this.execJson(args, options);
    return out as RipTaskOutput;
  }

  async taskWriteStdin(taskId: string, options: RipTaskOptions, chunk: Uint8Array): Promise<void> {
    const chunkB64 = Buffer.from(chunk).toString("base64");
    await this.execRaw(["tasks", "--server", options.server, "stdin", taskId, "--chunk-b64", chunkB64], options);
  }

  async taskWriteStdinText(
    taskId: string,
    options: RipTaskOptions,
    text: string,
    opts: { noNewline?: boolean } = {},
  ): Promise<void> {
    const payload = opts.noNewline ? text : `${text}\n`;
    await this.execRaw(["tasks", "--server", options.server, "stdin", taskId, "--text", payload, "--no-newline"], options);
  }

  async taskResize(taskId: string, options: RipTaskOptions, size: { rows: number; cols: number }): Promise<void> {
    await this.execRaw(
      ["tasks", "--server", options.server, "resize", taskId, "--rows", String(size.rows), "--cols", String(size.cols)],
      options,
    );
  }

  async taskSignal(taskId: string, options: RipTaskOptions, signal: string): Promise<void> {
    await this.execRaw(["tasks", "--server", options.server, "signal", taskId, signal], options);
  }

  async taskEventsStreamed(
    taskId: string,
    options: RipTaskOptions,
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipEventFrame[]> }> {
    const args = ["tasks", "--server", options.server, "events", taskId];
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

function unsetEnvVars(env: Record<string, string>, unset: readonly string[] | undefined) {
  if (!unset?.length) return;
  for (const key of unset) {
    delete env[key];
  }
}
