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

export function collectOutputText(frames: readonly RipEventFrame[]): string {
  let out = "";
  for (const frame of frames) {
    if (frame.type !== "output_text_delta") continue;
    const delta = frame.delta;
    if (typeof delta === "string") out += delta;
  }
  return out;
}

