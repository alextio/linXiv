import { useEffect } from "react";
import { useUiStore } from "../stores/ui.ts";
import { useShortcutsStore, type ShortcutOverride } from "../stores/shortcuts.ts";
import { ZOOM_STEP, DEFAULT_ZOOM } from "./zoom.ts";

// Central inventory of the app's keyboard shortcuts, and the single source of
// truth: the Settings "Shortcuts" view renders it, useGlobalShortcuts() binds
// the window-level ones. Element-scoped chords (form submit) live with their
// components and are listed here for display only (no `run`), since a global
// listener can't know which form is focused — so only shortcuts with `run` are
// dispatched, and only those are rebindable. An override lives in
// useShortcutsStore by shortcut id; effectiveMatch() prefers it over `match`.

export type { ShortcutOverride };

export type ShortcutScope = "global" | "form";


export interface Shortcut {
  id: string;
  /** Key tokens rendered as separate <kbd> chips, e.g. ["Ctrl/Cmd", "+"]. */
  keys: string[];
  description: string;
  scope: ShortcutScope;
  /** Present only for window-bound shortcuts dispatched by useGlobalShortcuts. */
  match?: (e: KeyboardEvent) => boolean;
  run?: () => void;
}

// Ctrl (Win/Linux) or Cmd (macOS), but not Alt — the zoom modifier.
const zoomMod = (e: KeyboardEvent) => (e.ctrlKey || e.metaKey) && !e.altKey;

export const SHORTCUTS: Shortcut[] = [
  {
    id: "zoom-in",
    keys: ["Ctrl/Cmd", "+"],
    description: "Zoom the interface in",
    scope: "global",
    match: (e) => zoomMod(e) && (e.key === "+" || e.key === "="),
    run: () => {
      const { zoom, setZoom } = useUiStore.getState();
      setZoom(zoom + ZOOM_STEP);
    },
  },
  {
    id: "zoom-out",
    keys: ["Ctrl/Cmd", "-"],
    description: "Zoom the interface out",
    scope: "global",
    match: (e) => zoomMod(e) && (e.key === "-" || e.key === "_"),
    run: () => {
      const { zoom, setZoom } = useUiStore.getState();
      setZoom(zoom - ZOOM_STEP);
    },
  },
  {
    id: "zoom-reset",
    keys: ["Ctrl/Cmd", "0"],
    description: "Reset the interface zoom to 100%",
    scope: "global",
    match: (e) => zoomMod(e) && e.key === "0",
    run: () => useUiStore.getState().setZoom(DEFAULT_ZOOM),
  },
  {
    id: "submit",
    keys: ["Ctrl/Cmd", "Enter"],
    description: "Submit the focused form or dialog",
    scope: "form",
    // No `run` — dispatch is element-scoped (src/lib/submitShortcut.ts); the
    // `match` remains so findConflict() can warn a rebind off this chord.
    match: (e) => (e.ctrlKey || e.metaKey) && !e.altKey && e.key === "Enter",
  },
];

/** Binds one window keydown listener that dispatches the global shortcuts.
 *  preventDefault stops the webview's native zoom compounding with ours; the
 *  store setters already clamp + persist. Mount once, near the app root. */
export function useGlobalShortcuts(): void {
  useEffect(() => {
    // Combos whose non-repeat keydown this listener actually saw. Holding a
    // shortcut should keep firing it (continuous zoom), so repeats pass unless
    // the combo never started here — exactly the case right after a rebind:
    // the capture UI stops propagation then unmounts, so a still-held key
    // reaches us as an auto-repeat for a combo we never opened, and would fire
    // the just-bound action the instant it's saved.
    const openCombos = new Set<string>();
    function onKeyDown(e: KeyboardEvent) {
      const combo = comboKey(captureOverride(e));
      if (e.repeat) {
        if (!openCombos.has(combo)) return;
      } else {
        openCombos.add(combo);
      }
      const { overrides } = useShortcutsStore.getState();
      for (const s of SHORTCUTS) {
        if (s.run && effectiveMatch(s, overrides)?.(e)) {
          e.preventDefault();
          s.run();
          return;
        }
      }
    }
    function onKeyUp(e: KeyboardEvent) {
      openCombos.delete(comboKey(captureOverride(e)));
    }
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, []);
}

// --- Rebinding ---------------------------------------------------------

/** Normalizes an override to a comparable string; the whole matching scheme
 * is just equality on this. */
function comboKey(o: ShortcutOverride): string {
  return `${o.ctrl ? 1 : 0}${o.alt ? 1 : 0}${o.shift ? 1 : 0}:${o.key.toLowerCase()}`;
}

/** Turns a captured keydown into an override. Ctrl and Cmd are treated as
 * the same modifier, matching how the built-in shortcuts already work. */
export function captureOverride(e: KeyboardEvent): ShortcutOverride {
  return { ctrl: e.ctrlKey || e.metaKey, alt: e.altKey, shift: e.shiftKey, key: e.key };
}

export function matchesOverride(e: KeyboardEvent, o: ShortcutOverride): boolean {
  return comboKey(captureOverride(e)) === comboKey(o);
}

/** <kbd> chips for a rebound combo, in the same style as the static `keys`. */
export function describeOverride(o: ShortcutOverride): string[] {
  const parts: string[] = [];
  if (o.ctrl) parts.push("Ctrl/Cmd");
  if (o.alt) parts.push("Alt");
  if (o.shift) parts.push("Shift");
  parts.push(o.key.length === 1 ? o.key.toUpperCase() : o.key);
  return parts;
}

/** A shortcut's live match predicate: the user's override if one is set,
 * otherwise its default. */
export function effectiveMatch(
  s: Shortcut,
  overrides: Record<string, ShortcutOverride>
): ((e: KeyboardEvent) => boolean) | undefined {
  const o = overrides[s.id];
  return o ? (e) => matchesOverride(e, o) : s.match;
}

/** Another shortcut whose current binding (default or overridden) would also
 * fire for `e`, including element-scoped ones like `submit` that match but
 * never run — so a rebind UI can warn instead of quietly sharing a key. */
export function findConflict(
  excludeId: string,
  e: KeyboardEvent,
  overrides: Record<string, ShortcutOverride>
): Shortcut | undefined {
  return SHORTCUTS.find(
    (s) => s.id !== excludeId && effectiveMatch(s, overrides)?.(e)
  );
}

/** A rebind must carry a real modifier (Ctrl/Cmd/Alt), never a bare printable
 * key or Shift+key: useGlobalShortcuts listens on window with no input/textarea
 * exclusion, so either would swallow ordinary typing app-wide. */
export function hasBindableModifier(e: KeyboardEvent): boolean {
  return e.ctrlKey || e.metaKey || e.altKey;
}

