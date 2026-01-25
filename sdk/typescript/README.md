# rip-sdk-ts

TypeScript SDK for RIP.

Requirements
- `rip` binary available (PATH), or pass `executablePath`.

Transport
- Default: spawns the local `rip` binary and reads JSONL event frames from stdout (`rip run ... --headless --view raw`).
- Optional remote: pass `server` to run against `rip serve`/`ripd` via `rip run --server <url> ...` and still consume the same frames.
- Optional (ADR-0017): opt-in direct HTTP/SSE transport to a remote control plane for environments that cannot spawn subprocesses.

Repo dev quickstart
```bash
cargo build -p rip-cli
cd sdk/typescript
npm ci
npm run build
node --input-type=module -e 'import { Rip } from "./dist/index.js"; const rip = new Rip({ executablePath: "../../target/debug/rip" }); const turn = await rip.run("Say hello."); console.log(turn.finalOutput);'
```

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

HTTP transport (server-only)
```ts
import { Rip } from "rip-sdk-ts";

const rip = new Rip({ transport: "http", server: "http://localhost:7341" });
const turn = await rip.run("hello");
console.log(turn.finalOutput);
```
