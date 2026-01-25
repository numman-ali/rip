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

test("Rip SDK exposes continuity-first thread.* via `rip threads`", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-threads-"));

  const opts = {
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
  } as const;

  try {
    const rip = new Rip({ executablePath: ripPath });
    const ensured = await rip.threadEnsure(opts);
    assert.ok(ensured.thread_id.length > 0);

    const list = await rip.threadList(opts);
    assert.ok(list.some((thread) => thread.thread_id === ensured.thread_id));

    const meta = await rip.threadGet(ensured.thread_id, opts);
    assert.equal(meta.thread_id, ensured.thread_id);

    const posted = await rip.threadPostMessage(ensured.thread_id, { content: "hello" }, opts);
    assert.equal(posted.thread_id, ensured.thread_id);
    assert.ok(posted.message_id.length > 0);
    assert.ok(posted.session_id.length > 0);

    const selection = await rip.threadContextSelectionStatus(ensured.thread_id, { limit: 1 }, opts);
    assert.equal(selection.thread_id, ensured.thread_id);
    assert.ok(Array.isArray(selection.decisions));

    const branched = await rip.threadBranch(ensured.thread_id, { title: "child", from_message_id: posted.message_id }, opts);
    assert.equal(branched.parent_thread_id, ensured.thread_id);
    assert.ok(branched.thread_id.length > 0);

    const handed = await rip.threadHandoff(
      ensured.thread_id,
      { title: "handoff", summary_markdown: "summary", from_message_id: posted.message_id },
      opts,
    );
    assert.equal(handed.from_thread_id, ensured.thread_id);
    assert.ok(handed.thread_id.length > 0);

    const { result } = await rip.threadEventsStreamed(ensured.thread_id, opts, { maxEvents: 3 });
    const frames = await result;
    assert.ok(frames.some((frame) => frame.type === "continuity_created"));
    assert.ok(frames.some((frame) => frame.type === "continuity_message_appended"));
    assert.ok(frames.some((frame) => frame.type === "continuity_run_spawned"));

    const { result: branchResult } = await rip.threadEventsStreamed(branched.thread_id, opts, { maxEvents: 2 });
    const branchFrames = await branchResult;
    assert.ok(branchFrames.some((frame) => frame.type === "continuity_created"));
    assert.ok(branchFrames.some((frame) => frame.type === "continuity_branched"));

    const { result: handoffResult } = await rip.threadEventsStreamed(handed.thread_id, opts, { maxEvents: 2 });
    const handoffFrames = await handoffResult;
    assert.ok(handoffFrames.some((frame) => frame.type === "continuity_created"));
    const handoffFrame = handoffFrames.find((frame) => frame.type === "continuity_handoff_created") as
      | Record<string, unknown>
      | undefined;
    assert.ok(handoffFrame);
    assert.equal(handoffFrame.from_thread_id, ensured.thread_id);
    assert.equal(handoffFrame.summary_markdown, "summary");
  } finally {
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("Rip SDK exposes compaction checkpoints via `rip threads`", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-compaction-"));
  const workspaceDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-workspace-"));

  const opts = {
    cwd: repoRoot,
    env: {
      RIP_DATA_DIR: dataDir,
      RIP_WORKSPACE_ROOT: workspaceDir,
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
  } as const;

  try {
    const rip = new Rip({ executablePath: ripPath });
    const ensured = await rip.threadEnsure(opts);
    const posted = await rip.threadPostMessage(ensured.thread_id, { content: "hello" }, opts);
    const checkpoint = await rip.threadCompactionCheckpoint(
      ensured.thread_id,
      { summary_markdown: "summary", to_message_id: posted.message_id },
      opts,
    );
    assert.equal(checkpoint.thread_id, ensured.thread_id);
    assert.ok(checkpoint.checkpoint_id.length > 0);
    assert.ok(checkpoint.summary_artifact_id.length > 0);
    assert.equal(checkpoint.to_message_id, posted.message_id);
  } finally {
    await rm(dataDir, { recursive: true, force: true });
    await rm(workspaceDir, { recursive: true, force: true });
  }
});

test("Rip SDK exposes compaction status via `rip threads`", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-compaction-status-"));
  const workspaceDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-workspace-"));

  const opts = {
    cwd: repoRoot,
    env: {
      RIP_DATA_DIR: dataDir,
      RIP_WORKSPACE_ROOT: workspaceDir,
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
  } as const;

  try {
    const rip = new Rip({ executablePath: ripPath });
    const ensured = await rip.threadEnsure(opts);
    await rip.threadPostMessage(ensured.thread_id, { content: "hello" }, opts);

    const status = await rip.threadCompactionStatus(ensured.thread_id, { stride_messages: 1 }, opts);
    assert.equal(status.thread_id, ensured.thread_id);
    assert.equal(status.message_count, 1);
    assert.equal(status.latest_checkpoint, null);
    assert.ok(status.next_cut_point);
    assert.equal(status.next_cut_point.to_message_id.length > 0, true);
  } finally {
    await rm(dataDir, { recursive: true, force: true });
    await rm(workspaceDir, { recursive: true, force: true });
  }
});
