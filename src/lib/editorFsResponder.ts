// ApiFsResponder — the FsResponder backing the embedded editor's on-disk vault.
//
// Each FsOp HostFsRouter routes here (i.e. whenever no host-picked disk folder is
// mounted) is forwarded to /api/editor/vault/<noteId>/fs, which performs it against
// the vault and returns the matching FsResult. The open project's note id is read
// lazily via a getter, so switching projects (re-`sendDocOpen`) needn't rebuild
// the bridge.
//
// Errors: vaultFsOp throws ApiError on a non-2xx response; the bridge's handleFs
// catches it and replies texbrain:fs:result { ok:false, error }, which the guest's
// FsRpc turns into a rejected promise. So throwing here is the contractual error path.

import type { FsResponder } from "./editorBridge";
import type { FsResult } from "./editorBridgeTypes";
import { vaultFsOp } from "../api/editor";

export class ApiFsResponder implements FsResponder {
  /** @param getNoteId resolves the currently-open editor project's note id (null ⇒ none). */
  constructor(private readonly getNoteId: () => number | null) {}

  private noteId(): number {
    const id = this.getNoteId();
    if (id == null) throw new Error("No editor project is open");
    return id;
  }

  async list(path: string): Promise<Extract<FsResult, { kind: "list" }>> {
    return (await vaultFsOp(this.noteId(), { kind: "list", path })) as Extract<
      FsResult,
      { kind: "list" }
    >;
  }

  async readFile(path: string): Promise<Extract<FsResult, { kind: "readFile" }>> {
    return (await vaultFsOp(this.noteId(), { kind: "readFile", path })) as Extract<
      FsResult,
      { kind: "readFile" }
    >;
  }

  async writeFile(
    path: string,
    data: string,
    binary: boolean
  ): Promise<Extract<FsResult, { kind: "ok" }>> {
    return (await vaultFsOp(this.noteId(), {
      kind: "writeFile",
      path,
      data,
      binary,
    })) as Extract<FsResult, { kind: "ok" }>;
  }

  async mkdir(path: string): Promise<Extract<FsResult, { kind: "ok" }>> {
    return (await vaultFsOp(this.noteId(), { kind: "mkdir", path })) as Extract<
      FsResult,
      { kind: "ok" }
    >;
  }

  async remove(
    path: string,
    recursive: boolean
  ): Promise<Extract<FsResult, { kind: "ok" }>> {
    return (await vaultFsOp(this.noteId(), {
      kind: "remove",
      path,
      recursive,
    })) as Extract<FsResult, { kind: "ok" }>;
  }
}
