// Pure rules for Stone Slide. No DOM. Needs the grid kit (for the seeded generator) loaded first.
// Attaches its API to globalThis.StoneSlideLogic.
(function (root) {
  "use strict";
  var K = root.LullshiftKits && root.LullshiftKits.grid;
  if (!K) throw new Error("Stone Slide needs the Lullshift grid kit");

  var SIZES = [3, 4];

  // Tiles are numbers 1..n*n-1 in reading order; 0 is the gap. The solved board has the gap last.
  function solvedTiles(size) {
    var t = [];
    for (var i = 1; i < size * size; i++) t.push(i);
    t.push(0);
    return t;
  }

  function isSolved(tiles) {
    for (var i = 0; i < tiles.length - 1; i++) if (tiles[i] !== i + 1) return false;
    return tiles[tiles.length - 1] === 0;
  }

  function gapIndex(tiles) {
    return tiles.indexOf(0);
  }

  // True when cell `i` is directly beside the gap (not diagonal, not the gap itself).
  function canSlide(tiles, size, i) {
    if (!Number.isInteger(i) || i < 0 || i >= tiles.length || tiles[i] === 0) return false;
    var g = gapIndex(tiles);
    var dr = Math.abs(Math.floor(i / size) - Math.floor(g / size));
    var dc = Math.abs((i % size) - (g % size));
    return dr + dc === 1;
  }

  // Slides tile `i` into the gap. Returns a new array, or null when the move is illegal.
  function slide(tiles, size, i) {
    if (!canSlide(tiles, size, i)) return null;
    var g = gapIndex(tiles);
    var next = tiles.slice();
    next[g] = tiles[i];
    next[i] = 0;
    return next;
  }

  // The legal moves are the cells beside the gap.
  function legalMoves(tiles, size) {
    var out = [];
    for (var i = 0; i < tiles.length; i++) if (canSlide(tiles, size, i)) out.push(i);
    return out;
  }

  // Parity test for a standard sliding puzzle, used by tests to check solvability independently.
  function isSolvable(tiles, size) {
    var inv = 0;
    var flat = tiles.filter(function (v) {
      return v !== 0;
    });
    for (var a = 0; a < flat.length; a++)
      for (var b = a + 1; b < flat.length; b++) if (flat[a] > flat[b]) inv += 1;
    if (size % 2 === 1) return inv % 2 === 0;
    var rowFromBottom = size - Math.floor(gapIndex(tiles) / size);
    return (inv + rowFromBottom) % 2 === 1;
  }

  // Scrambles by random legal moves from the solved board, so the result is always solvable.
  // Never returns a solved board and never immediately reverses a move.
  function generate(size, seed) {
    var rng = K.createRng(seed);
    var steps = size === 3 ? 60 : 120;
    for (;;) {
      var tiles = solvedTiles(size);
      var last = -1;
      for (var n = 0; n < steps; n++) {
        var g = gapIndex(tiles);
        var options = legalMoves(tiles, size).filter(function (i) {
          return i !== last;
        });
        var pick = rng.pick(options);
        last = g;
        tiles = slide(tiles, size, pick);
      }
      if (!isSolved(tiles)) return tiles;
    }
  }

  // Undo: `stack` holds the gap position before each move. Undoing slides the tile that now sits
  // in that old gap position back into the gap.
  function push(stack, previousGap) {
    return stack.concat([previousGap]);
  }
  function undo(tiles, size, stack) {
    if (stack.length === 0) return { tiles: tiles, stack: stack };
    var back = slide(tiles, size, stack[stack.length - 1]);
    if (!back) return { tiles: tiles, stack: stack };
    return { tiles: back, stack: stack.slice(0, -1) };
  }

  // Checks a saved tile list: a permutation of 0..n*n-1.
  function validTiles(tiles, size) {
    if (!Array.isArray(tiles) || tiles.length !== size * size) return false;
    var seen = {};
    for (var i = 0; i < tiles.length; i++) {
      var v = tiles[i];
      if (!Number.isInteger(v) || v < 0 || v >= tiles.length || seen[v]) return false;
      seen[v] = true;
    }
    return true;
  }

  function dailySeed(size, date) {
    return K.dailySeed(date, "stone-slide-" + size);
  }

  root.StoneSlideLogic = {
    SIZES: SIZES,
    solvedTiles: solvedTiles,
    isSolved: isSolved,
    gapIndex: gapIndex,
    canSlide: canSlide,
    slide: slide,
    legalMoves: legalMoves,
    isSolvable: isSolvable,
    generate: generate,
    push: push,
    undo: undo,
    validTiles: validTiles,
    dailySeed: dailySeed,
  };
})(globalThis);
