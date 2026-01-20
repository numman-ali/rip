import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { Rip } from "../index.js";

function repoRootFromSdkCwd(): string {
  // `scripts/check-sdk-ts` runs tests from `sdk/typescript`.
  return path.resolve(process.cwd(), "../..");
}

function ripExecutablePath(repoRoot: string): string {
  if (process.env.RIP_SDK_TEST_RIP) {
    return process.env.RIP_SDK_TEST_RIP;
  }

  const exe = process.platform === "win32" ? "rip.exe" : "rip";
  return path.join(repoRoot, "target", "debug", exe);
}

test("Rip SDK runs `rip` locally and parses JSONL frames", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-e2e-"));

  try {
    const rip = new Rip({ executablePath: ripPath });
    const turn = await rip.run("hello", {
      cwd: repoRoot,
      env: {
        RIP_DATA_DIR: dataDir,
        RIP_WORKSPACE_ROOT: path.join(repoRoot, "fixtures", "repo_small"),
      },
      unsetEnv: [
        "RIP_OPENRESPONSES_ENDPOINT",
        "RIP_OPENRESPONSES_API_KEY",
        "RIP_OPENRESPONSES_MODEL",
        "RIP_OPENRESPONSES_TOOL_CHOICE",
        "RIP_OPENRESPONSES_STATELESS_HISTORY",
        "RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS",
        "RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE",
      ],
    });

    assert.equal(turn.exitCode, 0);
    assert.equal(turn.finalOutput, "ack: hello");
    assert.ok(turn.frames.some((frame) => frame.type === "session_started"));
    assert.ok(turn.frames.some((frame) => frame.type === "output_text_delta"));
    assert.ok(turn.frames.some((frame) => frame.type === "session_ended"));
  } finally {
    await rm(dataDir, { recursive: true, force: true });
  }
});
