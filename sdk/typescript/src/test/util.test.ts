import assert from "node:assert/strict";
import test from "node:test";

import {
  buildRipRunArgs,
  buildRipThreadEnsureArgs,
  buildRipThreadEventsArgs,
  buildRipThreadGetArgs,
  buildRipThreadListArgs,
  buildRipThreadPostMessageArgs,
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

test("buildRipThreadPostMessageArgs includes provenance flags", () => {
  const args = buildRipThreadPostMessageArgs("t1", "hi", { actorId: "alice", origin: "sdk-ts" });
  assert.deepEqual(args, ["threads", "post-message", "t1", "--content", "hi", "--actor-id", "alice", "--origin", "sdk-ts"]);
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
