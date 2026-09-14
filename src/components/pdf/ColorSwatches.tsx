import { HIGHLIGHT_COLORS } from "../../lib/pdfAnchor";

// Highlight color row shared by the reader popup and the Annotations tab;
// the selected swatch gets an accent ring.
export function ColorSwatches({
  value,
  onChange,
}: {
  value: string;
  onChange: (color: string) => void;
}) {
  return (
    <div className="flex items-center gap-1.5">
      {HIGHLIGHT_COLORS.map((c) => (
        <button
          key={c}
          type="button"
          aria-label={`Highlight color ${c}`}
          aria-pressed={c === value}
          onClick={() => onChange(c)}
          className={`w-4 h-4 rounded-full border border-black/20 transition-transform hover:scale-110 ${
            c === value ? "ring-2 ring-accent ring-offset-1" : ""
          }`}
          style={{ backgroundColor: c }}
        />
      ))}
    </div>
  );
}
