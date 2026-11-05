// Pure rules for Petal Grid, a Latin-square puzzle. No DOM. Needs the grid kit (seeded generator)
// loaded first. Attaches its API to globalThis.PetalGridLogic.
//
// Cells are numbers: 0 is empty, 1..size is a petal kind. A puzzle is
//   { size, seed, solution: number[], givens: number[] }
// where givens has 0 for blank cells. The solution is unique (checked by a solver).
(function (root) {
  "use strict";
  var K = root.LullshiftKits && root.LullshiftKits.grid;
  if (!K) throw new Error("Petal Grid needs the Lullshift grid kit");

  var SIZES = [4, 6];
  var NAMES = ["rose", "iris", "tulip", "daisy", "lily", "violet"];
  // Keep at least this many givens so the puzzle stays friendly.
  var MIN_GIVENS = { 4: 7, 6: 15 };

  function petalName(kind) {
    return NAMES[kind - 1];
  }

  // Row r, column c of a cell index.
  function rowOf(size, i) {
    return Math.floor(i / size);
  }
  function colOf(size, i) {
    return i % size;
  }

  // Counts solutions of a (partial) grid, stopping at `limit`. Backtracking with the most
  // constrained cell first.
  function countSolutions(size, cells, limit) {
    var grid = cells.slice();
    var rowUsed = [];
    var colUsed = [];
    var r;
    var c;
    for (r = 0; r < size; r++) {
      rowUsed.push(0);
      colUsed.push(0);
    }
    for (var i = 0; i < grid.length; i++) {
      var v = grid[i];
      if (v === 0) continue;
      var bit = 1 << v;
      r = rowOf(size, i);
      c = colOf(size, i);
      if (rowUsed[r] & bit || colUsed[c] & bit) return 0;
      rowUsed[r] |= bit;
      colUsed[c] |= bit;
    }
    var found = 0;
    function search() {
      var bestCell = -1;
      var bestMask = 0;
      var bestCount = size + 1;
      for (var k = 0; k < grid.length; k++) {
        if (grid[k] !== 0) continue;
        var mask = 0;
        var count = 0;
        var used = rowUsed[rowOf(size, k)] | colUsed[colOf(size, k)];
        for (var v2 = 1; v2 <= size; v2++) {
          if (!(used & (1 << v2))) {
            mask |= 1 << v2;
            count += 1;
          }
        }
        if (count < bestCount) {
          bestCount = count;
          bestCell = k;
          bestMask = mask;
          if (count === 0) return;
        }
      }
      if (bestCell === -1) {
        found += 1;
        return;
      }
      var br = rowOf(size, bestCell);
      var bc = colOf(size, bestCell);
      for (var val = 1; val <= size && found < limit; val++) {
        var b = 1 << val;
        if (!(bestMask & b)) continue;
        grid[bestCell] = val;
        rowUsed[br] |= b;
        colUsed[bc] |= b;
        search();
        rowUsed[br] &= ~b;
        colUsed[bc] &= ~b;
        grid[bestCell] = 0;
      }
    }
    search();
    return found;
  }

  // A full Latin square from a seed: a cyclic square with its rows, columns and symbols shuffled.
  function fullSquare(size, rng) {
    var rows = rng.shuffle(range(size));
    var cols = rng.shuffle(range(size));
    var symbols = rng.shuffle(range(size));
    var out = [];
    for (var r = 0; r < size; r++)
      for (var c = 0; c < size; c++) out.push(symbols[(rows[r] + cols[c]) % size] + 1);
    return out;
  }

  function range(n) {
    var a = [];
    for (var i = 0; i < n; i++) a.push(i);
    return a;
  }

  // Removes givens one at a time in a seeded order, keeping a removal only while the solution
  // stays unique, until MIN_GIVENS is reached or nothing more can go.
  function generate(size, seed) {
    var rng = K.createRng(seed);
    var solution = fullSquare(size, rng);
    var givens = solution.slice();
    var order = rng.shuffle(range(size * size));
    var left = givens.length;
    for (var k = 0; k < order.length && left > MIN_GIVENS[size]; k++) {
      var i = order[k];
      var kept = givens[i];
      givens[i] = 0;
      if (countSolutions(size, givens, 2) === 1) left -= 1;
      else givens[i] = kept;
    }
    return { size: size, seed: seed, solution: solution, givens: givens };
  }

  // Indexes of the cells in a conflict: a petal kind that appears twice in its row or column.
  function conflicts(size, cells) {
    var bad = {};
    var i;
    var j;
    for (i = 0; i < cells.length; i++) {
      if (cells[i] === 0) continue;
      for (j = i + 1; j < cells.length; j++) {
        if (cells[j] !== cells[i]) continue;
        if (rowOf(size, i) === rowOf(size, j) || colOf(size, i) === colOf(size, j)) {
          bad[i] = true;
          bad[j] = true;
        }
      }
    }
    return Object.keys(bad)
      .map(Number)
      .sort(function (a, b) {
        return a - b;
      });
  }

  function filledCount(cells) {
    var n = 0;
    for (var i = 0; i < cells.length; i++) if (cells[i] !== 0) n += 1;
    return n;
  }

  // Solved: every cell filled and no conflicts. With unique givens this is the solution.
  function isSolved(size, cells) {
    return filledCount(cells) === size * size && conflicts(size, cells).length === 0;
  }

  // Places `value` (0 clears) in cell `i`. Returns a new array, or null for a fixed given, a bad
  // index, a bad value or no change.
  function place(puzzle, cells, i, value) {
    var n = puzzle.size;
    if (!Number.isInteger(i) || i < 0 || i >= n * n) return null;
    if (!Number.isInteger(value) || value < 0 || value > n) return null;
    if (puzzle.givens[i] !== 0) return null;
    if (cells[i] === value) return null;
    var next = cells.slice();
    next[i] = value;
    return next;
  }

  // Undo: the stack holds { i, prev } entries.
  function push(stack, i, prev) {
    return stack.concat([{ i: i, prev: prev }]);
  }
  function undo(cells, stack) {
    if (stack.length === 0) return { cells: cells, stack: stack };
    var top = stack[stack.length - 1];
    var next = cells.slice();
    next[top.i] = top.prev;
    return { cells: next, stack: stack.slice(0, -1) };
  }

  function startCells(puzzle) {
    return puzzle.givens.slice();
  }

  // Shape check for restored data.
  function validPuzzle(p) {
    if (!K.isRecord(p) || SIZES.indexOf(p.size) < 0) return false;
    var n = p.size * p.size;
    if (!Array.isArray(p.solution) || !Array.isArray(p.givens)) return false;
    if (p.solution.length !== n || p.givens.length !== n) return false;
    if (!K.isIntIn(p.seed, 0, 4294967295)) return false;
    for (var i = 0; i < n; i++) {
      if (!K.isIntIn(p.solution[i], 1, p.size)) return false;
      if (!K.isIntIn(p.givens[i], 0, p.size)) return false;
      if (p.givens[i] !== 0 && p.givens[i] !== p.solution[i]) return false;
    }
    return conflicts(p.size, p.solution).length === 0;
  }
  function validCells(puzzle, cells) {
    var n = puzzle.size * puzzle.size;
    if (!Array.isArray(cells) || cells.length !== n) return false;
    for (var i = 0; i < n; i++) {
      if (!K.isIntIn(cells[i], 0, puzzle.size)) return false;
      if (puzzle.givens[i] !== 0 && cells[i] !== puzzle.givens[i]) return false;
    }
    return true;
  }

  function dailySeed(size, date) {
    return K.dailySeed(date, "petal-grid-" + size);
  }

  root.PetalGridLogic = {
    SIZES: SIZES,
    NAMES: NAMES,
    MIN_GIVENS: MIN_GIVENS,
    petalName: petalName,
    countSolutions: countSolutions,
    generate: generate,
    conflicts: conflicts,
    filledCount: filledCount,
    isSolved: isSolved,
    place: place,
    push: push,
    undo: undo,
    startCells: startCells,
    validPuzzle: validPuzzle,
    validCells: validCells,
    dailySeed: dailySeed,
  };
})(globalThis);
