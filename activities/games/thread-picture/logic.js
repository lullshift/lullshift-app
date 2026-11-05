// Pure game logic for Thread Picture. No DOM access. Attaches its API to globalThis.ThreadLogic.
// A cell is 0 (unknown), 1 (filled) or 2 (marked empty). The hidden picture is an array of 0/1.
(function (root) {
  "use strict";
  var SIZES = [5, 10];
  var HISTORY_LIMIT = 400;
  var MAX_ATTEMPTS = 4000;

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

  // The same puzzle for everyone on a local date; each size gets its own.
  function dateSeed(d, size) {
    var ymd = d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();
    return (Math.imul(ymd, 2654435761) ^ 0x54687264 ^ Math.imul(size | 0, 40503)) >>> 0;
  }

  function isSize(n) {
    return SIZES.indexOf(n) !== -1;
  }

  // Lengths of the runs of filled cells in a line of 0/1 (or cell states, where only 1 counts).
  function runsOf(line) {
    var out = [];
    var run = 0;
    for (var i = 0; i < line.length; i++) {
      if (line[i] === 1) run += 1;
      else if (run > 0) {
        out.push(run);
        run = 0;
      }
    }
    if (run > 0) out.push(run);
    return out;
  }

  function rowOf(cells, size, y) {
    return cells.slice(y * size, y * size + size);
  }

  function colOf(cells, size, x) {
    var out = [];
    for (var y = 0; y < size; y++) out.push(cells[y * size + x]);
    return out;
  }

  // Clues are lists of run lengths; an empty line is [0].
  function cluesOf(picture, size) {
    var rows = [];
    var cols = [];
    for (var i = 0; i < size; i++) {
      var r = runsOf(rowOf(picture, size, i));
      var c = runsOf(colOf(picture, size, i));
      rows.push(r.length ? r : [0]);
      cols.push(c.length ? c : [0]);
    }
    return { rows: rows, cols: cols };
  }

  function sameList(a, b) {
    if (a.length !== b.length) return false;
    for (var i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
    return true;
  }

  function normClue(clue) {
    return clue.length === 1 && clue[0] === 0 ? [] : clue;
  }

  // Line solver for one line. `line` holds 0 unknown, 1 filled, 2 empty. Looks at every way the
  // clue can fit without contradicting what is known, and returns the line with every cell that
  // is the same in all of them decided (1 or 2), or null when no arrangement fits.
  function solveLine(clue, line) {
    var n = line.length;
    var blocks = normClue(clue);
    var canFill = [];
    var canEmpty = [];
    for (var k = 0; k < n; k++) {
      canFill.push(false);
      canEmpty.push(false);
    }
    var found = false;
    var cur = [];
    for (var z = 0; z < n; z++) cur.push(0);

    // minTail[b] is the room blocks b.. need, with a single gap between them.
    var minTail = [];
    var acc = 0;
    for (var b = blocks.length - 1; b >= 0; b--) {
      acc += blocks[b] + (b < blocks.length - 1 ? 1 : 0);
      minTail[b] = acc;
    }

    function record() {
      found = true;
      for (var i = 0; i < n; i++) {
        if (cur[i] === 1) canFill[i] = true;
        else canEmpty[i] = true;
      }
    }

    function place(bi, pos) {
      if (bi === blocks.length) {
        for (var j = pos; j < n; j++) if (line[j] === 1) return;
        for (var m = pos; m < n; m++) cur[m] = 0;
        record();
        return;
      }
      var len = blocks[bi];
      var last = n - minTail[bi];
      for (var start = pos; start <= last; start++) {
        // Cells between pos and start stay empty, so none of them may be known filled.
        if (start > pos && line[start - 1] === 1) break;
        var ok = true;
        for (var t = start; t < start + len; t++) {
          if (line[t] === 2) {
            ok = false;
            break;
          }
        }
        if (!ok) continue;
        var after = start + len;
        if (after < n && line[after] === 1) continue;
        for (var e = pos; e < start; e++) cur[e] = 0;
        for (var f = start; f < after; f++) cur[f] = 1;
        if (after < n) cur[after] = 0;
        place(bi + 1, after + 1);
      }
    }

    place(0, 0);
    if (!found) return null;
    var out = [];
    for (var i = 0; i < n; i++) {
      if (canFill[i] && !canEmpty[i]) out.push(1);
      else if (!canFill[i] && canEmpty[i]) out.push(2);
      else out.push(line[i]);
    }
    return out;
  }

  // Repeats the line solver over every row and column until nothing more is decided.
  // Returns { cells, solved, contradiction }.
  function lineSolve(size, rows, cols) {
    var cells = [];
    for (var i = 0; i < size * size; i++) cells.push(0);
    var changed = true;
    while (changed) {
      changed = false;
      for (var pass = 0; pass < 2; pass++) {
        for (var k = 0; k < size; k++) {
          var line = pass === 0 ? rowOf(cells, size, k) : colOf(cells, size, k);
          var res = solveLine(pass === 0 ? rows[k] : cols[k], line);
          if (res === null) return { cells: cells, solved: false, contradiction: true };
          for (var j = 0; j < size; j++) {
            if (res[j] !== line[j]) {
              var idx = pass === 0 ? k * size + j : j * size + k;
              cells[idx] = res[j];
              changed = true;
            }
          }
        }
      }
    }
    var solved = true;
    for (var q = 0; q < cells.length; q++) if (cells[q] === 0) solved = false;
    return { cells: cells, solved: solved, contradiction: false };
  }

  function filledTotal(clues) {
    var t = 0;
    for (var i = 0; i < clues.rows.length; i++) {
      for (var j = 0; j < clues.rows[i].length; j++) t += clues.rows[i][j];
    }
    return t;
  }

  // A seeded picture. It is kept only when the line solver alone finishes it, which also proves
  // it has exactly one solution. Deterministic: the same seed and size give the same puzzle.
  function generate(seed, size) {
    if (!isSize(size)) throw new RangeError("unsupported size");
    var rng = mulberry32(seed);
    var total = size * size;
    for (var attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
      var density = 0.5 + rng() * 0.15;
      var picture = [];
      var count = 0;
      for (var i = 0; i < total; i++) {
        var on = rng() < density ? 1 : 0;
        picture.push(on);
        count += on;
      }
      if (count < total * 0.4 || count > total * 0.7) continue;
      var clues = cluesOf(picture, size);
      var empty = false;
      for (var k = 0; k < size; k++) {
        if (clues.rows[k][0] === 0 || clues.cols[k][0] === 0) empty = true;
      }
      if (empty) continue;
      if (lineSolve(size, clues.rows, clues.cols).solved) {
        return {
          size: size,
          seed: seed,
          picture: picture,
          rows: clues.rows,
          cols: clues.cols,
          total: filledTotal(clues),
          attempts: attempt,
        };
      }
    }
    throw new Error("no puzzle found");
  }

  function blankCells(size) {
    var out = [];
    for (var i = 0; i < size * size; i++) out.push(0);
    return out;
  }

  // Fill an unknown or marked cell; clear a filled one.
  function toggleFill(cells, i) {
    var out = cells.slice();
    out[i] = out[i] === 1 ? 0 : 1;
    return out;
  }

  // Mark an unknown or filled cell as empty; clear a marked one.
  function toggleMark(cells, i) {
    var out = cells.slice();
    out[i] = out[i] === 2 ? 0 : 2;
    return out;
  }

  // Any set of filled cells that fits every clue is a win (the puzzle has one, but the check
  // does not rely on it). Marks are ignored.
  function isSolved(cells, size, rows, cols) {
    for (var k = 0; k < size; k++) {
      var r = runsOf(rowOf(cells, size, k));
      var c = runsOf(colOf(cells, size, k));
      if (!sameList(r, normClue(rows[k]))) return false;
      if (!sameList(c, normClue(cols[k]))) return false;
    }
    return true;
  }

  function filledCount(cells) {
    var n = 0;
    for (var i = 0; i < cells.length; i++) if (cells[i] === 1) n += 1;
    return n;
  }

  // Undo history: a list of { i, from } entries (the cell index and its earlier state).
  function pushMove(history, i, from, to) {
    var out = history.concat([{ i: i, from: from, to: to }]);
    return out.length > HISTORY_LIMIT ? out.slice(out.length - HISTORY_LIMIT) : out;
  }

  function undo(cells, history) {
    if (!history.length) return { cells: cells, history: history };
    var m = history[history.length - 1];
    var out = cells.slice();
    out[m.i] = m.from;
    return { cells: out, history: history.slice(0, -1) };
  }

  function isCellState(v) {
    return v === 0 || v === 1 || v === 2;
  }

  root.ThreadLogic = {
    SIZES: SIZES,
    HISTORY_LIMIT: HISTORY_LIMIT,
    dateSeed: dateSeed,
    isSize: isSize,
    runsOf: runsOf,
    rowOf: rowOf,
    colOf: colOf,
    cluesOf: cluesOf,
    solveLine: solveLine,
    lineSolve: lineSolve,
    generate: generate,
    blankCells: blankCells,
    toggleFill: toggleFill,
    toggleMark: toggleMark,
    isSolved: isSolved,
    filledCount: filledCount,
    pushMove: pushMove,
    undo: undo,
    isCellState: isCellState,
  };
})(globalThis);
