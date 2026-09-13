import { isTauri } from "../api/client.ts";

// Interface zoom bounds. 100% is the unscaled baseline; the range mirrors
// browser Ctrl +/− and what WebView2/WebKitGTK/WKWebView handle reliably.
export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 2.0;
export const ZOOM_STEP = 0.1;
export const DEFAULT_ZOOM = 1.0;

/**
 * Clamp to the supported range and round to whole percentage points: repeated
 * ±0.1 steps accumulate float error (0.5 stepped up 5× → 0.99999…), which
 * would otherwise break `zoom === DEFAULT_ZOOM`.
 */
export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return DEFAULT_ZOOM;
  const rounded = Math.round(zoom * 100) / 100;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, rounded));
}

/** WebKitGTK's native zoom breaks text-selection hit-testing (a drag anchors
 *  ~zoom× away from the cursor), so Linux desktop takes the CSS path too. */
const nativeZoom = isTauri && !navigator.userAgent.includes("Linux");

/**
 * Apply the zoom factor to the whole interface.
 *
 * Mac/Windows desktop use the webview's native zoom — the only mechanism that
 * also scales the px-based icons and inline sizes scattered through the UI.
 * It resets to 100% each launch, so callers re-apply the persisted value on
 * boot. Linux desktop and the browser dev server use the CSS `zoom` property.
 * Only one is ever active, so the two never compound.
 */
export function applyZoom(zoom: number): void {
  const factor = clampZoom(zoom);
  if (nativeZoom) {
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(factor))
      .catch((err) => console.error("Failed to set webview zoom", err));
  } else {
    document.documentElement.style.setProperty("zoom", String(factor));
  }
}

/**
 * Canvas backing resolution for pdf.js pages. Native zoom raises the DPR the
 * page sees, so canvases re-rasterize crisply for free; CSS zoom scales the
 * finished raster instead, so the backing store must grow by the same factor.
 * Capped so backing area (∝ scale²) can't blow WebKit's canvas byte limit on
 * HiDPI × max-zoom — text softens past the cap rather than crashing the page.
 */
const MAX_CANVAS_DPR = 3;

export function pdfCanvasDpr(zoom: number): number {
  const dpr = window.devicePixelRatio || 1;
  return nativeZoom ? dpr : Math.min(MAX_CANVAS_DPR, dpr * clampZoom(zoom));
}
