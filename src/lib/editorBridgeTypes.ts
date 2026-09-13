// Vendored wire-protocol + theme-bridge types for the embedded TeXbrain editor.
//
// These mirror tex-brain's src/lib/embed/contract.ts (the source of truth) so the
// linXiv host compiles WITHOUT a cross-repo import. Every message / FsOp /
// FsResult SHAPE must stay structurally identical to it; formatting follows this
// repo, and nothing enforces the sync, so shape changes are hand-mirrored both
// ways. ThemeColors / ThemeMode are instead re-exported from ./theme, which
// tex-brain's theme/presets.ts was ported from.
//
// Wire protocol over window.postMessage(JSON.stringify(msg), targetOrigin).
// Guest = embedded TeXbrain editor; Host = linXiv React shell, authoritative.
// All types are prefixed 'texbrain:'. Pin targetOrigin/event.origin, never '*'.

import type { ThemeColors, ThemeMode } from "./theme";
import type { FsOp, FsResult } from "../types/generated";

// Re-export so bridge consumers get the palette types from here, not ./theme.
export type { ThemeColors, ThemeMode };

// ---- FS-adapter RPC payloads ---------------------------------------
// `op` mirrors the HostFsAdapter (FsDirHandle / FsFileHandle) surface the guest
// consumes; the host resolves each op against linXiv's /api (or disk). FsOp /
// FsResult are generated from crates/core/src/service/vault.rs and re-exported
// here so the bridge contract stays in one place.

export type { FsOp, FsResult };

// ---- Guest -> Host -------------------------------------------------

export type GuestToHost =
  // Posted once on iframe mount; host replies with texbrain:theme + (optionally) doc:open.
  | { type: "texbrain:ready"; protocol: 1 }
  // Compile finished (or failed); pdf transferred as base64 (~1.33x raw size, vs
  // ~3.5x for a number[] through JSON), status 0 = first-pass clean.
  | { type: "texbrain:compiled"; status: number; log: string; pdf: string | null }
  // Editor buffer changed (dirty tracking on the host, optional).
  | { type: "texbrain:dirty"; dirty: boolean }
  // FS-adapter RPC: guest asks host to perform an FsDirHandle/FsFileHandle op.
  | { type: "texbrain:fs"; id: string; op: FsOp }
  // Ask the host to show its NATIVE directory picker and re-root the fs RPC at
  // the picked folder (the embed's "Open Folder": the iframe has no Tauri IPC
  // and WebKitGTK no File System Access API, so only the host can pick).
  // Additive within protocol 1 — an older host never replies, so a supporting
  // host posts texbrain:pick:folder:ack IMMEDIATELY on receipt and the guest
  // treats a missing ack (short window) as "unsupported" rather than hanging.
  | { type: "texbrain:pick:folder"; id: string };

// ---- Host -> Guest -------------------------------------------------

export type HostToGuest =
  // Push the active document/project; `files` is reserved, always {}.
  | { type: "texbrain:doc:open"; projectId: number; mainFile: string; files: Record<string, string>; projectName: string }
  // Ask the guest to compile the current buffers now.
  | { type: "texbrain:compile" }
  // Resolve a pending texbrain:fs request.
  | { type: "texbrain:fs:result"; id: string; ok: true; value: FsResult }
  | { type: "texbrain:fs:result"; id: string; ok: false; error: string }
  // Receipt ack for texbrain:pick:folder, posted BEFORE the long-lived native
  // dialog opens: lets the guest tell "dialog still open" from "host will never
  // reply" without capping how long the dialog may stay open.
  | { type: "texbrain:pick:folder:ack"; id: string }
  // Resolve a pending texbrain:pick:folder. `name` is the picked folder's
  // basename (the guest mounts it as projectName); null = user cancelled.
  // After ok+name, every subsequent texbrain:fs op resolves against the picked
  // disk folder until the next doc:open re-roots the responder at the vault.
  | { type: "texbrain:pick:folder:result"; id: string; ok: true; name: string | null }
  | { type: "texbrain:pick:folder:result"; id: string; ok: false; error: string }
  // Theme push (resolved 8 colors; see ThemeBridge contract).
  | { type: "texbrain:theme"; mode: ThemeMode; colors: ThemeColors };

export type EditorMessage = GuestToHost | HostToGuest;

// Lifecycle: mount iframe -> guest posts {ready} -> host replies {theme} then {doc:open}
// -> on demand host posts {compile}; guest replies {compiled, pdf}. FS ops are
// request/response correlated by `id`.
