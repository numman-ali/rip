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

export type RipThreadBranchArgsOptions = RipThreadsArgsOptions & {
  title?: string;
  fromMessageId?: string;
  fromSeq?: number;
  actorId?: string;
  origin?: string;
};

export function buildRipThreadBranchArgs(
  parentThreadId: string,
  options: RipThreadBranchArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "branch", parentThreadId];
  if (options.title) args.push("--title", options.title);
  if (options.fromMessageId) args.push("--from-message-id", options.fromMessageId);
  if (typeof options.fromSeq === "number") args.push("--from-seq", String(options.fromSeq));
  if (options.actorId) args.push("--actor-id", options.actorId);
  if (options.origin) args.push("--origin", options.origin);
  return args;
}

export type RipThreadHandoffArgsOptions = RipThreadsArgsOptions & {
  title?: string;
  summaryMarkdown?: string;
  summaryArtifactId?: string;
  fromMessageId?: string;
  fromSeq?: number;
  actorId?: string;
  origin?: string;
};

export function buildRipThreadHandoffArgs(
  fromThreadId: string,
  options: RipThreadHandoffArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "handoff", fromThreadId];
  if (options.title) args.push("--title", options.title);
  if (options.summaryMarkdown) args.push("--summary-markdown", options.summaryMarkdown);
  if (options.summaryArtifactId) args.push("--summary-artifact-id", options.summaryArtifactId);
  if (options.fromMessageId) args.push("--from-message-id", options.fromMessageId);
  if (typeof options.fromSeq === "number") args.push("--from-seq", String(options.fromSeq));
  if (options.actorId) args.push("--actor-id", options.actorId);
  if (options.origin) args.push("--origin", options.origin);
  return args;
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

export type RipThreadCompactionCheckpointArgsOptions = RipThreadsArgsOptions & {
  summaryMarkdown?: string;
  summaryArtifactId?: string;
  toMessageId?: string;
  toSeq?: number;
  strideMessages?: number;
  actorId?: string;
  origin?: string;
};

export function buildRipThreadCompactionCheckpointArgs(
  threadId: string,
  options: RipThreadCompactionCheckpointArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "compaction-checkpoint", threadId];
  if (options.summaryMarkdown) args.push("--summary-markdown", options.summaryMarkdown);
  if (options.summaryArtifactId) args.push("--summary-artifact-id", options.summaryArtifactId);
  if (options.toMessageId) args.push("--to-message-id", options.toMessageId);
  if (typeof options.toSeq === "number") args.push("--to-seq", String(options.toSeq));
  if (typeof options.strideMessages === "number") args.push("--stride-messages", String(options.strideMessages));
  if (options.actorId) args.push("--actor-id", options.actorId);
  if (options.origin) args.push("--origin", options.origin);
  return args;
}

export type RipThreadCompactionCutPointsArgsOptions = RipThreadsArgsOptions & {
  strideMessages?: number;
  limit?: number;
};

export function buildRipThreadCompactionCutPointsArgs(
  threadId: string,
  options: RipThreadCompactionCutPointsArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "compaction-cut-points", threadId];
  if (typeof options.strideMessages === "number") args.push("--stride-messages", String(options.strideMessages));
  if (typeof options.limit === "number") args.push("--limit", String(options.limit));
  return args;
}

export type RipThreadCompactionStatusArgsOptions = RipThreadsArgsOptions & {
  strideMessages?: number;
};

export function buildRipThreadCompactionStatusArgs(
  threadId: string,
  options: RipThreadCompactionStatusArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "compaction-status", threadId];
  if (typeof options.strideMessages === "number") args.push("--stride-messages", String(options.strideMessages));
  return args;
}

export type RipThreadCompactionAutoArgsOptions = RipThreadsArgsOptions & {
  strideMessages?: number;
  maxNewCheckpoints?: number;
  dryRun?: boolean;
  actorId?: string;
  origin?: string;
};

export function buildRipThreadCompactionAutoArgs(
  threadId: string,
  options: RipThreadCompactionAutoArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "compaction-auto", threadId];
  if (typeof options.strideMessages === "number") args.push("--stride-messages", String(options.strideMessages));
  if (typeof options.maxNewCheckpoints === "number") args.push("--max-new-checkpoints", String(options.maxNewCheckpoints));
  if (options.dryRun) args.push("--dry-run");
  if (options.actorId) args.push("--actor-id", options.actorId);
  if (options.origin) args.push("--origin", options.origin);
  return args;
}

export type RipThreadCompactionAutoScheduleArgsOptions = RipThreadsArgsOptions & {
  strideMessages?: number;
  maxNewCheckpoints?: number;
  allowInflight?: boolean;
  noExecute?: boolean;
  dryRun?: boolean;
  actorId?: string;
  origin?: string;
};

export function buildRipThreadCompactionAutoScheduleArgs(
  threadId: string,
  options: RipThreadCompactionAutoScheduleArgsOptions = {},
): string[] {
  const args = [...buildRipThreadsBaseArgs(options), "compaction-auto-schedule", threadId];
  if (typeof options.strideMessages === "number") args.push("--stride-messages", String(options.strideMessages));
  if (typeof options.maxNewCheckpoints === "number") args.push("--max-new-checkpoints", String(options.maxNewCheckpoints));
  if (options.allowInflight) args.push("--allow-inflight");
  if (options.noExecute) args.push("--no-execute");
  if (options.dryRun) args.push("--dry-run");
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
