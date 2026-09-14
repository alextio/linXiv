/** Shared pdf.js init options for every react-pdf <Document>. Range/stream
 * loading resolves the document early and re-lays out pages as chunks arrive,
 * which flashes for the whole download on slow links; force the full download
 * first instead. Module-level const: react-pdf reloads if identity changes. */
export const pdfDocumentOptions = {
  disableStream: true,
  disableRange: true,
};
