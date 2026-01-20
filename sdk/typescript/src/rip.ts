import { spawn } from "node:child_process";
import readline from "node:readline";

import type { RipEventFrame } from "./frames.js";
import { buildRipRunArgs } from "./util.js";

export type RipOptions = {
  executablePath?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
};

export type RipRunOptions = {
  server?: string;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  executablePath?: string;
  signal?: AbortSignal;
  extraArgs?: string[];
};

export type RipTurn = {
  frames: RipEventFrame[];
  finalOutput: string;
  exitCode: number;
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

