import type { QueryClient, UseMutationOptions } from "@tanstack/react-query";
import { addPapers, createProjectWithPapers } from "../api/projects.ts";
import type { AddPapersVars, CreateProjectWithPapersVars } from "../api/projects.ts";
import { errText } from "./errText.ts";

// Invalidation registry: one owner per operation, so every page doing it
// refreshes the same views. Keys are prefixes — ["papers"] matches
// ["papers","list",sort].

/** Keys affected by a tag edit. Folded into the sets whose operations can edit
 *  tags (paper saves, project saves, imports). */
export const TAG_QUERY_KEYS: readonly string[] = ["tags", "tag"];

/** "This operation changes what `GET /api/graph` would return."
 *
 *  Marked stale but NOT refetched (see `invalidateAll`): a reload rebuilds the
 *  force layout the user may have spent a while arranging, so GraphPage's
 *  Refresh dot (fed by `onGraphDirtying` below) leaves reloading to them.
 *  Note/annotation edits are invisible to the graph; anything that adds,
 *  removes, retitles, retags, reprojects or re-authors a paper is not. */
export const GRAPH_QUERY_KEY = "graph";

/** Cached keys whose contents depend on which papers exist. */
export const PAPER_QUERY_KEYS: readonly string[] = [
  "papers",
  "paper",
  "projects",
  "project",
  "notes",
  "note",
  "annotations",
  ...TAG_QUERY_KEYS,
  GRAPH_QUERY_KEY,
  "stats",
  "trash",
  // Reading-status rows cascade with papers/memberships and move on merge.
  "reading-status",
];

/** Keys affected by a paper mutation short of deletion: save from search/DOI/
 *  feed, import, new-version fetch, full-text index, PDF attach/detach. */
export const PAPER_MUTATION_QUERY_KEYS: readonly string[] = [
  "papers",
  "paper",
  "stats",
  ...TAG_QUERY_KEYS,
  "saved-pdfs",
  // A saved paper is a new node; a new version or tag edit changes a drawn one.
  GRAPH_QUERY_KEY,
];

/** Keys affected by a project mutation: create, edit (incl. its tags),
 *  archive, restore, soft/hard delete, share import. */
export const PROJECT_MUTATION_QUERY_KEYS: readonly string[] = [
  "projects",
  "project",
  ...TAG_QUERY_KEYS,
  "trash",
  // `/api/graph` carries each active project's name, colour and tags — what the
  // Projects / Project Tags filter rows resolve against.
  GRAPH_QUERY_KEY,
  // Trashing/restoring a reading list hides/reveals its status rows.
  "reading-status",
];

/** Keys affected by changing which papers belong to a project. */
export const PROJECT_MEMBERSHIP_QUERY_KEYS: readonly string[] = [
  "projects",
  "project",
  "papers",
  // Each paper node carries its active projects (`project_ids`), what the
  // graph's Projects filter matches on.
  GRAPH_QUERY_KEY,
  // Removing a paper from a reading list cascades its status row away.
  "reading-status",
];

/** Keys affected by an author rename, delete, merge, or a paper↔author
 *  link/unlink (reassign is unlink+link). The graph draws author nodes and
 *  paper->author edges; no page need hold an ["authors"] query, so only the
 *  GRAPH_QUERY_KEY marker reliably tells the graph to redraw. */
export const AUTHOR_MUTATION_QUERY_KEYS: readonly string[] = [
  "authors",
  "author",
  "author-merge-candidates",
  GRAPH_QUERY_KEY,
];

/** Keys affected by a note create/edit/delete. */
export const NOTE_QUERY_KEYS: readonly string[] = ["notes", "note"];

/** Keys affected by an annotation create/edit/delete. */
export const ANNOTATION_QUERY_KEYS: readonly string[] = ["annotations"];

// --- Graph staleness -------------------------------------------------------
// GraphPage also watches the query cache for `invalidate` events, but
// react-query emits one only per query ACTUALLY IN THE CACHE: invalidating
// ["authors"] from a page that never mounted such a query notifies nobody. The
// registry already knows which operations touch the graph, so it says so
// directly; the cache subscription stays for sites that bypass this file.
type GraphDirtyListener = () => void;
const graphDirtyListeners = new Set<GraphDirtyListener>();

/** Subscribe to "an operation just changed what `/api/graph` would return".
 *  Returns the unsubscribe. */
export function onGraphDirtying(listener: GraphDirtyListener): () => void {
  graphDirtyListeners.add(listener);
  return () => {
    graphDirtyListeners.delete(listener);
  };
}

function invalidateAll(qc: QueryClient, keys: readonly string[]): Promise<void> {
  // Announced before the awaits: the flag is about backend data that already
  // changed, not about the refetches finishing.
  if (keys.includes(GRAPH_QUERY_KEY)) {
    for (const listener of [...graphDirtyListeners]) listener();
  }
  return Promise.all(
    keys.map((k) =>
      // `refetchType: "none"` for the graph alone: mark stale, do not fetch.
      // Everything else refreshes on its own, and should. The default would
      // refresh the graph too — ["graph"] matches GraphPage's ["graph",
      // hideSingleAuthors] by prefix, and refetchType "active" refetches a
      // mounted query at once whatever its staleTime. AppShell keeps GraphPage
      // mounted behind `display: none`, so a retitle from the Library would
      // silently re-fetch, re-anneal from alpha 1 and drift the user's layout —
      // or destroy the node under a mid-drag gesture.
      qc.invalidateQueries(
        k === GRAPH_QUERY_KEY ? { queryKey: [k], refetchType: "none" } : { queryKey: [k] }
      )
    )
  ).then(() => {});
}

/** Fan-out for a paper appearing, disappearing, or changing: soft delete,
 *  restore, hard delete, metadata save. */
export function invalidatePaperQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, PAPER_QUERY_KEYS);
}

export function invalidatePaperMutationQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, PAPER_MUTATION_QUERY_KEYS);
}

export function invalidateProjectMutationQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, PROJECT_MUTATION_QUERY_KEYS);
}

export function invalidateProjectMembershipQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, PROJECT_MEMBERSHIP_QUERY_KEYS);
}

export function invalidateAuthorQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, AUTHOR_MUTATION_QUERY_KEYS);
}

export function invalidateNoteQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, NOTE_QUERY_KEYS);
}

export function invalidateAnnotationQueries(qc: QueryClient): Promise<void> {
  return invalidateAll(qc, ANNOTATION_QUERY_KEYS);
}

/** Shared wording for the partial-failure contract. */
export function partialFailureMessage(failedCount: number, totalCount: number): string {
  const plural = totalCount !== 1 ? "s" : "";
  return `${failedCount} of ${totalCount} paper${plural} could not be added`;
}

/** The page-specific hooks the shared project-picker mutations drive. */
export interface ProjectPickerActions {
  setError: (message: string | null) => void;
  /** Re-select only the failures so a retry can't re-add the rest. */
  selectFailures: (sourceIds: string[]) => void;
  /** Close the picker and clear the page's selection. */
  onDone: () => void;
  /** Clear the new-project name field (create only). */
  clearName: () => void;
}

/** Add-selection-to-project, shared by Library and Graph. Partial-failure
 *  contract: never throw — resolve with the failed ids, re-select exactly
 *  those and report the count, so a retry can't re-add what already landed. */
export function addToProjectMutationOptions(
  qc: QueryClient,
  ui: ProjectPickerActions
): UseMutationOptions<string[], Error, AddPapersVars> {
  return {
    mutationFn: addPapers,
    onMutate: () => {
      ui.setError(null);
    },
    onSettled: () => {
      invalidateProjectMembershipQueries(qc);
    },
    onSuccess: (failedIds, { sourceIds }) => {
      if (failedIds.length > 0) {
        ui.selectFailures(failedIds);
        ui.setError(partialFailureMessage(failedIds.length, sourceIds.length));
        return;
      }
      ui.setError(null);
      ui.onDone();
    },
    onError: (err) => {
      ui.setError(errText(err, "Failed to add papers to project"));
    },
  };
}

/** Create-project-with-selection, shared by Library and Graph. Same
 *  partial-failure contract as addToProjectMutationOptions. */
export function createProjectMutationOptions(
  qc: QueryClient,
  ui: ProjectPickerActions
): UseMutationOptions<string[], Error, CreateProjectWithPapersVars> {
  return {
    mutationFn: createProjectWithPapers,
    // onSettled, not onSuccess: only a failed createProject rejects here, and
    // that path has nothing to refresh anyway.
    onSettled: () => {
      invalidateProjectMembershipQueries(qc);
    },
    onSuccess: (failedIds) => {
      // The project exists either way; clear the name so a retry can't dupe it.
      ui.clearName();
      if (failedIds.length > 0) {
        ui.selectFailures(failedIds);
        ui.setError(
          `Project created, but ${failedIds.length} paper${failedIds.length !== 1 ? "s" : ""} could not be added`
        );
        return;
      }
      ui.setError(null);
      ui.onDone();
    },
    onError: (err) => {
      ui.setError(errText(err, "Failed to create project"));
    },
  };
}
