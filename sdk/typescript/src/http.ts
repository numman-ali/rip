export type RipFetch = typeof fetch;

export type RipHttpConfig = {
  server: string;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export type RipHttpRequestOptions = {
  signal?: AbortSignal;
  headers?: Record<string, string>;
  fetch?: RipFetch;
};

export function normalizeServerBaseUrl(server: string): string {
  const trimmed = server.trim();
  if (!trimmed) throw new Error("server is required");
  return trimmed.replace(/\/+$/, "");
}

export function joinUrl(base: string, path: string): string {
  if (!path) return base;
  if (path.startsWith("/")) return `${base}${path}`;
  return `${base}/${path}`;
}

export async function httpJson(
  config: RipHttpConfig,
  path: string,
  init: Omit<RequestInit, "headers" | "signal"> & { headers?: Record<string, string>; signal?: AbortSignal } = {},
): Promise<unknown> {
  const response = await httpRequest(config, path, init);
  const text = await response.text();
  const trimmed = text.trim();
  if (!trimmed) return null;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch (err) {
    throw new Error(`http JSON parse error: ${(err as Error).message}: ${trimmed.slice(0, 200)}`);
  }
}

export async function httpRequest(
  config: RipHttpConfig,
  path: string,
  init: Omit<RequestInit, "headers"> & { headers?: Record<string, string> } = {},
): Promise<Response> {
  const fetchFn = config.fetch ?? globalThis.fetch;
  if (typeof fetchFn !== "function") {
    throw new Error("global fetch is unavailable; provide fetch in Rip options");
  }

  const base = normalizeServerBaseUrl(config.server);
  const url = joinUrl(base, path);
  const headers = new Headers();
  for (const [key, value] of Object.entries(config.headers ?? {})) headers.set(key, value);
  for (const [key, value] of Object.entries(init.headers ?? {})) headers.set(key, value);

  const response = await fetchFn(url, { ...init, headers });
  if (!response.ok) {
    const body = await safeReadText(response);
    throw new Error(`http ${init.method ?? "GET"} ${path} failed: ${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
  }
  return response;
}

async function safeReadText(response: Response): Promise<string> {
  try {
    const text = await response.text();
    return text.trim().slice(0, 300);
  } catch {
    return "";
  }
}

export async function* sseDataMessages(response: Response): AsyncGenerator<string> {
  if (!response.body) return;

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  let eventName: string | null = null;
  let dataLines: string[] = [];

  const flush = () => {
    if (dataLines.length === 0) return null;
    const data = dataLines.join("\n");
    dataLines = [];
    eventName = null;
    return data;
  };

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    while (true) {
      const newlineIdx = buffer.indexOf("\n");
      if (newlineIdx === -1) break;
      let line = buffer.slice(0, newlineIdx);
      buffer = buffer.slice(newlineIdx + 1);
      if (line.endsWith("\r")) line = line.slice(0, -1);

      if (line === "") {
        const data = flush();
        if (data !== null) yield data;
        continue;
      }

      if (line.startsWith(":")) {
        continue;
      }

      const colonIdx = line.indexOf(":");
      const field = colonIdx === -1 ? line : line.slice(0, colonIdx);
      let valueStr = colonIdx === -1 ? "" : line.slice(colonIdx + 1);
      if (valueStr.startsWith(" ")) valueStr = valueStr.slice(1);

      if (field === "event") {
        eventName = valueStr;
        void eventName;
      } else if (field === "data") {
        dataLines.push(valueStr);
      }
    }
  }

  const trailing = flush();
  if (trailing !== null) yield trailing;
}

