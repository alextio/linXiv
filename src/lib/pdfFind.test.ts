// Run: node --experimental-strip-types --test src/lib/pdfFind.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  buildPageIndex,
  findMatches,
  foldCase,
  highlightHtml,
  rangesForItem,
} from "./pdfFind.ts";

test("findMatches is case-insensitive and spans item boundaries", () => {
  const pages = [
    buildPageIndex(["Deep Lear", "ning models"]),
    buildPageIndex(["learning rate"]),
  ];
  const matches = findMatches("learning", pages);
  assert.deepEqual(matches, [
    { page: 1, start: 5, end: 13 },
    { page: 2, start: 0, end: 8 },
  ]);
  assert.deepEqual(findMatches("", pages), []);
});

test("findMatches finds repeated non-overlapping hits", () => {
  const pages = [buildPageIndex(["aaaa"])];
  assert.deepEqual(findMatches("aa", pages), [
    { page: 1, start: 0, end: 2 },
    { page: 1, start: 2, end: 4 },
  ]);
});

test("foldCase preserves string length", () => {
  assert.equal(foldCase("ABC İ ẞ"), foldCase("ABC İ ẞ"));
  assert.equal(foldCase("İX").length, 2);
  assert.equal(foldCase("AbC"), "abc");
});

test("rangesForItem clips page ranges to one item", () => {
  // items: "Deep Lear" (start 0) + "ning models" (start 9); match 5..13
  const ranges = [{ start: 5, end: 13, current: true }];
  assert.deepEqual(rangesForItem(0, 9, ranges), [
    { start: 5, end: 9, current: true },
  ]);
  assert.deepEqual(rangesForItem(9, 11, ranges), [
    { start: 0, end: 4, current: true },
  ]);
  assert.deepEqual(rangesForItem(20, 5, ranges), []);
});

test("highlightHtml wraps ranges in marks and escapes HTML", () => {
  assert.equal(highlightHtml("a<b>", []), "a&lt;b&gt;");
  assert.equal(
    highlightHtml("x <cat> y", [{ start: 2, end: 7, current: false }]),
    'x <mark class="pdf-find-mark">&lt;cat&gt;</mark> y',
  );
  assert.equal(
    highlightHtml("abc", [{ start: 1, end: 2, current: true }]),
    'a<mark class="pdf-find-mark pdf-find-current">b</mark>c',
  );
});
