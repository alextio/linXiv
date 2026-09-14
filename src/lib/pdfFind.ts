// Pure text-search logic for the PDF find bar. A page's text items (from
// pdf.js getTextContent, default options) are joined without separators into
// one string; match offsets in that string map back onto react-pdf's
// customTextRenderer items via PageIndex.starts, since both sides iterate the
// same item list in the same order.

/** Window event the find shortcuts dispatch; PdfReader listens and opens its bar. */
export const PDF_FIND_EVENT = "linxiv:pdf-find";

export interface PageIndex {
  /** All of the page's text items joined in order. */
  text: string;
  /** starts[i] = offset of item i's first char in `text`. */
  starts: number[];
}

/** One hit, with offsets into that page's PageIndex.text. */
export interface FindMatch {
  page: number;
  start: number;
  end: number;
}

/** A highlight span with a current-match flag; offsets are page-relative in
 * rangesForItem's input and item-relative in its output. */
export interface HighlightRange {
  start: number;
  end: number;
  current: boolean;
}

/** Per-char lowercase that never changes string length, so offsets found in
 * the folded text are valid in the original. The rare chars whose lowercase
 * grows (e.g. "İ") are left as-is instead. */
export function foldCase(s: string): string {
  let out = "";
  for (const ch of s) {
    const lower = ch.toLowerCase();
    out += lower.length === ch.length ? lower : ch;
  }
  return out;
}

export function buildPageIndex(items: string[]): PageIndex {
  const starts: number[] = [];
  let text = "";
  for (const s of items) {
    starts.push(text.length);
    text += s;
  }
  return { text, starts };
}

/** Case-insensitive, non-overlapping matches across all pages, in reading
 * order. Items join with no separator so a query can span item boundaries
 * (PDF text often splits words mid-line). */
// ponytail: no whitespace folding — a spaced query won't match where the gap
// is only an item boundary with no space char; fold both sides if that bites.
export function findMatches(query: string, pages: PageIndex[]): FindMatch[] {
  const q = foldCase(query);
  if (!q) return [];
  const out: FindMatch[] = [];
  pages.forEach((p, i) => {
    const hay = foldCase(p.text);
    let at = hay.indexOf(q);
    while (at !== -1) {
      out.push({ page: i + 1, start: at, end: at + q.length });
      at = hay.indexOf(q, at + q.length);
    }
  });
  return out;
}

/** The slices of one text item covered by the page's match ranges, clipped to
 * the item and rebased to item-local offsets. */
export function rangesForItem(
  itemStart: number,
  itemLength: number,
  pageRanges: HighlightRange[],
): HighlightRange[] {
  const itemEnd = itemStart + itemLength;
  const out: HighlightRange[] = [];
  for (const r of pageRanges) {
    if (r.end <= itemStart || r.start >= itemEnd) continue;
    out.push({
      start: Math.max(r.start - itemStart, 0),
      end: Math.min(r.end - itemStart, itemLength),
      current: r.current,
    });
  }
  return out;
}

const HTML_ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
};

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => HTML_ESCAPES[c]);
}

/** HTML for one text item with its ranges wrapped in <mark>; the current
 * match's piece gets pdf-find-current (styled in globals.css). Ranges must be
 * sorted and non-overlapping, which findMatches guarantees. */
export function highlightHtml(str: string, ranges: HighlightRange[]): string {
  if (ranges.length === 0) return escapeHtml(str);
  let html = "";
  let at = 0;
  for (const r of ranges) {
    html += escapeHtml(str.slice(at, r.start));
    html += `<mark class="pdf-find-mark${r.current ? " pdf-find-current" : ""}">${escapeHtml(str.slice(r.start, r.end))}</mark>`;
    at = r.end;
  }
  return html + escapeHtml(str.slice(at));
}
