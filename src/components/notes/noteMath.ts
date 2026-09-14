// Pure (JSX-free) math-placeholder extraction for the note markdown preview, split
// out of NoteMarkdown.tsx so `node --experimental-strip-types` can test it: it
// strips type annotations but cannot parse the JSX in the .tsx.

// Matches, in priority order: fenced code blocks, inline code spans, display
// math $$...$$, inline math $...$. Backtick matches pass through unchanged.
const MATH_OR_CODE_RE = /```[\s\S]*?```|`[^`\n]*`|\$\$[\s\S]+?\$\$|\$(?!\s)[^$\n]*?[^$\s]\$/g;

// Private Use Area sentinels (U+E000 open, U+E001 close) bracket a math[] index.
// PUA codepoints don't occur in normal note text, so the restore regex matches only
// placeholders we inserted — a bare "\d+" token would clobber any real digit.
// Built via fromCharCode so the source stays ASCII.
const MATH_OPEN = String.fromCharCode(0xe000);
const MATH_CLOSE = String.fromCharCode(0xe001);
const MATH_TOKEN_RE = new RegExp(MATH_OPEN + "(\\d+)" + MATH_CLOSE, "g");

// Pull $…$ / $$…$$ spans out of the raw markdown into placeholder tokens
// before react-markdown parses it.
export function extractMath(raw: string): { text: string; math: string[] } {
  const math: string[] = [];
  const text = raw.replace(MATH_OR_CODE_RE, (m) => {
    if (m[0] === "`") return m;
    const token = `${MATH_OPEN}${math.length}${MATH_CLOSE}`;
    math.push(m);
    return token;
  });
  return { text, math };
}

// Swap placeholder tokens in a rendered leaf string back for their original
// $…$ / $$…$$ source text.
export function restoreMath(text: string, math: string[]): string {
  if (math.length === 0) return text;
  return text.replace(MATH_TOKEN_RE, (_m, i: string) => math[Number(i)] ?? "");
}
