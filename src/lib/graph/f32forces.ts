// Float32Array-backed replacements for the two quadtree forces d3 runs every
// tick — many-body repulsion and collision. d3's own versions rebuild an
// object quadtree per tick (per collide iteration, even) and chase pointers
// through it for every node; these keep the same math but hold positions,
// velocities and the tree itself in flat typed arrays that persist across
// ticks, so the hot O(n log n) loops touch cache-friendly f32 lanes and
// allocate nothing.
//
// Two deliberate narrowings, matching how GraphCanvas actually uses d3:
// membership is binary — a node is either in the layout (charge -repel,
// collide radius r) or filter-excluded (0, 0). An excluded node is always
// pinned (fx/fy set) and its zero strength/radius makes it inert both ways,
// so it is dropped from the tree and the receiver loop outright instead of
// being carried at weight zero like d3 carries it. Uniformity is also what
// lets a cell aggregate be a plain centroid and the collide correction split
// 50/50.

import type { Force, SimulationNodeDatum } from "d3-force";

/** What the forces read and write on a simulation node. */
export type F32Node = SimulationNodeDatum & { id: string; x: number; y: number };

// d3-force defaults, kept so the layout settles exactly as it used to.
const THETA2 = 0.81;
const DISTANCE_MIN2 = 1;
/** Splits stop here and near-coincident points chain instead: two distinct
 *  f32 coordinates separate within ~23 halvings of the extent, so deeper
 *  cells could only chase rounding noise forever. */
const MAX_DEPTH = 23;
/** Traversal stack bound: at most 4 pending entries per tree level. */
const STACK = 4 * (MAX_DEPTH + 2);

/** d3's tie-breaker for coincident points: a tiny random offset. */
const jiggle = (random: () => number) => (random() - 0.5) * 1e-6;

/**
 * A quadtree over f32 points, flattened into typed arrays. `child` holds 4
 * slots per cell: 0 = empty, c > 0 = internal cell c, v < 0 = leaf holding
 * point -(v+1), possibly chained through `next` (coincident points, or
 * near-coincident ones parked together at MAX_DEPTH). Cells are allocated
 * parent-before-child, which is what lets `aggregate` fold sums bottom-up
 * with one reverse index sweep instead of a traversal.
 */
class F32Quadtree {
  child = new Int32Array(256 * 4);
  next = new Int32Array(0);
  cx = new Float32Array(256);
  cy = new Float32Array(256);
  cnt = new Int32Array(256);
  cells = 0;
  x0 = 0;
  y0 = 0;
  size = 0;

  build(px: Float32Array, py: Float32Array, n: number): void {
    if (this.next.length < n) this.next = new Int32Array(n);
    this.cells = 1;
    this.child.fill(0, 0, 4);
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (let i = 0; i < n; i++) {
      const x = px[i];
      const y = py[i];
      if (!Number.isFinite(x) || !Number.isFinite(y)) continue; // as d3 drops them
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
    if (minX > maxX) {
      this.size = 0;
      return;
    }
    this.x0 = minX;
    this.y0 = minY;
    this.size = Math.max(maxX - minX, maxY - minY) || 1;
    for (let i = 0; i < n; i++) {
      if (Number.isFinite(px[i]) && Number.isFinite(py[i])) this.insert(px, py, i);
    }
  }

  private newCell(): number {
    if (4 * (this.cells + 1) > this.child.length) {
      const child = new Int32Array(this.child.length * 2);
      child.set(this.child);
      this.child = child;
      const cap = child.length / 4;
      const cx = new Float32Array(cap);
      const cy = new Float32Array(cap);
      const cnt = new Int32Array(cap);
      this.cx = cx;
      this.cy = cy;
      this.cnt = cnt;
    }
    const m = this.cells++;
    this.child.fill(0, 4 * m, 4 * m + 4);
    return m;
  }

  private insert(px: Float32Array, py: Float32Array, p: number): void {
    const x = px[p];
    const y = py[p];
    let cell = 0;
    let bx = this.x0;
    let by = this.y0;
    let s = this.size;
    let depth = 0;
    for (;;) {
      const half = s / 2;
      const q = (x >= bx + half ? 1 : 0) | (y >= by + half ? 2 : 0);
      const slot = 4 * cell + q;
      const v = this.child[slot];
      if (v === 0) {
        this.child[slot] = -(p + 1);
        this.next[p] = -1;
        return;
      }
      if (v > 0) {
        cell = v;
        if (q & 1) bx += half;
        if (q & 2) by += half;
        s = half;
        depth++;
        continue;
      }
      const h = -v - 1; // occupied leaf: split, or chain if inseparable
      if (depth >= MAX_DEPTH || (px[h] === x && py[h] === y)) {
        this.child[slot] = -(p + 1);
        this.next[p] = h;
        return;
      }
      const hx = px[h];
      const hy = py[h];
      let pslot = slot;
      if (q & 1) bx += half;
      if (q & 2) by += half;
      s = half;
      depth++;
      // Push the resident chain down through fresh cells until the newcomer
      // lands in a different quadrant (or depth runs out and they chain).
      for (;;) {
        const m = this.newCell();
        this.child[pslot] = m;
        const hh = s / 2;
        const qh = (hx >= bx + hh ? 1 : 0) | (hy >= by + hh ? 2 : 0);
        const qp = (x >= bx + hh ? 1 : 0) | (y >= by + hh ? 2 : 0);
        if (qh !== qp) {
          this.child[4 * m + qh] = -(h + 1);
          this.child[4 * m + qp] = -(p + 1);
          this.next[p] = -1;
          return;
        }
        if (depth >= MAX_DEPTH) {
          this.child[4 * m + qh] = -(p + 1);
          this.next[p] = h;
          return;
        }
        pslot = 4 * m + qh;
        if (qh & 1) bx += hh;
        if (qh & 2) by += hh;
        s = hh;
        depth++;
      }
    }
  }

  /** Per-cell point count and centroid — the whole Barnes-Hut aggregate when
   *  every point carries the same charge. */
  aggregate(px: Float32Array, py: Float32Array): void {
    const { child, next, cx, cy, cnt, cells } = this;
    cnt.fill(0, 0, cells);
    cx.fill(0, 0, cells);
    cy.fill(0, 0, cells);
    // Children always index higher than their parent, so a single descending
    // sweep sees every cell's subtrees finished before folding them in.
    for (let c = cells - 1; c >= 0; c--) {
      for (let q = 0; q < 4; q++) {
        const v = child[4 * c + q];
        if (v === 0) continue;
        if (v > 0) {
          cnt[c] += cnt[v];
          cx[c] += cx[v];
          cy[c] += cy[v];
        } else {
          for (let j = -v - 1; j >= 0; j = next[j]) {
            cnt[c]++;
            cx[c] += px[j];
            cy[c] += py[j];
          }
        }
      }
    }
    for (let c = 0; c < cells; c++) {
      cx[c] /= cnt[c];
      cy[c] /= cnt[c];
    }
  }
}

/** The layout members' indices into the simulation's node array. */
function memberIndices<N extends F32Node>(
  nodes: N[],
  members: ReadonlySet<string>
): Int32Array<ArrayBuffer> {
  const list: number[] = [];
  for (let i = 0; i < nodes.length; i++) if (members.has(nodes[i].id)) list.push(i);
  return new Int32Array(list);
}

/**
 * d3's forceManyBody (theta 0.9, distanceMin 1), restricted to `members` at a
 * uniform `-repel` charge and run over flat f32 arrays.
 */
export function f32ManyBody<N extends F32Node>(
  members: ReadonlySet<string>,
  repel: number
): Force<N, undefined> {
  let nodes: N[] = [];
  let random: () => number = Math.random;
  let idx = new Int32Array(0);
  let px = new Float32Array(0);
  let py = new Float32Array(0);
  const tree = new F32Quadtree();
  const stackC = new Int32Array(STACK);
  const stackB = new Float64Array(STACK * 3); // x0, y0, size per entry

  const force: Force<N, undefined> = (alpha: number) => {
    const m = idx.length;
    if (m < 2 || repel === 0) return;
    for (let k = 0; k < m; k++) {
      const n = nodes[idx[k]];
      px[k] = n.x;
      py[k] = n.y;
    }
    tree.build(px, py, m);
    tree.aggregate(px, py);
    const { child, next, cx, cy, cnt } = tree;
    const value = -repel;
    for (let k = 0; k < m; k++) {
      const xi = px[k];
      const yi = py[k];
      let ax = 0;
      let ay = 0;
      stackC[0] = 0;
      stackB[0] = tree.x0;
      stackB[1] = tree.y0;
      stackB[2] = tree.size;
      let sp = 1;
      while (sp > 0) {
        sp--;
        const c = stackC[sp];
        const bx = stackB[3 * sp];
        const by = stackB[3 * sp + 1];
        const s = stackB[3 * sp + 2];
        let dx = cx[c] - xi;
        let dy = cy[c] - yi;
        let l = dx * dx + dy * dy;
        if ((s * s) / THETA2 < l) {
          // Far enough: the whole cell acts as one body at its centroid.
          // d3 jiggles each zero component independently, not just full
          // coincidence — an axis-aligned pair must still break collinearity.
          if (dx === 0) {
            dx = jiggle(random);
            l += dx * dx;
          }
          if (dy === 0) {
            dy = jiggle(random);
            l += dy * dy;
          }
          if (l < DISTANCE_MIN2) l = Math.sqrt(DISTANCE_MIN2 * l);
          const w = (cnt[c] * value * alpha) / l;
          ax += dx * w;
          ay += dy * w;
          continue;
        }
        const half = s / 2;
        for (let q = 0; q < 4; q++) {
          const v = child[4 * c + q];
          if (v === 0) continue;
          if (v > 0) {
            stackC[sp] = v;
            stackB[3 * sp] = q & 1 ? bx + half : bx;
            stackB[3 * sp + 1] = q & 2 ? by + half : by;
            stackB[3 * sp + 2] = half;
            sp++;
          } else {
            for (let j = -v - 1; j >= 0; j = next[j]) {
              if (j === k) continue;
              let ex = px[j] - xi;
              let ey = py[j] - yi;
              let el = ex * ex + ey * ey;
              // Per-component like d3: zero x with nonzero y still jiggles x.
              if (ex === 0) {
                ex = jiggle(random);
                el += ex * ex;
              }
              if (ey === 0) {
                ey = jiggle(random);
                el += ey * ey;
              }
              if (el < DISTANCE_MIN2) el = Math.sqrt(DISTANCE_MIN2 * el);
              const w = (value * alpha) / el;
              ax += ex * w;
              ay += ey * w;
            }
          }
        }
      }
      const n = nodes[idx[k]];
      n.vx = (n.vx ?? 0) + ax;
      n.vy = (n.vy ?? 0) + ay;
    }
  };
  force.initialize = (simNodes, rand) => {
    nodes = simNodes;
    if (rand) random = rand;
    idx = memberIndices(nodes, members);
    px = new Float32Array(idx.length);
    py = new Float32Array(idx.length);
  };
  return force;
}

/**
 * d3's forceCollide (strength 1, one iteration), restricted to `members` at
 * one uniform `radius` and run over flat f32 arrays. Uniform radii mean every
 * overlap correction splits 50/50 and the prune distance is a constant.
 * ponytail: per-node radii would need a max-radius aggregate per cell — add it
 * if node sizes ever diverge.
 */
export function f32Collide<N extends F32Node>(
  members: ReadonlySet<string>,
  radius: number
): Force<N, undefined> {
  let nodes: N[] = [];
  let random: () => number = Math.random;
  let idx = new Int32Array(0);
  let posx = new Float32Array(0);
  let posy = new Float32Array(0);
  let velx = new Float32Array(0);
  let vely = new Float32Array(0);
  let predx = new Float32Array(0);
  let predy = new Float32Array(0);
  const tree = new F32Quadtree();
  const stackC = new Int32Array(STACK);
  const stackB = new Float64Array(STACK * 3);

  const force: Force<N, undefined> = () => {
    const m = idx.length;
    if (m < 2 || radius <= 0) return;
    for (let k = 0; k < m; k++) {
      const n = nodes[idx[k]];
      posx[k] = n.x;
      posy[k] = n.y;
      velx[k] = n.vx ?? 0;
      vely[k] = n.vy ?? 0;
      predx[k] = posx[k] + velx[k];
      predy[k] = posy[k] + vely[k];
    }
    tree.build(predx, predy, m);
    const { child, next } = tree;
    const r = 2 * radius;
    const r2 = r * r;
    for (let k = 0; k < m; k++) {
      // Predicted position, re-read so corrections from earlier nodes count;
      // the tree's geometry stays as built, exactly as d3's does mid-pass.
      const xi = posx[k] + velx[k];
      const yi = posy[k] + vely[k];
      stackC[0] = 0;
      stackB[0] = tree.x0;
      stackB[1] = tree.y0;
      stackB[2] = tree.size;
      let sp = 1;
      while (sp > 0) {
        sp--;
        const c = stackC[sp];
        const bx = stackB[3 * sp];
        const by = stackB[3 * sp + 1];
        const s = stackB[3 * sp + 2];
        if (bx > xi + r || bx + s < xi - r || by > yi + r || by + s < yi - r) continue;
        const half = s / 2;
        for (let q = 0; q < 4; q++) {
          const v = child[4 * c + q];
          if (v === 0) continue;
          if (v > 0) {
            stackC[sp] = v;
            stackB[3 * sp] = q & 1 ? bx + half : bx;
            stackB[3 * sp + 1] = q & 2 ? by + half : by;
            stackB[3 * sp + 2] = half;
            sp++;
          } else {
            // Each pair once (j > k), corrected symmetrically like d3 —
            // except the whole coincident chain is resolved, where d3 only
            // ever touches the chain head.
            for (let j = -v - 1; j >= 0; j = next[j]) {
              if (j <= k) continue;
              let ex = xi - posx[j] - velx[j];
              let ey = yi - posy[j] - vely[j];
              let l = ex * ex + ey * ey;
              if (l < r2) {
                if (ex === 0) {
                  ex = jiggle(random);
                  l += ex * ex;
                }
                if (ey === 0) {
                  ey = jiggle(random);
                  l += ey * ey;
                }
                l = Math.sqrt(l);
                const w = ((r - l) / l) * 0.5;
                ex *= w;
                ey *= w;
                velx[k] += ex;
                vely[k] += ey;
                velx[j] -= ex;
                vely[j] -= ey;
              }
            }
          }
        }
      }
    }
    for (let k = 0; k < m; k++) {
      const n = nodes[idx[k]];
      n.vx = velx[k];
      n.vy = vely[k];
    }
  };
  force.initialize = (simNodes, rand) => {
    nodes = simNodes;
    if (rand) random = rand;
    idx = memberIndices(nodes, members);
    posx = new Float32Array(idx.length);
    posy = new Float32Array(idx.length);
    velx = new Float32Array(idx.length);
    vely = new Float32Array(idx.length);
    predx = new Float32Array(idx.length);
    predy = new Float32Array(idx.length);
  };
  return force;
}
