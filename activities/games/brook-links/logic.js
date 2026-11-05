// Pure game logic for Brook Links. No DOM access. Attaches its API to globalThis.BrookLogic.
// Cells are indexes (y * size + x). A puzzle has pairs of matching stones; the player draws one
// stream per pair. paths[c] is the list of cells of stream c, starting at one of its stones.
(function (root) {
  "use strict";
  var SIZES = [5, 6, 7];
  var HISTORY_LIMIT = 200;
  var NAMES = ["amber", "berry", "clover", "dusk", "ember", "fern", "glacier", "heather"];
  var LETTERS = ["A", "B", "C", "D", "E", "F", "G", "H"];
  var DIRS = {
    up: { dx: 0, dy: -1, word: "north" },
    right: { dx: 1, dy: 0, word: "east" },
    down: { dx: 0, dy: 1, word: "south" },
    left: { dx: -1, dy: 0, word: "west" },
  };

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
    return (Math.imul(ymd, 2654435761) ^ 0x42726f6f ^ Math.imul(size | 0, 40503)) >>> 0;
  }

  function isSize(n) {
    return SIZES.indexOf(n) !== -1;
  }

  function adjacent(a, b, size) {
    var ax = a % size;
    var ay = Math.floor(a / size);
    var bx = b % size;
    var by = Math.floor(b / size);
    return Math.abs(ax - bx) + Math.abs(ay - by) === 1;
  }

  function neighbours(i, size) {
    var x = i % size;
    var y = Math.floor(i / size);
    var out = [];
    if (y > 0) out.push(i - size);
    if (x < size - 1) out.push(i + 1);
    if (y < size - 1) out.push(i + size);
    if (x > 0) out.push(i - 1);
    return out;
  }

  // A random path through every cell, made from a snake by "backbite" moves: join an end to a
  // neighbouring cell and reverse the tail. Every step keeps a valid path through all cells.
  function randomFullPath(size, rng) {
    var n = size * size;
    var p = [];
    for (var y = 0; y < size; y++) {
      for (var x = 0; x < size; x++) p.push(y % 2 === 0 ? y * size + x : y * size + (size - 1 - x));
    }
    var steps = n * 40;
    for (var s = 0; s < steps; s++) {
      if (rng() < 0.5) p.reverse();
      var end = p[n - 1];
      var prev = p[n - 2];
      var opts = neighbours(end, size).filter(function (c) {
        return c !== prev;
      });
      if (!opts.length) continue;
      var pick = opts[Math.floor(rng() * opts.length)];
      var j = p.indexOf(pick);
      p = p.slice(0, j + 1).concat(p.slice(j + 1).reverse());
    }
    return p;
  }

  // Cuts the full path into k runs of at least 3 cells; each run's two ends become a pair of
  // stones. The runs are the intended streams, so the puzzle always has a full-garden answer.
  function generate(seed, size) {
    if (!isSize(size)) throw new RangeError("unsupported size");
    var rng = mulberry32(seed);
    var n = size * size;
    var kMin = size === 5 ? 4 : size === 6 ? 5 : 6;
    var kMax = size === 5 ? 5 : size === 6 ? 6 : 8;
    var k = kMin + Math.floor(rng() * (kMax - kMin + 1));
    var lens = [];
    for (var i = 0; i < k; i++) lens.push(3);
    for (var rest = n - 3 * k; rest > 0; rest--) lens[Math.floor(rng() * k)] += 1;
    var path = randomFullPath(size, rng);
    var stones = [];
    var solution = [];
    var at = 0;
    for (var c = 0; c < k; c++) {
      var run = path.slice(at, at + lens[c]);
      at += lens[c];
      solution.push(run);
      stones.push({ a: run[0], b: run[run.length - 1] });
    }
    return { size: size, seed: seed, stones: stones, solution: solution };
  }

  function blankPaths(puzzle) {
    return puzzle.stones.map(function () {
      return [];
    });
  }

  function clonePaths(paths) {
    return paths.map(function (p) {
      return p.slice();
    });
  }

  // Colour index at each cell (stones and stream cells), or -1.
  function owners(puzzle, paths) {
    var out = [];
    for (var i = 0; i < puzzle.size * puzzle.size; i++) out.push(-1);
    puzzle.stones.forEach(function (s, c) {
      out[s.a] = c;
      out[s.b] = c;
    });
    paths.forEach(function (p, c) {
      p.forEach(function (cell) {
        out[cell] = c;
      });
    });
    return out;
  }

  function stoneColor(puzzle, cell) {
    for (var c = 0; c < puzzle.stones.length; c++) {
      if (puzzle.stones[c].a === cell || puzzle.stones[c].b === cell) return c;
    }
    return -1;
  }

  function partner(puzzle, c, cell) {
    var s = puzzle.stones[c];
    return s.a === cell ? s.b : s.a;
  }

  function isLinked(puzzle, paths, c) {
    var p = paths[c];
    if (p.length < 2) return false;
    var s = puzzle.stones[c];
    var f = p[0];
    var l = p[p.length - 1];
    return (f === s.a && l === s.b) || (f === s.b && l === s.a);
  }

  function linkedCount(puzzle, paths) {
    var n = 0;
    for (var c = 0; c < puzzle.stones.length; c++) if (isLinked(puzzle, paths, c)) n += 1;
    return n;
  }

  function coveredCount(puzzle, paths) {
    var o = owners(puzzle, paths);
    var n = 0;
    for (var i = 0; i < o.length; i++) if (o[i] !== -1) n += 1;
    return n;
  }

  // Independent check of a whole set of streams: each is a chain of neighbouring cells inside the
  // board that starts and ends on its own stones, and no cell is used twice or lies on another
  // colour's stone. Any such set wins, not only the generator's.
  function isSolved(puzzle, paths) {
    var size = puzzle.size;
    var seen = {};
    puzzle.stones.forEach(function (s) {
      seen[s.a] = "stone";
      seen[s.b] = "stone";
    });
    for (var c = 0; c < puzzle.stones.length; c++) {
      var p = paths[c];
      if (!p || !isLinked(puzzle, paths, c)) return false;
      for (var i = 0; i < p.length; i++) {
        var cell = p[i];
        if (!Number.isInteger(cell) || cell < 0 || cell >= size * size) return false;
        if (i > 0 && !adjacent(p[i - 1], cell, size)) return false;
        var ends = i === 0 || i === p.length - 1;
        if (ends) {
          if (stoneColor(puzzle, cell) !== c) return false;
        } else {
          if (seen[cell] === "stone" || seen[cell] === "path") return false;
        }
        if (!ends) seen[cell] = "path";
      }
      if (new Set(p).size !== p.length) return false;
    }
    return true;
  }

  function isFullGarden(puzzle, paths) {
    return isSolved(puzzle, paths) && coveredCount(puzzle, paths) === puzzle.size * puzzle.size;
  }

  // Begin drawing from a stone. Pressing either end of an existing stream keeps it, so the player
  // can carry on or retract from its head; any other stone starts that colour afresh.
  function startPath(puzzle, paths, cell) {
    var c = stoneColor(puzzle, cell);
    if (c === -1) return null;
    var out = clonePaths(paths);
    var cur = out[c];
    if (!(cur.length > 0 && (cur[0] === cell || cur[cur.length - 1] === cell))) out[c] = [cell];
    return { paths: out, color: c };
  }

  // Moves the head of stream c one step. Returns { ok, paths, event, reason }. event is "extended",
  // "linked" or "retracted". Streams may not cross, branch, or touch another colour's stone.
  function step(puzzle, paths, c, dir) {
    var d = DIRS[dir];
    var p = paths[c];
    if (!d || !p || p.length === 0) return { ok: false, reason: "not drawing" };
    var size = puzzle.size;
    var head = p[p.length - 1];
    var x = (head % size) + d.dx;
    var y = Math.floor(head / size) + d.dy;
    if (x < 0 || y < 0 || x >= size || y >= size) return { ok: false, reason: "edge" };
    var target = y * size + x;
    var out = clonePaths(paths);
    if (p.length >= 2 && target === p[p.length - 2]) {
      out[c] = p.slice(0, -1);
      return { ok: true, paths: out, event: "retracted" };
    }
    if (isLinked(puzzle, paths, c)) return { ok: false, reason: "already linked" };
    var o = owners(puzzle, paths);
    if (o[target] !== -1) {
      if (o[target] === c && target === partner(puzzle, c, p[0])) {
        out[c] = p.concat([target]);
        return { ok: true, paths: out, event: "linked" };
      }
      return { ok: false, reason: o[target] === c ? "own stream" : "another stream" };
    }
    out[c] = p.concat([target]);
    return { ok: true, paths: out, event: "extended" };
  }

  // Backspace: remove the last cell. A stream down to its stone is cleared.
  function retract(paths, c) {
    var p = paths[c];
    if (!p || p.length === 0) return { ok: false, paths: paths };
    var out = clonePaths(paths);
    out[c] = p.length <= 1 ? [] : p.slice(0, -1);
    return { ok: true, paths: out };
  }

  // Cut stream c back so it ends at `cell` (which must lie on it).
  function truncateTo(paths, c, cell) {
    var j = paths[c].indexOf(cell);
    if (j === -1) return { ok: false, paths: paths };
    var out = clonePaths(paths);
    out[c] = paths[c].slice(0, j + 1);
    return { ok: true, paths: out };
  }

  function clearStream(paths, c) {
    var out = clonePaths(paths);
    out[c] = [];
    return out;
  }

  function samePaths(a, b) {
    if (a.length !== b.length) return false;
    for (var c = 0; c < a.length; c++) {
      if (a[c].length !== b[c].length) return false;
      for (var i = 0; i < a[c].length; i++) if (a[c][i] !== b[c][i]) return false;
    }
    return true;
  }

  // Undo history: snapshots of all streams, one per stroke.
  function pushSnapshot(history, paths) {
    var out = history.concat([clonePaths(paths)]);
    return out.length > HISTORY_LIMIT ? out.slice(out.length - HISTORY_LIMIT) : out;
  }

  function undo(paths, history) {
    if (!history.length) return { paths: paths, history: history };
    return { paths: clonePaths(history[history.length - 1]), history: history.slice(0, -1) };
  }

  // Checks untrusted saved streams against a puzzle (shape and bounds only; rules are enforced
  // by isSolved and by play).
  function validPaths(puzzle, paths) {
    if (!Array.isArray(paths) || paths.length !== puzzle.stones.length) return false;
    var o = [];
    var cells = puzzle.size * puzzle.size;
    for (var i = 0; i < cells; i++) o.push(-1);
    for (var c = 0; c < paths.length; c++) {
      var p = paths[c];
      if (!Array.isArray(p) || p.length > cells) return false;
      for (var k = 0; k < p.length; k++) {
        var cell = p[k];
        if (!Number.isInteger(cell) || cell < 0 || cell >= cells) return false;
        if (o[cell] !== -1) return false;
        o[cell] = c;
        if (k > 0 && !adjacent(p[k - 1], cell, puzzle.size)) return false;
      }
      if (p.length > 0 && stoneColor(puzzle, p[0]) !== c) return false;
      for (var m = 1; m < p.length; m++) {
        var sc = stoneColor(puzzle, p[m]);
        if (sc !== -1 && !(sc === c && m === p.length - 1)) return false;
      }
    }
    return true;
  }

  root.BrookLogic = {
    SIZES: SIZES,
    HISTORY_LIMIT: HISTORY_LIMIT,
    NAMES: NAMES,
    LETTERS: LETTERS,
    DIRS: DIRS,
    dateSeed: dateSeed,
    isSize: isSize,
    adjacent: adjacent,
    generate: generate,
    blankPaths: blankPaths,
    clonePaths: clonePaths,
    owners: owners,
    stoneColor: stoneColor,
    partner: partner,
    isLinked: isLinked,
    linkedCount: linkedCount,
    coveredCount: coveredCount,
    isSolved: isSolved,
    isFullGarden: isFullGarden,
    startPath: startPath,
    step: step,
    retract: retract,
    truncateTo: truncateTo,
    clearStream: clearStream,
    samePaths: samePaths,
    pushSnapshot: pushSnapshot,
    undo: undo,
    validPaths: validPaths,
  };
})(globalThis);
