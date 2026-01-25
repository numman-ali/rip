import assert from "node:assert/strict";
import test from "node:test";

import {
  buildRipRunArgs,
  buildRipThreadEnsureArgs,
  buildRipThreadBranchArgs,
  buildRipThreadHandoffArgs,
  buildRipThreadEventsArgs,
  buildRipThreadGetArgs,
  buildRipThreadListArgs,
  buildRipThreadPostMessageArgs,
  buildRipThreadCompactionCheckpointArgs,
  buildRipThreadCompactionCutPointsArgs,
  buildRipThreadCompactionStatusArgs,
  buildRipThreadCompactionAutoArgs,
  buildRipThreadProviderCursorStatusArgs,
  buildRipThreadProviderCursorRotateArgs,
  collectOutputText,
} from "../util.js";

test("buildRipRunArgs includes headless raw view", () => {
  const args = buildRipRunArgs("hello");
  assert.deepEqual(args, ["run", "hello", "--headless", "true", "--view", "raw"]);
});

test("buildRipRunArgs includes --server when provided", () => {
  const args = buildRipRunArgs("hello", { server: "http://127.0.0.1:7341" });
  assert.deepEqual(args, [
    "run",
    "hello",
    "--headless",
    "true",
    "--view",
    "raw",
    "--server",
    "http://127.0.0.1:7341",
  ]);
});

test("buildRipThreadEnsureArgs includes subcommand", () => {
  const args = buildRipThreadEnsureArgs();
  assert.deepEqual(args, ["threads", "ensure"]);
});

test("buildRipThreadEnsureArgs includes --server when provided", () => {
  const args = buildRipThreadEnsureArgs({ server: "http://127.0.0.1:7341" });
  assert.deepEqual(args, ["threads", "--server", "http://127.0.0.1:7341", "ensure"]);
});

test("buildRipThreadListArgs builds list command", () => {
  const args = buildRipThreadListArgs();
  assert.deepEqual(args, ["threads", "list"]);
});

test("buildRipThreadGetArgs builds get command", () => {
  const args = buildRipThreadGetArgs("t1");
  assert.deepEqual(args, ["threads", "get", "t1"]);
});

test("buildRipThreadBranchArgs includes selectors and provenance flags", () => {
  const args = buildRipThreadBranchArgs("t1", {
    title: "child",
    fromMessageId: "m1",
    fromSeq: 3,
    actorId: "alice",
    origin: "sdk-ts",
  });
  assert.deepEqual(args, [
    "threads",
    "branch",
    "t1",
    "--title",
    "child",
    "--from-message-id",
    "m1",
    "--from-seq",
    "3",
    "--actor-id",
    "alice",
    "--origin",
    "sdk-ts",
  ]);
});

test("buildRipThreadHandoffArgs includes summary, selectors, and provenance flags", () => {
  const args = buildRipThreadHandoffArgs("t1", {
    title: "handoff",
    summaryMarkdown: "summary",
    summaryArtifactId: "a1",
    fromMessageId: "m1",
    fromSeq: 3,
    actorId: "alice",
    origin: "sdk-ts",
  });
  assert.deepEqual(args, [
    "threads",
    "handoff",
    "t1",
    "--title",
    "handoff",
    "--summary-markdown",
    "summary",
    "--summary-artifact-id",
    "a1",
    "--from-message-id",
    "m1",
    "--from-seq",
    "3",
    "--actor-id",
    "alice",
    "--origin",
    "sdk-ts",
  ]);
});

test("buildRipThreadPostMessageArgs includes provenance flags", () => {
  const args = buildRipThreadPostMessageArgs("t1", "hi", { actorId: "alice", origin: "sdk-ts" });
  assert.deepEqual(args, ["threads", "post-message", "t1", "--content", "hi", "--actor-id", "alice", "--origin", "sdk-ts"]);
});

test("buildRipThreadCompactionCheckpointArgs includes cut points and provenance flags", () => {
  const args = buildRipThreadCompactionCheckpointArgs("t1", {
    summaryMarkdown: "summary",
    summaryArtifactId: "a1",
    toMessageId: "m1",
    toSeq: 4,
    strideMessages: 10000,
    actorId: "alice",
    origin: "sdk-ts",
  });
  assert.deepEqual(args, [
    "threads",
    "compaction-checkpoint",
    "t1",
    "--summary-markdown",
    "summary",
    "--summary-artifact-id",
    "a1",
    "--to-message-id",
    "m1",
    "--to-seq",
    "4",
    "--stride-messages",
    "10000",
    "--actor-id",
    "alice",
    "--origin",
    "sdk-ts",
  ]);
});

test("buildRipThreadCompactionCutPointsArgs includes stride and limit", () => {
  const args = buildRipThreadCompactionCutPointsArgs("t1", { strideMessages: 10000, limit: 3, server: "http://127.0.0.1:7341" });
  assert.deepEqual(args, [
    "threads",
    "--server",
    "http://127.0.0.1:7341",
    "compaction-cut-points",
    "t1",
    "--stride-messages",
    "10000",
    "--limit",
    "3",
  ]);
});

test("buildRipThreadCompactionStatusArgs includes stride", () => {
  const args = buildRipThreadCompactionStatusArgs("t1", { strideMessages: 10000, server: "http://127.0.0.1:7341" });
  assert.deepEqual(args, [
    "threads",
    "--server",
    "http://127.0.0.1:7341",
    "compaction-status",
    "t1",
    "--stride-messages",
    "10000",
  ]);
});

test("buildRipThreadProviderCursorStatusArgs builds status command", () => {
  const args = buildRipThreadProviderCursorStatusArgs("t1", { server: "http://127.0.0.1:7341" });
  assert.deepEqual(args, [
    "threads",
    "--server",
    "http://127.0.0.1:7341",
    "provider-cursor-status",
    "t1",
  ]);
});

test("buildRipThreadProviderCursorRotateArgs includes provenance and reason flags", () => {
  const args = buildRipThreadProviderCursorRotateArgs("t1", {
    reason: "reset",
    actorId: "alice",
    origin: "sdk-ts",
  });
  assert.deepEqual(args, ["threads", "provider-cursor-rotate", "t1", "--reason", "reset", "--actor-id", "alice", "--origin", "sdk-ts"]);
});

test("buildRipThreadCompactionAutoArgs includes dry-run and limits", () => {
  const args = buildRipThreadCompactionAutoArgs("t1", {
    strideMessages: 10000,
    maxNewCheckpoints: 2,
    dryRun: true,
    actorId: "alice",
    origin: "sdk-ts",
  });
  assert.deepEqual(args, [
    "threads",
    "compaction-auto",
    "t1",
    "--stride-messages",
    "10000",
    "--max-new-checkpoints",
    "2",
    "--dry-run",
    "--actor-id",
    "alice",
    "--origin",
    "sdk-ts",
  ]);
});

test("buildRipThreadEventsArgs includes max-events when provided", () => {
  const args = buildRipThreadEventsArgs("t1", { maxEvents: 3 });
  assert.deepEqual(args, ["threads", "events", "t1", "--max-events", "3"]);
});

test("collectOutputText concatenates output_text_delta", () => {
  const out = collectOutputText([
    { type: "session_started", id: "e1" },
    { type: "output_text_delta", delta: "a" },
    { type: "output_text_delta", delta: "b" },
    { type: "tool_started", name: "ls" },
  ]);
  assert.equal(out, "ab");
});
