import { spawn } from "node:child_process";
import readline from "node:readline";

import type { RipEventFrame } from "./frames.js";
import { buildRipRunArgs } from "./util.js";

export type RipOptions = {
  executablePath?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: string[];
};

export type RipRunOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: string[];
  executablePath?: string;
  signal?: AbortSignal;
  extraArgs?: string[];
};

export type RipTaskOptions = {
  server: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  unsetEnv?: string[];
  executablePath?: string;
  signal?: AbortSignal;
};

export type RipTurn = {
  frames: RipEventFrame[];
  finalOutput: string;
  exitCode: number;
};

export type RipTaskSpawnRequest = {
  tool: string;
  args: unknown;
  title?: string;
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
  stream: "stdout" | "stderr";
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

  async taskSpawn(request: RipTaskSpawnRequest, options: RipTaskOptions): Promise<RipTaskCreated> {
    const payload = {
      tool: request.tool,
      args: request.args,
      title: request.title ?? null,
      execution_mode: "pipes",
    };
    const out = await this.execJson(["tasks", "--server", options.server, "spawn", "--tool", request.tool, "--args", JSON.stringify(payload.args), ...(request.title ? ["--title", request.title] : [])], options);
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

  async taskOutput(taskId: string, options: RipTaskOptions, query: { stream?: "stdout" | "stderr"; offsetBytes?: number; maxBytes?: number } = {}): Promise<RipTaskOutput> {
    const stream = query.stream ?? "stdout";
    const offset = query.offsetBytes ?? 0;
    const args = ["tasks", "--server", options.server, "output", taskId, "--stream", stream, "--offset-bytes", String(offset)];
    if (typeof query.maxBytes === "number") args.push("--max-bytes", String(query.maxBytes));
    const out = await this.execJson(args, options);
    return out as RipTaskOutput;
  }

  async taskEventsStreamed(
    taskId: string,
    options: RipTaskOptions,
  ): Promise<{ events: AsyncGenerator<RipEventFrame>; result: Promise<RipEventFrame[]> }> {
    const args = ["tasks", "--server", options.server, "events", taskId];
    const { events, result } = await this.execJsonlFrames(args, options);
    return { events, result };
  }

  private async execRaw(args: string[], options: RipTaskOptions | RipRunOptions): Promise<{ stdout: string; stderr: string }> {
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

  private async execJson(args: string[], options: RipTaskOptions | RipRunOptions): Promise<unknown> {
    const { stdout } = await this.execRaw(args, options);
    const trimmed = stdout.trim();
    if (!trimmed) return null;
    return JSON.parse(trimmed) as unknown;
  }

  private async execJsonlFrames(
    args: string[],
    options: RipTaskOptions | RipRunOptions,
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

function unsetEnvVars(env: Record<string, string>, unset: string[] | undefined) {
  if (!unset?.length) return;
  for (const key of unset) {
    delete env[key];
  }
}
