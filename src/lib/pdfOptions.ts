/** Shared pdf.js init options for every react-pdf <Document>. Range/stream
 * loading resolves the document early and re-lays out pages as chunks arrive,
 * which flashes for the whole download on slow links; force the full download
 * first instead. Module-level const: react-pdf reloads if identity changes. */
export const pdfDocumentOptions = {
  disableStream: true,
  disableRange: true,
};

// Horizontal padding per page, subtracted from the scroller's measured width
// to get the width react-pdf renders at.
export const PAGE_INSET = 32;

/** Estimated page height from a letter aspect ratio, for spacers and <Page>
 * loading placeholders — without one, freshly mounted pages collapse to a
 * few pixels and the document flashes as a bunch of thin strips. */
export function estPageHeight(width: number | null | undefined) {
  return width ? Math.round((width - PAGE_INSET) * 1.3) : 800;
}
