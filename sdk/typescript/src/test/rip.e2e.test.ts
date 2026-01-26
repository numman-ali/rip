import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
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

async function killAuthority(dataDir: string): Promise<void> {
  const metaPath = path.join(dataDir, "authority", "meta.json");
  let raw: string;
  try {
    raw = await readFile(metaPath, "utf8");
  } catch {
    return;
  }
  let meta: { pid?: number } | null = null;
  try {
    meta = JSON.parse(raw) as { pid?: number };
  } catch {
    return;
  }
  if (!meta || typeof meta.pid !== "number") return;
  try {
    process.kill(meta.pid, "SIGTERM");
  } catch {
    return;
  }
}

async function cleanupDataDir(dataDir: string): Promise<void> {
  await killAuthority(dataDir);
  await rm(dataDir, { recursive: true, force: true });
}

type AuthorityMeta = { endpoint?: unknown; pid?: unknown };

async function waitForAuthorityEndpoint(
  dataDir: string,
  child: ReturnType<typeof spawn> | null,
  stderrSnapshot: () => string,
  timeoutMs = 10_000,
  childError: () => unknown | null = () => null,
): Promise<string> {
  const metaPath = path.join(dataDir, "authority", "meta.json");
  const deadline = Date.now() + timeoutMs;
  let lastErr: unknown | null = null;

  while (Date.now() < deadline) {
    const err = childError();
    if (err) {
      const detail = err instanceof Error ? err.message : String(err);
      throw new Error(`rip serve failed to start: ${detail}\n${stderrSnapshot()}`);
    }
    if (child && child.exitCode !== null) {
      throw new Error(`rip serve exited early: exitCode=${child.exitCode}\n${stderrSnapshot()}`);
    }

    try {
      const raw = await readFile(metaPath, "utf8");
      const meta = JSON.parse(raw) as AuthorityMeta;
      if (meta && typeof meta.endpoint === "string" && meta.endpoint.trim()) return meta.endpoint.trim();
    } catch (err) {
      lastErr = err;
    }

    await sleep(50);
  }

  const detail = lastErr instanceof Error ? lastErr.message : String(lastErr ?? "");
  throw new Error(`timed out waiting for authority meta.json endpoint at ${metaPath}${detail ? `: ${detail}` : ""}\n${stderrSnapshot()}`);
}

async function waitForServerReady(endpoint: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${endpoint}/openapi.json`);
      if (res.ok) return;
    } catch {
      // ignore
    }
    await sleep(50);
  }
  throw new Error(`timed out waiting for server ready: ${endpoint}`);
}

async function stopRipServer(child: ReturnType<typeof spawn>): Promise<void> {
  if (child.exitCode !== null) return;

  const killAndWait = async (signal: NodeJS.Signals, timeoutMs: number) => {
    try {
      child.kill(signal);
    } catch {
      try {
        child.kill();
      } catch {
        return;
      }
    }

    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline && child.exitCode === null) {
      await sleep(25);
    }
  };

  await killAndWait("SIGTERM", 2_000);
  if (child.exitCode !== null) return;
  await killAndWait("SIGKILL", 2_000);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
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
    await cleanupDataDir(dataDir);
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
    await cleanupDataDir(dataDir);
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
    await cleanupDataDir(dataDir);
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
    await cleanupDataDir(dataDir);
    await rm(workspaceDir, { recursive: true, force: true });
  }
});

test("Rip SDK exposes task.* locally without server", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-tasks-"));

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
    const created = await rip.taskSpawn({ tool: "bash", args: { command: "sleep 30" }, title: "sdk-e2e" }, opts);
    assert.ok(created.task_id.length > 0);

    const status = await rip.taskStatus(created.task_id, opts);
    assert.equal(status.task_id, created.task_id);

    const list = await rip.taskList(opts);
    assert.ok(list.some((task) => task.task_id === created.task_id));

    const output = await rip.taskOutput(created.task_id, opts);
    assert.equal(output.task_id, created.task_id);

    await rip.taskCancel(created.task_id, opts, "sdk-e2e-cancel");

    const deadline = Date.now() + 10_000;
    let terminal = await rip.taskStatus(created.task_id, opts);
    while (Date.now() < deadline && (terminal.status === "queued" || terminal.status === "running")) {
      await sleep(50);
      terminal = await rip.taskStatus(created.task_id, opts);
    }

    assert.equal(terminal.task_id, created.task_id);
    assert.ok(["cancelled", "exited", "failed"].includes(terminal.status));
  } finally {
    await cleanupDataDir(dataDir);
  }
});

test("Rip SDK streams task events locally without server", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-task-events-"));

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
    const created = await rip.taskSpawn(
      { tool: "bash", args: { command: "sleep 1; echo done" }, title: "sdk-e2e-events" },
      opts,
    );
    assert.ok(created.task_id.length > 0);

    const terminalStatuses = new Set(["exited", "cancelled", "failed"]);
    const { events, result } = await rip.taskEventsStreamed(created.task_id, opts);
    let sawTerminal = false;

    for await (const frame of events) {
      if (frame.type !== "tool_task_status") continue;
      const status = (frame as { status?: unknown }).status;
      if (typeof status === "string" && terminalStatuses.has(status)) {
        sawTerminal = true;
        break;
      }
    }

    const frames = await result;
    const terminalFrame = frames.find((frame) => {
      if (frame.type !== "tool_task_status") return false;
      const status = (frame as { status?: unknown }).status;
      return typeof status === "string" && terminalStatuses.has(status);
    });
    assert.ok(terminalFrame);
    assert.ok(sawTerminal);
  } finally {
    await cleanupDataDir(dataDir);
  }
});

test("Rip SDK streams task events over HTTP transport until terminal status", async () => {
  const repoRoot = repoRootFromSdkCwd();
  const ripPath = ripExecutablePath(repoRoot);
  const dataDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-http-task-events-"));
  const workspaceDir = await mkdtemp(path.join(os.tmpdir(), "rip-sdk-http-workspace-"));

  const env = { ...process.env };
  env.RIP_DATA_DIR = dataDir;
  env.RIP_WORKSPACE_ROOT = workspaceDir;
  env.RIP_SERVER_ADDR = "127.0.0.1:0";
  for (const key of [
    "RIP_OPENRESPONSES_ENDPOINT",
    "RIP_OPENRESPONSES_API_KEY",
    "RIP_OPENRESPONSES_MODEL",
    "RIP_OPENRESPONSES_TOOL_CHOICE",
    "RIP_OPENRESPONSES_STATELESS_HISTORY",
    "RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS",
    "RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE",
  ]) {
    delete env[key];
  }

  let stderr = "";
  let childError: unknown | null = null;
  const child = spawn(ripPath, ["serve"], { cwd: repoRoot, env, stdio: ["ignore", "ignore", "pipe"] });
  child.once("error", (err) => {
    childError = err;
  });
  child.stderr?.setEncoding("utf8");
  child.stderr?.on("data", (chunk) => {
    stderr += chunk;
    if (stderr.length > 20_000) stderr = stderr.slice(-20_000);
  });
  const stderrSnapshot = () => stderr.trim();

  try {
    const endpoint = await waitForAuthorityEndpoint(dataDir, child, stderrSnapshot, 10_000, () => childError);
    await waitForServerReady(endpoint);

    const rip = new Rip({ transport: "http", server: endpoint });
    const created = await rip.taskSpawn({ tool: "bash", args: { command: "sleep 0.2; echo done" }, title: "sdk-e2e-http-events" }, {});
    assert.ok(created.task_id.length > 0);

    const terminalStatuses = new Set(["exited", "cancelled", "failed"]);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 10_000);

    try {
      const { events, result } = await rip.taskEventsStreamed(created.task_id, { signal: controller.signal });

      let sawTerminal = false;
      for await (const frame of events) {
        if (frame.type !== "tool_task_status") continue;
        const status = (frame as { status?: unknown }).status;
        if (typeof status === "string" && terminalStatuses.has(status)) {
          sawTerminal = true;
        }
      }

      const frames = await result;
      const terminalFrame = frames.find((frame) => {
        if (frame.type !== "tool_task_status") return false;
        const status = (frame as { status?: unknown }).status;
        return typeof status === "string" && terminalStatuses.has(status);
      });

      assert.ok(terminalFrame);
      assert.ok(sawTerminal);
    } finally {
      clearTimeout(timer);
    }
  } finally {
    await stopRipServer(child);
    await cleanupDataDir(dataDir);
    await rm(workspaceDir, { recursive: true, force: true });
  }
});
