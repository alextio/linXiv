/** Format an ISO date string ("2024-01-01" or "2024-01-01T...") as a short
 *  localized date. Returns the raw string on anything unparseable, and guards
 *  against JS's silent month/day rollover (e.g. month 13, day 99). */
export function formatDate(dateStr: string | null): string {
  if (!dateStr) return "";
  // Slice to 10 chars to handle ISO 8601 timestamps ("2024-01-01T...").
  const [y, m, d] = dateStr.slice(0, 10).split("-").map(Number);
  if (!Number.isFinite(y) || !Number.isFinite(m) || !Number.isFinite(d)) return dateStr;
  const date = new Date(y, m - 1, d);
  // Round-trip every component back out of the constructed Date: this catches
  // the rollover above AND JS's two-digit-year remap, where new Date(1, 0, 1)
  // becomes 1901 — so a "0001-01-01" sentinel never renders as a real 1901 date.
  if (date.getFullYear() !== y || date.getMonth() !== m - 1 || date.getDate() !== d) return dateStr;
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
