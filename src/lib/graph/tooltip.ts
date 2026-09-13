// What the hover inspector says about a node.
//
// Canvas labels ellipsize, but the payload already carries each paper's
// category, date, tags, PDF flag and full abstract — so hovering peeks
// without navigating.

import type { GraphIndex, GraphNodeType } from "./model.ts";

export const SUMMARY_MAX = 260;

export interface TooltipContent {
  /** May contain TeX — arXiv titles do. The component renders it via MathText. */
  title: string;
  /** Meta lines, rendered one per line. Plain text: this is library metadata. */
  meta: string[];
  /** Truncated abstract. Kept out of `meta` so only it and the title take the
   *  MathText/TeX path. */
  summary?: string;
}

export function truncate(text: string, max: number): string {
  const t = text.replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  const cut = t.slice(0, max);
  const space = cut.lastIndexOf(" ");
  return `${(space > max * 0.6 ? cut.slice(0, space) : cut).trimEnd()}…`;
}

function pluralPapers(n: number): string {
  return `${n} ${n === 1 ? "paper" : "papers"}`;
}

/**
 * "Author · 37 papers", plus how many of those 37 the filter left drawn.
 *
 * The payload's degree counts every paper in the graph, so the client filter
 * must not rewrite it — but a node on a filtered canvas stands for a set mostly
 * not shown. Report both; unfiltered they are equal and the tail drops.
 */
function degreeLine(kind: string, total: number, drawn: number): string {
  const head = `${kind} · ${pluralPapers(total)}`;
  if (drawn === total) return head;
  return head + (drawn === 0 ? " (none shown)" : ` (${drawn} shown)`);
}

/** `drawnPapers` is the MATCHED paper ids of a drawn type — how much of what
 *  the hovered node stands for is shown, not whether it is itself. */
export function tooltipFor(
  nodeId: string,
  type: GraphNodeType,
  index: GraphIndex,
  drawnPapers: ReadonlySet<string>
): TooltipContent {
  if (type === "paper") {
    const p = index.paperById.get(nodeId);
    if (!p) return { title: "(untitled)", meta: [] };
    const meta: string[] = [];
    const head: string[] = [];
    if (p.category) head.push(p.category);
    // `published` is null for the no-date sentinel, so say so, not year 1.
    head.push(p.published ?? "No publication date");
    head.push(p.has_pdf ? "PDF" : "No PDF");
    meta.push(head.join(" · "));
    // Deduped and canonically spelled by the backend, same as the chips.
    if (p.tags.length) meta.push(p.tags.join(" · "));
    return {
      title: p.label || "(untitled)",
      meta,
      summary: p.summary ? truncate(p.summary, SUMMARY_MAX) : undefined,
    };
  }

  const node = type === "author" ? index.authorById.get(nodeId) : index.tagById.get(nodeId);
  if (!node) return { title: "(untitled)", meta: [] };
  const papers = index.papersByNode.get(nodeId) ?? [];
  const drawn = papers.filter((p) => drawnPapers.has(p)).length;
  return {
    title: node.label || "(untitled)",
    meta: [degreeLine(type === "author" ? "Author" : "Tag", node.paper_count, drawn)],
  };
}
