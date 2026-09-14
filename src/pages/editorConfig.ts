// Where the embedded TeXbrain editor lives: iframe origin and src. The bridge
// itself is src/lib/editorBridge.ts, re-exported below so EditorPage can wire
// everything from a single import.
//
// ADR 0015 (LOCKED): the address derives from `import.meta.env.DEV`, NOT
// `isTauri` — `tauri dev` is a Tauri webview that MUST use the dev address.
//   dev:  http://localhost:5173/editor — the editor's own Vite dev server (the
//         host owns 5180). Cross-origin like prod, no proxy, so origin bugs
//         can't hide until release.
//   prod: <texbrain scheme origin>/editor — the runtime-downloaded Editor
//         plugin served by tauri-plugin-texbrain's scheme (ADR 0017/0016).
// Only the /editor PATH is constant; the origin is environment-derived.

/**
 * Prod origin of a Tauri-registered scheme. PLATFORM-DEPENDENT (verified
 * against tauri 2.11.2 `plugin.rs`): macOS/iOS/Linux resolve it to
 * `<scheme>://localhost`, Windows (WebView2) to `http://<scheme>.localhost`.
 * The wrong form silently rejects every guest message and the handshake never
 * completes, so it MUST match the iframe's real origin. UA sniffing is
 * dependable inside the Tauri webview (WebView2 always reports a Windows UA).
 */
export function schemeOrigin(scheme: string): string {
  const isWindows =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
  return isWindows ? `http://${scheme}.localhost` : `${scheme}://localhost`;
}

/**
 * Origin of the embedded TeXbrain editor, pinned both ways: the postMessage
 * targetOrigin (host -> guest) and the only event.origin EditorBridgeClient
 * accepts (guest -> host). Never '*' — the bridge is first-party app-to-app.
 */
export const EDITOR_ORIGIN: string = import.meta.env.DEV
  ? "http://localhost:5173"
  : schemeOrigin("texbrain");

/**
 * The iframe `src`; only the `/editor` path is constant (ADR 0015). `?host=`
 * carries the host's own origin, so the guest knows where to post
 * `texbrain:ready` and which origin to accept. Essential in prod: the host page
 * is a custom scheme and the referrer to a `texbrain://` iframe is stripped, so
 * the guest's old referrer-derived origin was wrong and the bridge silently
 * never handed off.
 */
export const EDITOR_SRC: string = `${EDITOR_ORIGIN}/editor?host=${encodeURIComponent(
  typeof window !== "undefined" ? window.location.origin : ""
)}`;

// Re-export the bridge so EditorPage can wire everything from a single import.
export {
  EditorBridgeClient,
  NoopFsResponder,
  pushThemeToEditor,
} from "../lib/editorBridge";
export type {
  ThemePushState,
  FsResponder,
  EditorBridgeHandlers,
  DocOpenPayload,
} from "../lib/editorBridge";
