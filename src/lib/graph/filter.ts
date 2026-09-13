// Which nodes the Knowledge Graph's filter panels MATCH.
//
// Deliberately client-side rather than a Rust-side query: an excluded paper is
// still DRAWN (an 8% ghost), so "matched" is a rendering state, not a WHERE
// clause. Querying it away would take the ghosts, the layout they are pinned
// in, and the counts the panels report about what is held back.

import type { GraphIndex, GraphNodeType, GraphView } from "./model.ts";
import { normTag } from "./model.ts";

/** One row of the Paper Tags logic builder. The first row's `op` is unused. */
export interface TagRow {
  op: "AND" | "OR";
  tag: string;
}

export interface GraphFilterState {
  showPapers: boolean;
  showAuthors: boolean;
  showTags: boolean;
  category: string;
  hasPdf: boolean;
  dateFrom: string;
  dateTo: string;
  title: string;
  author: string;
  /** "Show highlighted only" — take the non-matching nodes to opacity 0. */
  isolate: boolean;
  /** Free text matched against project names, case-insensitively, by substring. */
  projectNames: string[];
  /** Free text matched WHOLE against project tags, case-insensitively. */
  projectTags: string[];
  tagRows: TagRow[];
}

export const EMPTY_FILTER: GraphFilterState = {
  showPapers: true,
  showAuthors: true,
  showTags: true,
  category: "",
  hasPdf: false,
  dateFrom: "",
  dateTo: "",
  title: "",
  author: "",
  isolate: false,
  projectNames: [],
  projectTags: [],
  tagRows: [],
};

/** What a filter pass concluded. With no filter in force the three sets still
 *  hold every node of their type, so style and layout passes never need an
 *  "is a filter on" branch. */
export interface GraphMatch {
  papers: Set<string>;
  authors: Set<string>;
  tags: Set<string>;
  /** Node types a Visibility checkbox switched off: not drawn at all. */
  hiddenTypes: Set<GraphNodeType>;
  isolate: boolean;
  /** Matched nodes of a still-drawn type. Zero on a non-empty library is the
   *  "no matches" state. */
  drawnCount: number;
}

/**
 * The node ids the force layout runs over — the one source charge, collision
 * radius, link set and drag release must agree on (excluded nodes are PINNED,
 * not removed). Hidden types drop out too: an invisible node must not shape the
 * layout of the ones the user can see.
 */
export function layoutIds(m: GraphMatch): Set<string> {
  const ids = new Set<string>();
  if (!m.hiddenTypes.has("paper")) for (const id of m.papers) ids.add(id);
  if (!m.hiddenTypes.has("author")) for (const id of m.authors) ids.add(id);
  if (!m.hiddenTypes.has("tag")) for (const id of m.tags) ids.add(id);
  return ids;
}

/** Whether anything in `state` narrows what the canvas shows. */
export function isFilterActive(state: GraphFilterState): boolean {
  return activeFilterSummary(state).length > 0 || activeTagFilterSummary(state).length > 0;
}

/** The projects one Projects row stands for: case-insensitive SUBSTRING on name. */
export function projectsMatchingName(view: GraphView, name: string) {
  const lower = name.toLowerCase();
  return view.projects.filter((p) => p.name.toLowerCase().includes(lower));
}

/** The projects one Project Tags row stands for: case-insensitive WHOLE tag. */
export function projectsWithTag(view: GraphView, tag: string) {
  const lower = tag.toLowerCase();
  return view.projects.filter((p) => p.tags.some((t) => t.toLowerCase() === lower));
}

/**
 * Evaluate the Paper Tags logic builder against one paper's normalized tags.
 * Rows fold left, so `A OR B AND C` is `((A OR B) AND C)` — which is what the
 * row list reads as top to bottom.
 */
export function evalTagRows(tagKeys: readonly string[], rows: readonly TagRow[]): boolean {
  if (rows.length === 0) return true;
  const has = (row: TagRow) => tagKeys.includes(normTag(row.tag));
  let result = has(rows[0]);
  for (let i = 1; i < rows.length; i++) {
    result = rows[i].op === "AND" ? result && has(rows[i]) : result || has(rows[i]);
  }
  return result;
}

/**
 * The project ids a Projects filter resolves to, `null` when it is off. An EMPTY
 * set is a different answer: rows are free text, so a typo or a renamed project
 * resolves to nothing — which must match no paper rather than every one.
 */
function resolveProjectIds(view: GraphView, names: readonly string[]): Set<number> | null {
  if (names.length === 0) return null;
  const ids = new Set<number>();
  for (const name of names) {
    for (const p of projectsMatchingName(view, name)) ids.add(p.id);
  }
  return ids;
}

export function matchGraph(
  view: GraphView,
  index: GraphIndex,
  state: GraphFilterState
): GraphMatch {
  const category = state.category.trim().toLowerCase();
  const title = state.title.trim().toLowerCase();
  const author = state.author.trim().toLowerCase();
  const dateFrom = state.dateFrom.trim();
  const dateTo = state.dateTo.trim();
  const projectIds = resolveProjectIds(view, state.projectNames);
  // Whole-string and case-insensitive, as TAG.TAG's UNIQUE COLLATE NOCASE makes
  // every tag comparison. Folded once here.
  const projTags =
    state.projectTags.length > 0
      ? new Set(state.projectTags.map((t) => t.trim().toLowerCase()))
      : null;

  const papers = new Set<string>();
  for (const p of view.papers) {
    if (category && !(p.category ?? "").toLowerCase().includes(category)) continue;
    if (state.hasPdf && !p.has_pdf) continue;
    if (!evalTagRows(p.tag_keys, state.tagRows)) continue;
    if (projectIds && !p.project_ids.some((id) => projectIds.has(id))) continue;
    if (projTags) {
      const hit = p.project_ids.some((id) =>
        (index.projectById.get(id)?.tags ?? []).some((t) => projTags.has(t.toLowerCase()))
      );
      if (!hit) continue;
    }
    if (title && !p.label.toLowerCase().includes(title)) continue;
    // `published` is null for an undated paper (backend folds the sentinel), so
    // a date range never silently drops one as "too old".
    if (dateFrom && p.published && p.published < dateFrom) continue;
    if (dateTo && p.published && p.published > dateTo) continue;
    if (author && !p.author_keys.some((a) => a.includes(author))) continue;
    papers.add(p.id);
  }

  // Authors and tags match purely by adjacency to a matching paper; the
  // Visibility checkboxes below are a RENDER concern. Folding them in is what
  // made unchecking "Papers" blank the canvas. Match first, hide after.
  const authors = new Set<string>();
  const tags = new Set<string>();
  for (const pid of papers) {
    for (const nid of index.neighboursByPaper.get(pid) ?? []) {
      if (index.typeById.get(nid) === "author") authors.add(nid);
      else if (index.typeById.get(nid) === "tag") tags.add(nid);
    }
  }

  const hiddenTypes = new Set<GraphNodeType>();
  if (!state.showPapers) hiddenTypes.add("paper");
  if (!state.showAuthors) hiddenTypes.add("author");
  if (!state.showTags) hiddenTypes.add("tag");

  const drawnCount =
    (hiddenTypes.has("paper") ? 0 : papers.size) +
    (hiddenTypes.has("author") ? 0 : authors.size) +
    (hiddenTypes.has("tag") ? 0 : tags.size);

  return { papers, authors, tags, hiddenTypes, isolate: state.isolate, drawnCount };
}

/**
 * The Filters panel's active controls, in the order the panel lists them.
 *
 * Both panels open COLLAPSED and their state outlives navigation, so without
 * this the header says nothing about a canvas of 8% ghosts. The Visibility
 * checkboxes count: "don't draw authors" is just as much a reason.
 */
export function activeFilterSummary(state: GraphFilterState): string[] {
  const on: string[] = [];
  if (!state.showPapers) on.push("Papers hidden");
  if (!state.showAuthors) on.push("Authors hidden");
  if (!state.showTags) on.push("Tags hidden");
  if (state.category.trim()) on.push(`Category: ${state.category.trim()}`);
  if (state.hasPdf) on.push("Has PDF only");
  if (state.dateFrom.trim()) on.push(`Published from ${state.dateFrom.trim()}`);
  if (state.dateTo.trim()) on.push(`Published to ${state.dateTo.trim()}`);
  if (state.title.trim()) on.push(`Title: ${state.title.trim()}`);
  if (state.author.trim()) on.push(`Author: ${state.author.trim()}`);
  if (state.isolate) on.push("Show highlighted only");
  return on;
}

/**
 * The Tag Filter panel's three lists. Only the ROWS count — text left in an
 * add-box is not a filter, the same line `matchGraph` draws. Paper-tag rows
 * carry their AND/OR, since OR and AND filter nothing alike.
 */
export function activeTagFilterSummary(state: GraphFilterState): string[] {
  return [
    ...state.projectNames.map((n) => `Project: ${n}`),
    ...state.projectTags.map((n) => `Project tag: ${n}`),
    ...state.tagRows.map((r, i) => `${i > 0 ? `${r.op} ` : ""}Tag: ${r.tag}`),
  ];
}

/**
 * Why the canvas is empty — i.e. which panel to send the user to. Only
 * meaningful when `drawnCount === 0` on a non-empty library.
 *
 * A paper still MATCHING means the attribute filters are not the cause; it is a
 * Visibility checkbox. `types` names only switched-off types that had something
 * to show, so a library with no tags is not told its Tags are hidden — which is
 * also why "count the hidden types" is the wrong test.
 */
export type NoMatchCause =
  | { kind: "visibility"; types: Array<"Papers" | "Authors" | "Tags"> }
  | { kind: "filters" };

export function noMatchCause(match: GraphMatch): NoMatchCause {
  if (match.papers.size === 0) return { kind: "filters" };
  const sizes = { paper: match.papers.size, author: match.authors.size, tag: match.tags.size };
  const labels = { paper: "Papers", author: "Authors", tag: "Tags" } as const;
  const types = (["paper", "author", "tag"] as const)
    .filter((t) => match.hiddenTypes.has(t) && sizes[t] > 0)
    .map((t) => labels[t]);
  return { kind: "visibility", types };
}

/** "Papers", "Papers and Authors", "Papers, Authors and Tags". */
export function joinTypes(types: readonly string[]): string {
  if (types.length === 0) return "Every node type";
  if (types.length === 1) return types[0];
  return `${types.slice(0, -1).join(", ")} and ${types[types.length - 1]}`;
}
