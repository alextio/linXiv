import type { Anchor } from "../../lib/pdfAnchor";

export interface PageHighlight {
  id: number;
  anchor: Anchor;
}

// Purely visual overlay inside a react-pdf `<Page>` (position:relative), so
// percentage coords map onto the page box. pointer-events:none keeps the text
// under a highlight selectable; PdfReader hit-tests clicks geometrically.
export function HighlightLayer({ highlights }: { highlights: PageHighlight[] }) {
  if (highlights.length === 0) return null;
  return (
    <div className="absolute inset-0 z-10 pointer-events-none">
      {highlights.map(({ id, anchor }) =>
        anchor.rects.map((r, i) => (
          <div
            key={`${id}-${i}`}
            className="absolute mix-blend-multiply rounded-[1px]"
            style={{
              left: `${r.x * 100}%`,
              top: `${r.y * 100}%`,
              width: `${r.w * 100}%`,
              height: `${r.h * 100}%`,
              backgroundColor: anchor.color,
              opacity: 0.4,
            }}
          />
        )),
      )}
    </div>
  );
}
