// Pure game logic for Moss Paths. No DOM access. Attaches its API to globalThis.MossLogic.
// A tile is just a 4-bit mask of the sides its stream touches: N=1, E=2, S=4, W=8.
(function (root) {
  "use strict";
  var SIZE = 6;
  var SIZES = [4, 6, 8];
  var N = 1;
  var E = 2;
  var S = 4;
  var W = 8;
  var DIRS = [
    { bit: N, dx: 0, dy: -1, opp: S },
    { bit: E, dx: 1, dy: 0, opp: W },
    { bit: S, dx: 0, dy: 1, opp: N },
    { bit: W, dx: -1, dy: 0, opp: E },
  ];

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

  // Turns a tile a quarter turn: clockwise by default, counter-clockwise when cw is false.
  function rotate(mask, cw) {
    if (cw === false) return ((mask >> 1) | ((mask & 1) << 3)) & 15;
    return ((mask << 1) | (mask >> 3)) & 15;
  }

  function bitCount(mask) {
    var n = 0;
    for (var i = 0; i < 4; i++) if (mask & (1 << i)) n += 1;
    return n;
  }

  // "end", "straight", "bend", "tee" or "cross".
  function pieceType(mask) {
    var n = bitCount(mask);
    if (n === 1) return "end";
    if (n === 3) return "tee";
    if (n === 4) return "cross";
    if (n === 2) return mask === (N | S) || mask === (E | W) ? "straight" : "bend";
    return "empty";
  }

  var NAMES = ["north", "east", "south", "west"];
  function openings(mask) {
    var out = [];
    for (var i = 0; i < 4; i++) if (mask & (1 << i)) out.push(NAMES[i]);
    return out;
  }

  // False when turning the tile changes nothing (a cross). Turning is still allowed: it counts as
  // a move and can be undone; this only tells the game to say so.
  function canRotate(mask) {
    return rotate(mask, true) !== mask;
  }

  function isSize(n) {
    return SIZES.indexOf(n) !== -1;
  }

  // Today's seed. Salted so Moss Paths gets its own garden for the day, and each size its own.
  // The 6 by 6 seed is the same as before sizes existed.
  function dateSeed(d, size) {
    var ymd = d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();
    var seed = (Math.imul(ymd, 2654435761) ^ 0x4d6f7373) >>> 0;
    if (isSize(size) && size !== SIZE) seed = (seed ^ Math.imul(size, 0x9e3779b1)) >>> 0;
    return seed;
  }

  function isMask(v) {
    return Number.isInteger(v) && v >= 0 && v <= 15;
  }

  // Which tiles the spring's water reaches: a stream passes between two neighbours only when both
  // tiles open toward each other.
  function watered(cells, size, src) {
    var wet = [];
    for (var i = 0; i < size * size; i++) wet.push(false);
    wet[src] = true;
    var queue = [src];
    while (queue.length) {
      var cur = queue.pop();
      var cx = cur % size;
      var cy = Math.floor(cur / size);
      for (var k = 0; k < 4; k++) {
        var d = DIRS[k];
        if (!(cells[cur] & d.bit)) continue;
        var nx = cx + d.dx;
        var ny = cy + d.dy;
        if (nx < 0 || ny < 0 || nx >= size || ny >= size) continue;
        var ni = ny * size + nx;
        if (wet[ni] || !(cells[ni] & d.opp)) continue;
        wet[ni] = true;
        queue.push(ni);
      }
    }
    return wet;
  }

  function wateredCount(cells, size, src) {
    var wet = watered(cells, size, src);
    var n = 0;
    for (var i = 0; i < wet.length; i++) if (wet[i]) n += 1;
    return n;
  }

  function isSolved(cells, size, src) {
    return wateredCount(cells, size, src) === size * size;
  }

  // Builds a random spanning tree of streams from the spring, then turns every tile a random
  // number of quarter turns. Returns { size, src, solution, cells }: `solution` is the solved
  // garden, `cells` the tangled one. Always solvable (turning each tile back is a solution) and
  // never already solved.
  function generate(seed, size) {
    size = size || SIZE;
    var total = size * size;
    var rand = mulberry32(seed);
    var src = Math.floor(rand() * total);
    var solution = [];
    var seen = [];
    for (var i = 0; i < total; i++) {
      solution.push(0);
      seen.push(false);
    }
    seen[src] = true;
    var frontier = [];
    function addFrontier(from) {
      var fx = from % size;
      var fy = Math.floor(from / size);
      for (var k = 0; k < 4; k++) {
        var d = DIRS[k];
        var nx = fx + d.dx;
        var ny = fy + d.dy;
        if (nx < 0 || ny < 0 || nx >= size || ny >= size) continue;
        if (!seen[ny * size + nx]) frontier.push({ from: from, k: k, to: ny * size + nx });
      }
    }
    addFrontier(src);
    while (frontier.length) {
      // Half the time extend the newest branch (long winding paths), half the time any branch.
      var pickIdx = rand() < 0.55 ? frontier.length - 1 : Math.floor(rand() * frontier.length);
      var edge = frontier[pickIdx];
      frontier[pickIdx] = frontier[frontier.length - 1];
      frontier.pop();
      if (seen[edge.to]) continue;
      seen[edge.to] = true;
      solution[edge.from] |= DIRS[edge.k].bit;
      solution[edge.to] |= DIRS[edge.k].opp;
      addFrontier(edge.to);
    }
    var cells;
    for (var attempt = 0; attempt < 50; attempt++) {
      cells = [];
      for (var j = 0; j < total; j++) {
        var m = solution[j];
        var turns = Math.floor(rand() * 4);
        for (var t = 0; t < turns; t++) m = rotate(m, true);
        cells.push(m);
      }
      if (!isSolved(cells, size, src)) break;
    }
    return { size: size, src: src, solution: solution, cells: cells };
  }

  // Returns a new cells array with tile `index` turned a quarter turn.
  function turn(cells, index, cw) {
    var next = cells.slice();
    next[index] = rotate(next[index], cw !== false);
    return next;
  }

  // Keeps the undo list from growing without bound.
  var HISTORY_LIMIT = 400;
  function pushMove(history, index, cw) {
    var next = history.concat([{ i: index, cw: cw !== false }]);
    return next.length > HISTORY_LIMIT ? next.slice(next.length - HISTORY_LIMIT) : next;
  }

  function undo(cells, history) {
    if (history.length === 0) return { cells: cells, history: history };
    var last = history[history.length - 1];
    return { cells: turn(cells, last.i, !last.cw), history: history.slice(0, -1) };
  }

  root.MossLogic = {
    SIZE: SIZE,
    SIZES: SIZES,
    isSize: isSize,
    N: N,
    E: E,
    S: S,
    W: W,
    HISTORY_LIMIT: HISTORY_LIMIT,
    rotate: rotate,
    pieceType: pieceType,
    openings: openings,
    canRotate: canRotate,
    isMask: isMask,
    dateSeed: dateSeed,
    watered: watered,
    wateredCount: wateredCount,
    isSolved: isSolved,
    generate: generate,
    turn: turn,
    pushMove: pushMove,
    undo: undo,
  };
})(globalThis);
