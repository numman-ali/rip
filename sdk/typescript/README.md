# rip-sdk-ts

TypeScript SDK for RIP.

Transport
- Default: spawns the local `rip` binary and reads JSONL event frames from stdout (`rip run ... --headless --view raw`).
- Optional remote: pass `server` to run against `rip serve`/`ripd` via `rip run --server <url> ...` and still consume the same frames.

Quickstart
```ts
import { Rip } from "rip-sdk-ts";

const rip = new Rip();
const turn = await rip.run("List what's in this directory. Use the ls tool, then answer with just the filenames.");

console.log(turn.finalOutput);
```

Streaming
```ts
import { Rip } from "rip-sdk-ts";

const rip = new Rip();
const { events, result } = await rip.runStreamed("Say hello, then use ls.");

for await (const ev of events) {
  if (ev.type === "output_text_delta") process.stdout.write(ev.delta);
}

const turn = await result;
console.log("\nDONE:", turn.exitCode);
```
