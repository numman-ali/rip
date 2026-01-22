import type { RipEventFrame } from "./frames.js";

export type RipRunArgsOptions = {
  server?: string;
  extraArgs?: string[];
};

export function buildRipRunArgs(prompt: string, options: RipRunArgsOptions = {}): string[] {
  const args: string[] = ["run", prompt, "--headless", "true", "--view", "raw"];
  if (options.server) {
    args.push("--server", options.server);
  }
  if (options.extraArgs?.length) {
    args.push(...options.extraArgs);
  }
  return args;
}

export type RipThreadsArgsOptions = {
  server?: string;
};

function buildRipThreadsBaseArgs(options: RipThreadsArgsOptions): string[] {
  const args: string[] = ["threads"];
  if (options.server) {
    args.push("--server", options.server);
  }
  return args;
}

export function buildRipThreadEnsureArgs(options: RipThreadsArgsOptions = {}): string[] {
  return [...buildRipThreadsBaseArgs(options), "ensure"];
}

export function buildRipThreadListArgs(options: RipThreadsArgsOptions = {}): string[] {
  return [...buildRipThreadsBaseArgs(options), "list"];
}

export function buildRipThreadGetArgs(threadId: string, options: RipThreadsArgsOptions = {}): string[] {
  return [...buildRipThreadsBaseArgs(options), "get", threadId];
}

export type RipThreadPostMessageArgsOptions = RipThreadsArgsOptions & {
  actorId?: string;
  origin?: string;
};

export function buildRipThreadPostMessageArgs(
  threadId: string,
  content: string,
  options: RipThreadPostMessageArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "post-message", threadId, "--content", content];
  if (options.actorId) args.push("--actor-id", options.actorId);
  if (options.origin) args.push("--origin", options.origin);
  return args;
}

export type RipThreadEventsArgsOptions = RipThreadsArgsOptions & {
  maxEvents?: number;
};

export function buildRipThreadEventsArgs(threadId: string, options: RipThreadEventsArgsOptions = {}): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "events", threadId];
  if (typeof options.maxEvents === "number") args.push("--max-events", String(options.maxEvents));
  return args;
}

export function collectOutputText(frames: readonly RipEventFrame[]): string {
  let out = "";
  for (const frame of frames) {
    if (frame.type !== "output_text_delta") continue;
    const delta = frame.delta;
    if (typeof delta === "string") out += delta;
  }
  return out;
}
