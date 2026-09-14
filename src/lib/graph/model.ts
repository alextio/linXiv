// Shared vocabulary for the Knowledge Graph page. The wire types are generated
// from src-tauri/crates/core/src/graph.rs into src/types/generated.ts.

import type {
  GraphAuthor,
  GraphEdge,
  GraphPaper,
  GraphProject,
  GraphTag,
  GraphView,
} from "../../types/generated.ts";

export type {
  GraphAuthor,
  GraphEdge,
  GraphPaper,
  GraphProject,
  GraphTag,
  GraphView,
};

export type GraphNodeType = "paper" | "author" | "tag";

/** A node as cytoscape holds it: one id space over all three types. */
export interface GraphNodeData {
  id: string;
  type: GraphNodeType;
  label: string;
  /** Papers only — the vocabulary the project picker speaks. */
  source_id?: string;
  /** Authors only — the `/authors/:id` route param. */
  author_id?: number;
  /** Authors and tags only — papers on this canvas joined to the node. */
  paper_count?: number;
}

/** Lookups the filter and the canvas share, built once per payload — id-space
 *  bookkeeping only; names, canonical tag spelling and degree ride on the payload. */
export interface GraphIndex {
  paperById: Map<string, GraphPaper>;
  authorById: Map<string, GraphAuthor>;
  tagById: Map<string, GraphTag>;
  projectById: Map<number, GraphProject>;
  /** Node id -> its type, for edge endpoints and style passes. */
  typeById: Map<string, GraphNodeType>;
  /** Paper node id -> the author/tag node ids it is joined to. */
  neighboursByPaper: Map<string, string[]>;
  /** Author or tag node id -> the paper node ids joined to it. */
  papersByNode: Map<string, string[]>;
}

export function indexView(view: GraphView): GraphIndex {
  const paperById = new Map(view.papers.map((p) => [p.id, p]));
  const authorById = new Map(view.authors.map((a) => [a.id, a]));
  const tagById = new Map(view.tags.map((t) => [t.id, t]));
  const projectById = new Map(view.projects.map((p) => [p.id, p]));

  const typeById = new Map<string, GraphNodeType>();
  for (const p of view.papers) typeById.set(p.id, "paper");
  for (const a of view.authors) typeById.set(a.id, "author");
  for (const t of view.tags) typeById.set(t.id, "tag");

  const neighboursByPaper = new Map<string, string[]>();
  const papersByNode = new Map<string, string[]>();
  for (const e of view.edges) {
    push(neighboursByPaper, e.source, e.target);
    push(papersByNode, e.target, e.source);
  }
  return {
    paperById,
    authorById,
    tagById,
    projectById,
    typeById,
    neighboursByPaper,
    papersByNode,
  };
}

function push<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  const list = map.get(key);
  if (list) list.push(value);
  else map.set(key, [value]);
}

/** Trim then lower-case — the rule `linxiv_core::graph::norm_tag` applies
 *  server-side, and the one every PAPER-tag comparison here goes through: a
 *  typed tag is free text, while `GraphPaper.tag_keys` / `GraphTag.key` arrive
 *  normalized. (Project tags fold inline in filter.ts.) */
export function normTag(raw: string): string {
  return raw.trim().toLowerCase();
}
