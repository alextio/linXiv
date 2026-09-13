/** Backend client. Packaged, the backend is IN-PROCESS: requests go through the
 *  `api` Tauri command (PDFs over linxiv://), so there is no HTTP base. In
 *  browser dev Vite proxies `/api` to the dev shim (D32), which an empty base
 *  lets through. */
import type {
  ApiError as WireApiError,
  ApiRequest,
  RemoteError,
} from "../types/generated";

export const isTauri =
  typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;

// Empty base: only the browser-dev `fetch` path builds a URL, and Vite proxies it.
export const BASE_URL = "";

// Webviews can't send a multipart body through Tauri `invoke`, so file uploads
// travel as a base64 `file_b64` JSON field instead.
export { bytesToBase64 } from "../lib/base64.ts";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string
  ) {
    super(message);
    this.name = "ApiError";
  }
}

// ── Library Backend addressing (CONTEXT.md: Library Backend / Remote Query
// Mode). Every request names a backend: `null` = local in-process, otherwise a
// registered remote node reached via `api_remote`. It is a PARAMETER — this
// module holds no default and reads no UI state; the PoC "default backend"
// lives in stores/backend.ts, whose `libraryFetch` passes it for library
// queries.

/** One registered remote Library Backend. Hand-kept twin of `Backend` in
 *  src-tauri/src/remote_backend.rs; the app crate is out of ts_bindings' reach. */
export interface RemoteBackend {
  id: string;
  label: string;
  node_address: string;
}

/** `null` addresses the local backend explicitly. */
export type BackendRef = RemoteBackend | null;

/** The ONE honest refused-or-offline state: a non-admitted device is refused
 *  indistinguishably from an offline node, by design. */
export const UNREACHABLE_MESSAGE =
  "Can't reach this node — it may be offline, or this device isn't admitted yet. " +
  "Check Settings → Remote backends and send your member code to the node operator.";

/** Maps an `api_remote`/`remote_*` rejection (the generated `RemoteError`
 *  union, tagged by `kind`) to `ApiError`. `transport` and junk both fall
 *  through to the default 500. */
export function mapRemoteError(e: unknown): ApiError {
  const err = e as RemoteError | { kind?: undefined; detail?: string } | null;
  switch (err?.kind) {
    case "unreachable":
      return new ApiError(503, UNREACHABLE_MESSAGE);
    case "remote": // the node's own error envelope — same shape as local errors
      return new ApiError(err.status ?? 500, err.detail ?? "Remote error");
    case "invalid":
      return new ApiError(400, err.detail ?? "Invalid request");
    default:
      return new ApiError(500, err?.detail ?? "Remote request failed");
  }
}

/** The invoke() command + args for a request — pure, so addressing is testable:
 *  local uses `api`, remote `api_remote`. */
export function buildInvoke(
  path: string,
  init: RequestInit | undefined,
  backend: RemoteBackend | null
): { cmd: string; args: Record<string, unknown> } {
  const method = (init?.method ?? "GET").toUpperCase();
  const body =
    typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null;
  const req: ApiRequest = { method, path, body };
  return backend
    ? { cmd: "api_remote", args: { backendId: backend.id, req } }
    : { cmd: "api", args: { req } };
}

// Tauri never reaches here with FormData — uploads send base64 JSON, so the
// FormData branch in apiFetch is the browser-dev path only.
async function invokeApi<T>(
  path: string,
  init: RequestInit | undefined,
  backend: RemoteBackend | null
): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { cmd, args } = buildInvoke(path, init, backend);
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (backend) throw mapRemoteError(e);
    const err = e as Partial<WireApiError>;
    throw new ApiError(err.status ?? 500, err.detail ?? "Request failed");
  }
}

export async function apiFetch<T>(
  path: string,
  init?: RequestInit,
  backend: BackendRef = null
): Promise<T> {
  if (backend) {
    // Remote backends only exist in the desktop app (iroh lives in-process).
    if (!isTauri)
      throw new ApiError(500, "Remote backends require the desktop app");
    if (init?.body instanceof FormData)
      throw new ApiError(400, "Uploads aren't supported on a remote backend");
    return invokeApi<T>(path, init, backend);
  }
  if (isTauri && !(init?.body instanceof FormData)) {
    return invokeApi<T>(path, init, null);
  }
  const url = `${BASE_URL}${path}`;
  const isFormData = init?.body instanceof FormData;
  const response = await fetch(url, {
    ...init,
    headers: isFormData
      ? init?.headers
      : { "Content-Type": "application/json", ...init?.headers },
  });

  if (!response.ok) {
    let detail = `HTTP ${response.status}`;
    try {
      const body = (await response.json()) as { detail?: string };
      if (body.detail) detail = body.detail;
    } catch {
      // ignore parse errors
    }
    throw new ApiError(response.status, detail);
  }

  // 204 No Content or empty body
  const text = await response.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}
