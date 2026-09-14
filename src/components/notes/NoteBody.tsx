import type { Note } from "../../types/api";
import { NoteMarkdown } from "./NoteMarkdown";

// A saved note's body, rendered through NoteMarkdown exactly as the editor's
// Preview tab does. forceInline (default) keeps display math inline so a
// display:block container can't escape the card's line-clamp box; the full read
// page passes false so display equations render as centered blocks.
export function NoteBody({
  content,
  className,
  forceInline = true,
}: {
  content: string;
  className?: string;
  forceInline?: boolean;
}) {
  return <NoteMarkdown content={content} className={className} forceInline={forceInline} />;
}

// created_at and updated_at are equal at creation and diverge on PATCH. Compare
// parsed instants, not raw strings, so timestamp-formatting differences can't
// produce a false "edited" flag. Returns the timestamp to display + the flag.
export function noteEdited(note: Note): { date: string | null; edited: boolean } {
  const createdMs = note.created_at ? Date.parse(note.created_at) : NaN;
  const updatedMs = note.updated_at ? Date.parse(note.updated_at) : NaN;
  const edited =
    Number.isFinite(createdMs) &&
    Number.isFinite(updatedMs) &&
    updatedMs !== createdMs;
  return { date: edited ? note.updated_at : note.created_at, edited };
}
