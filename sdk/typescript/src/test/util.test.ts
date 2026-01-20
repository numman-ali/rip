import assert from "node:assert/strict";
import test from "node:test";

import { buildRipRunArgs, collectOutputText } from "../util.js";

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

test("collectOutputText concatenates output_text_delta", () => {
  const out = collectOutputText([
    { type: "session_started", id: "e1" },
    { type: "output_text_delta", delta: "a" },
    { type: "output_text_delta", delta: "b" },
    { type: "tool_started", name: "ls" },
  ]);
  assert.equal(out, "ab");
});

