// Pure game logic for Island Bridges. No DOM access. Attaches its API to globalThis.IslandLogic.
// Islands are numbered cells (listed top to bottom, left to right). A bridge joins two islands that
// face each other in a straight line with no island between and at least one cell of water in
// between. bridges is a list of [a, b, count] with a < b (island indexes) and count 1 or 2.
(function (root) {
  "use strict";
  var SIZES = [7, 9];
  var HISTORY_LIMIT = 300;
  var DIRS = {
    up: { dx: 0, dy: -1, word: "north" },
    right: { dx: 1, dy: 0, word: "east" },
    down: { dx: 0, dy: 1, word: "south" },
    left: { dx: -1, dy: 0, word: "west" },
  };
  var DIR_NAMES = ["up", "right", "down", "left"];

  // Small seeded PRNG (mulberry32). Returns a function giving floats in [0, 1).
  function mulberry32(seed) {
    var a = seed >>> 0;
    return function () {
      a = (a + 0x6d2b79f5) >>> 0;
      var t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  function dateSeed(d, size) {
    var ymd = d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();
    return (Math.imul(ymd, 2654435761) ^ 0x49736c64 ^ Math.imul(size | 0, 40503)) >>> 0;
  }

  function isSize(n) {
    return SIZES.indexOf(n) !== -1;
  }

  function normPair(a, b) {
    return a < b ? [a, b] : [b, a];
  }

  // Generates a connected layout by growing it island by island, then adds a few extra bridges
  // between islands that already face each other. Only the island numbers are kept for play, but
  // the layout is stored as one valid answer. Different answers are accepted by isSolved.
  function generate(seed, size) {
    if (!isSize(size)) throw new RangeError("unsupported size");
    var rng = mulberry32(seed);
    var target = size === 7 ? 8 + Math.floor(rng() * 4) : 13 + Math.floor(rng() * 5);
    var occ = [];
    for (var i = 0; i < size * size; i++) occ.push(0); // 0 water, 1 island, 2 bridge
    var raw = [];
    var links = []; // { a, b, c } with raw indexes
    var first = Math.floor(rng() * size * size);
    occ[first] = 1;
    raw.push(first);

    function weight() {
      return rng() < 0.35 ? 2 : 1;
    }

    for (var attempt = 0; attempt < 3000 && raw.length < target; attempt++) {
      var s = Math.floor(rng() * raw.length);
      var dn = DIR_NAMES[Math.floor(rng() * 4)];
      var d = DIRS[dn];
      var sx = raw[s] % size;
      var sy = Math.floor(raw[s] / size);
      var maxLen = 0;
      while (true) {
        var nx = sx + d.dx * (maxLen + 1);
        var ny = sy + d.dy * (maxLen + 1);
        if (nx < 0 || ny < 0 || nx >= size || ny >= size) break;
        if (occ[ny * size + nx] !== 0) break;
        maxLen += 1;
      }
      if (maxLen < 2) continue;
      var len = 2 + Math.floor(rng() * Math.min(maxLen - 1, 4));
      var endCell = (sy + d.dy * len) * size + (sx + d.dx * len);
      for (var k = 1; k < len; k++) occ[(sy + d.dy * k) * size + (sx + d.dx * k)] = 2;
      occ[endCell] = 1;
      raw.push(endCell);
      links.push({ a: s, b: raw.length - 1, c: weight() });
    }

    // Extra bridges between islands that already face each other across empty water.
    for (var r = 0; r < raw.length; r++) {
      for (var di = 1; di <= 2; di++) {
        var dd = DIRS[DIR_NAMES[di]]; // right and down only, so each pair is seen once
        var x0 = raw[r] % size;
        var y0 = Math.floor(raw[r] / size);
        var step = 1;
        var clear = true;
        var hit = -1;
        while (true) {
          var xx = x0 + dd.dx * step;
          var yy = y0 + dd.dy * step;
          if (xx < 0 || yy < 0 || xx >= size || yy >= size) break;
          var cell = yy * size + xx;
          if (occ[cell] === 1) {
            hit = raw.indexOf(cell);
            break;
          }
          if (occ[cell] === 2) {
            clear = false;
            break;
          }
          step += 1;
        }
        if (hit === -1 || !clear || step < 2) continue;
        var exists = links.some(function (l) {
          return (l.a === r && l.b === hit) || (l.a === hit && l.b === r);
        });
        if (exists || rng() > 0.4) continue;
        for (var m = 1; m < step; m++) occ[(y0 + dd.dy * m) * size + (x0 + dd.dx * m)] = 2;
        links.push({ a: r, b: hit, c: weight() });
      }
    }

    // Sort islands top to bottom, left to right and number them by their bridges.
    var order = raw.map(function (_, idx) {
      return idx;
    });
    order.sort(function (p, q) {
      return raw[p] - raw[q];
    });
    var newIndex = [];
    order.forEach(function (oldIdx, nIdx) {
      newIndex[oldIdx] = nIdx;
    });
    var counts = raw.map(function () {
      return 0;
    });
    var solution = links.map(function (l) {
      counts[l.a] += l.c;
      counts[l.b] += l.c;
      var pr = normPair(newIndex[l.a], newIndex[l.b]);
      return [pr[0], pr[1], l.c];
    });
    solution.sort(function (p, q) {
      return p[0] - q[0] || p[1] - q[1];
    });
    var islands = order.map(function (oldIdx) {
      return { x: raw[oldIdx] % size, y: Math.floor(raw[oldIdx] / size), n: counts[oldIdx] };
    });
    return { size: size, seed: seed, islands: islands, solution: solution };
  }

  function islandAt(puzzle, x, y) {
    for (var i = 0; i < puzzle.islands.length; i++) {
      if (puzzle.islands[i].x === x && puzzle.islands[i].y === y) return i;
    }
    return -1;
  }

  // The first island you meet going from island i in direction dir, or -1.
  function neighbourIsland(puzzle, i, dir) {
    var d = DIRS[dir];
    if (!d) return -1;
    var x = puzzle.islands[i].x + d.dx;
    var y = puzzle.islands[i].y + d.dy;
    while (x >= 0 && y >= 0 && x < puzzle.size && y < puzzle.size) {
      var j = islandAt(puzzle, x, y);
      if (j !== -1) return j;
      x += d.dx;
      y += d.dy;
    }
    return -1;
  }

  function distance(puzzle, a, b) {
    var p = puzzle.islands[a];
    var q = puzzle.islands[b];
    return Math.abs(p.x - q.x) + Math.abs(p.y - q.y);
  }

  function orientation(puzzle, a, b) {
    return puzzle.islands[a].y === puzzle.islands[b].y ? "h" : "v";
  }

  function countBetween(bridges, a, b) {
    var pr = normPair(a, b);
    for (var i = 0; i < bridges.length; i++) {
      if (bridges[i][0] === pr[0] && bridges[i][1] === pr[1]) return bridges[i][2];
    }
    return 0;
  }

  function degree(bridges, i) {
    var n = 0;
    for (var k = 0; k < bridges.length; k++) {
      if (bridges[k][0] === i || bridges[k][1] === i) n += bridges[k][2];
    }
    return n;
  }

  // True when the straight bridge a-b would cross the straight bridge c-d.
  function segmentsCross(puzzle, a, b, c, d) {
    var oa = orientation(puzzle, a, b);
    if (oa === orientation(puzzle, c, d)) return false;
    var h = oa === "h" ? [a, b] : [c, d];
    var v = oa === "h" ? [c, d] : [a, b];
    var hy = puzzle.islands[h[0]].y;
    var hx1 = Math.min(puzzle.islands[h[0]].x, puzzle.islands[h[1]].x);
    var hx2 = Math.max(puzzle.islands[h[0]].x, puzzle.islands[h[1]].x);
    var vx = puzzle.islands[v[0]].x;
    var vy1 = Math.min(puzzle.islands[v[0]].y, puzzle.islands[v[1]].y);
    var vy2 = Math.max(puzzle.islands[v[0]].y, puzzle.islands[v[1]].y);
    return hx1 < vx && vx < hx2 && vy1 < hy && hy < vy2;
  }

  function crossesAny(puzzle, bridges, a, b) {
    for (var i = 0; i < bridges.length; i++) {
      if (bridges[i][2] > 0 && segmentsCross(puzzle, a, b, bridges[i][0], bridges[i][1]))
        return true;
    }
    return false;
  }

  function cloneBridges(bridges) {
    return bridges.map(function (b) {
      return b.slice();
    });
  }

  function setCount(bridges, a, b, count) {
    var pr = normPair(a, b);
    var out = [];
    var done = false;
    for (var i = 0; i < bridges.length; i++) {
      if (bridges[i][0] === pr[0] && bridges[i][1] === pr[1]) {
        if (count > 0) out.push([pr[0], pr[1], count]);
        done = true;
      } else {
        out.push(bridges[i].slice());
      }
    }
    if (!done && count > 0) out.push([pr[0], pr[1], count]);
    out.sort(function (p, q) {
      return p[0] - q[0] || p[1] - q[1];
    });
    return out;
  }

  // Adds one bridge from island i toward the next island in direction dir.
  // Returns { ok, bridges, j, from, to, reason }.
  function addBridge(puzzle, bridges, i, dir) {
    var j = neighbourIsland(puzzle, i, dir);
    if (j === -1) return { ok: false, reason: "no island" };
    if (distance(puzzle, i, j) < 2) return { ok: false, reason: "too close", j: j };
    var have = countBetween(bridges, i, j);
    if (have >= 2) return { ok: false, reason: "two already", j: j };
    if (degree(bridges, i) >= puzzle.islands[i].n) return { ok: false, reason: "full", j: i };
    if (degree(bridges, j) >= puzzle.islands[j].n) return { ok: false, reason: "full", j: j };
    if (have === 0 && crossesAny(puzzle, bridges, i, j)) {
      return { ok: false, reason: "crossed", j: j };
    }
    return { ok: true, bridges: setCount(bridges, i, j, have + 1), j: j, from: have, to: have + 1 };
  }

  function removeBridge(puzzle, bridges, i, dir) {
    var j = neighbourIsland(puzzle, i, dir);
    if (j === -1) return { ok: false, reason: "no island" };
    var have = countBetween(bridges, i, j);
    if (have === 0) return { ok: false, reason: "none", j: j };
    return { ok: true, bridges: setCount(bridges, i, j, have - 1), j: j, from: have, to: have - 1 };
  }

  // Number of connected groups of islands joined by bridges.
  function groupCount(puzzle, bridges) {
    var n = puzzle.islands.length;
    var parent = [];
    for (var i = 0; i < n; i++) parent.push(i);
    function find(x) {
      while (parent[x] !== x) {
        parent[x] = parent[parent[x]];
        x = parent[x];
      }
      return x;
    }
    var groups = n;
    bridges.forEach(function (b) {
      if (b[2] > 0) {
        var ra = find(b[0]);
        var rb = find(b[1]);
        if (ra !== rb) {
          parent[ra] = rb;
          groups -= 1;
        }
      }
    });
    return groups;
  }

  function doneCount(puzzle, bridges) {
    var n = 0;
    for (var i = 0; i < puzzle.islands.length; i++) {
      if (degree(bridges, i) === puzzle.islands[i].n) n += 1;
    }
    return n;
  }

  // Untrusted saved bridges: shape and geometry only (rules are checked by isSolved).
  function validBridges(puzzle, bridges) {
    if (!Array.isArray(bridges) || bridges.length > puzzle.islands.length * 4) return false;
    var seen = {};
    for (var k = 0; k < bridges.length; k++) {
      var b = bridges[k];
      if (!Array.isArray(b) || b.length !== 3) return false;
      var a = b[0];
      var c = b[1];
      if (!Number.isInteger(a) || !Number.isInteger(c) || a < 0 || c >= puzzle.islands.length) {
        return false;
      }
      if (a >= c || (b[2] !== 1 && b[2] !== 2)) return false;
      var p = puzzle.islands[a];
      var q = puzzle.islands[c];
      if (p.x !== q.x && p.y !== q.y) return false;
      var dir = p.x === q.x ? "down" : "right";
      if (neighbourIsland(puzzle, a, dir) !== c) return false;
      if (distance(puzzle, a, c) < 2) return false;
      var key = a + "-" + c;
      if (seen[key]) return false;
      seen[key] = true;
    }
    return true;
  }

  // The win: every island has exactly its number of bridges, no bridges cross, and all islands
  // are one group. Any layout meeting these rules wins, not only the generator's.
  function isSolved(puzzle, bridges) {
    if (!validBridges(puzzle, bridges)) return false;
    for (var i = 0; i < puzzle.islands.length; i++) {
      if (degree(bridges, i) !== puzzle.islands[i].n) return false;
    }
    for (var x = 0; x < bridges.length; x++) {
      for (var y = x + 1; y < bridges.length; y++) {
        if (segmentsCross(puzzle, bridges[x][0], bridges[x][1], bridges[y][0], bridges[y][1])) {
          return false;
        }
      }
    }
    return groupCount(puzzle, bridges) === 1;
  }

  // For drawing: for each cell, null or { o: "h" | "v", c: 1 | 2, a, b } if a bridge runs over it.
  function cellBridges(puzzle, bridges) {
    var size = puzzle.size;
    var out = [];
    for (var i = 0; i < size * size; i++) out.push(null);
    bridges.forEach(function (b) {
      var p = puzzle.islands[b[0]];
      var q = puzzle.islands[b[1]];
      if (p.y === q.y) {
        for (var x = p.x + 1; x < q.x; x++)
          out[p.y * size + x] = { o: "h", c: b[2], a: b[0], b: b[1] };
      } else {
        for (var y = p.y + 1; y < q.y; y++)
          out[y * size + p.x] = { o: "v", c: b[2], a: b[0], b: b[1] };
      }
    });
    return out;
  }

  // Undo history: { a, b, from } entries (the pair and its earlier count).
  function pushMove(history, a, b, from, to) {
    var pr = normPair(a, b);
    var out = history.concat([{ a: pr[0], b: pr[1], from: from, to: to }]);
    return out.length > HISTORY_LIMIT ? out.slice(out.length - HISTORY_LIMIT) : out;
  }

  function undo(bridges, history) {
    if (!history.length) return { bridges: bridges, history: history };
    var m = history[history.length - 1];
    return { bridges: setCount(bridges, m.a, m.b, m.from), history: history.slice(0, -1) };
  }

  root.IslandLogic = {
    SIZES: SIZES,
    HISTORY_LIMIT: HISTORY_LIMIT,
    DIRS: DIRS,
    dateSeed: dateSeed,
    isSize: isSize,
    generate: generate,
    islandAt: islandAt,
    neighbourIsland: neighbourIsland,
    countBetween: countBetween,
    degree: degree,
    addBridge: addBridge,
    removeBridge: removeBridge,
    groupCount: groupCount,
    doneCount: doneCount,
    validBridges: validBridges,
    isSolved: isSolved,
    cellBridges: cellBridges,
    cloneBridges: cloneBridges,
    pushMove: pushMove,
    undo: undo,
  };
})(globalThis);
