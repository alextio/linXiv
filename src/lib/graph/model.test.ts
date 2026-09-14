// Run: node --experimental-transform-types --test src/lib/graph/model.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";

import { indexView, normTag } from "./model.ts";
import { sampleView } from "./fixture.ts";

// A typed filter row is free text, so it is folded here with the same rule
// `linxiv_core::graph::norm_tag` applies to what it is compared against.
test("normTag trims then folds case", () => {
  assert.equal(normTag("ML"), "ml");
  assert.equal(normTag("  ml  "), "ml");
  assert.equal(normTag("ml "), normTag("ML"));
  assert.equal(normTag("   "), "");
});

test("indexView keys every node by id and type", () => {
  const index = indexView(sampleView());
  assert.equal(index.paperById.get("1")?.label, "Attention Is All You Need");
  assert.equal(index.authorById.get("author::7")?.author_id, 7);
  assert.equal(index.tagById.get("tag::ml")?.label, "ML");
  assert.equal(index.projectById.get(10)?.name, "Transformers");
  assert.equal(index.typeById.get("1"), "paper");
  assert.equal(index.typeById.get("author::7"), "author");
  assert.equal(index.typeById.get("tag::nlp"), "tag");
  assert.equal(index.typeById.get("nope"), undefined);
});

test("indexView walks the edges both ways", () => {
  const index = indexView(sampleView());
  assert.deepEqual(index.neighboursByPaper.get("1"), [
    "author::7",
    "author::8",
    "tag::ml",
    "tag::nlp",
  ]);
  assert.deepEqual(index.papersByNode.get("author::7"), ["1", "2"]);
  assert.deepEqual(index.papersByNode.get("tag::ml"), ["1"]);
  // A non-paper id is absent rather than empty — every caller defaults it.
  assert.equal(index.neighboursByPaper.get("author::7"), undefined);
});

test("an empty payload indexes to empty lookups", () => {
  const index = indexView({
    papers: [],
    authors: [],
    tags: [],
    edges: [],
    categories: [],
    projects: [],
  });
  assert.equal(index.paperById.size, 0);
  assert.equal(index.typeById.size, 0);
  assert.equal(index.neighboursByPaper.size, 0);
});
