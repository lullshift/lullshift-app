// Frost Steps rules. No DOM. Classic script: attaches FrostLogic to globalThis.
//
// A walker stands on an icy pond. A move picks a direction and the walker slides until the next
// square is a rock or the pond's edge. The warm lantern square stops the walker too, so reaching it
// means stopping on it. A puzzle is { w, h, rocks, start, goal, best }, squares counted row by row
// from 0. `best` is the exact fewest moves, found with a breadth-first search; every generated
// puzzle needs at least MIN_MOVES[size] moves, so none is trivial.
(function (root) {
  "use strict";

  var SIZES = [6, 7, 8, 9];
  var MIN_MOVES = { 6: 6, 7: 8, 8: 10, 9: 12 };
  var DIRS = ["up", "down", "left", "right"];
  var DX = { up: 0, down: 0, left: -1, right: 1 };
  var DY = { up: -1, down: 1, left: 0, right: 0 };
  var ATTEMPTS = 400;

  function kit() {
    return root.LullshiftKits && root.LullshiftKits.grid;
  }

  function dateSeed(d) {
    return kit().dailySeed(d, "frost-steps");
  }

  function sizeForSeed(seed) {
    return SIZES[(Math.imul(seed >>> 0, 0x9e3779b1) >>> 16) % SIZES.length];
  }

  // Slides from square `pos`. Returns { pos, moved, stop } where stop is "rock", "edge" or
  // "lantern" (why the walker stopped).
  function slide(p, pos, dir) {
    var rocks = p.rockSet || (p.rockSet = toSet(p.rocks));
    var x = pos % p.w;
    var y = Math.floor(pos / p.w);
    var moved = false;
    for (;;) {
      var nx = x + DX[dir];
      var ny = y + DY[dir];
      if (nx < 0 || ny < 0 || nx >= p.w || ny >= p.h)
        return { pos: y * p.w + x, moved: moved, stop: "edge" };
      if (rocks[ny * p.w + nx]) return { pos: y * p.w + x, moved: moved, stop: "rock" };
      x = nx;
      y = ny;
      moved = true;
      if (y * p.w + x === p.goal) return { pos: p.goal, moved: true, stop: "lantern" };
    }
  }

  function toSet(list) {
    var s = {};
    for (var i = 0; i < list.length; i++) s[list[i]] = true;
    return s;
  }

  // Breadth-first search over stopping squares. Returns { best, path } (path is a list of
  // directions) or null when the lantern cannot be reached.
  function solve(p) {
    var n = p.w * p.h;
    var dist = new Array(n);
    var from = new Array(n);
    var how = new Array(n);
    for (var i = 0; i < n; i++) dist[i] = -1;
    dist[p.start] = 0;
    var queue = [p.start];
    for (var head = 0; head < queue.length; head++) {
      var at = queue[head];
      if (at === p.goal) break;
      for (var d = 0; d < 4; d++) {
        var r = slide(p, at, DIRS[d]);
        if (!r.moved || dist[r.pos] !== -1) continue;
        dist[r.pos] = dist[at] + 1;
        from[r.pos] = at;
        how[r.pos] = DIRS[d];
        queue.push(r.pos);
      }
    }
    if (dist[p.goal] < 0) return null;
    var path = [];
    for (var c = p.goal; c !== p.start; c = from[c]) path.push(how[c]);
    path.reverse();
    return { best: dist[p.goal], path: path };
  }

  // Distances from the start with no lantern at all (every square is a plain stopping point test).
  function reach(p) {
    var n = p.w * p.h;
    var dist = new Array(n);
    for (var i = 0; i < n; i++) dist[i] = -1;
    dist[p.start] = 0;
    var queue = [p.start];
    for (var head = 0; head < queue.length; head++) {
      var at = queue[head];
      for (var d = 0; d < 4; d++) {
        var r = slide(p, at, DIRS[d]);
        if (!r.moved || dist[r.pos] !== -1) continue;
        dist[r.pos] = dist[at] + 1;
        queue.push(r.pos);
      }
    }
    return dist;
  }

  // The puzzle for a seed (and optionally a size). Same seed, same puzzle, always. Random rock
  // layouts are tried until a lantern square exists that needs at least MIN_MOVES[size] moves,
  // checked by the search with the lantern in place.
  function generate(seed, size) {
    var s = size || sizeForSeed(seed);
    var need = MIN_MOVES[s];
    var n = s * s;
    for (var attempt = 0; attempt < ATTEMPTS; attempt++) {
      var rng = kit().createRng(((seed >>> 0) + Math.imul(attempt + 1, 0x9e3779b1)) >>> 0);
      var count = Math.round(n * (0.14 + rng.next() * 0.14));
      var rocks = [];
      var used = {};
      while (rocks.length < count) {
        var r = rng.int(0, n - 1);
        if (used[r]) continue;
        used[r] = true;
        rocks.push(r);
      }
      var free = [];
      for (var i = 0; i < n; i++) if (!used[i]) free.push(i);
      var start = free[rng.int(0, free.length - 1)];
      var probe = { w: s, h: s, rocks: rocks, start: start, goal: -1 };
      var dist = reach(probe);
      var candidates = [];
      for (var c = 0; c < n; c++) if (dist[c] >= need) candidates.push(c);
      // Try the candidates in a seeded order; the lantern changes the paths, so check each.
      candidates = rng.shuffle(candidates);
      for (var k = 0; k < candidates.length && k < 12; k++) {
        var puzzle = {
          w: s,
          h: s,
          rocks: rocks.slice().sort(numeric),
          start: start,
          goal: candidates[k],
          best: 0,
        };
        var sol = solve(puzzle);
        if (sol && sol.best >= need) {
          puzzle.best = sol.best;
          return puzzle;
        }
      }
    }
    return null;
  }

  function numeric(a, b) {
    return a - b;
  }

  // Checks saved puzzle data (it is untrusted) and returns a clean puzzle with `best` recomputed,
  // or null.
  function validatePuzzle(raw) {
    if (!raw || typeof raw !== "object") return null;
    var w = raw.w;
    var h = raw.h;
    if (!Number.isInteger(w) || w !== h || SIZES.indexOf(w) < 0) return null;
    var n = w * h;
    if (!Array.isArray(raw.rocks) || raw.rocks.length > n - 2) return null;
    var seen = {};
    for (var i = 0; i < raw.rocks.length; i++) {
      var r = raw.rocks[i];
      if (!Number.isInteger(r) || r < 0 || r >= n || seen[r]) return null;
      seen[r] = true;
    }
    if (!Number.isInteger(raw.start) || raw.start < 0 || raw.start >= n || seen[raw.start])
      return null;
    if (!Number.isInteger(raw.goal) || raw.goal < 0 || raw.goal >= n || seen[raw.goal]) return null;
    if (raw.goal === raw.start) return null;
    var p = {
      w: w,
      h: h,
      rocks: raw.rocks.slice().sort(numeric),
      start: raw.start,
      goal: raw.goal,
      best: 0,
    };
    var sol = solve(p);
    if (!sol) return null;
    p.best = sol.best;
    return p;
  }

  function copyPuzzle(p) {
    return { w: p.w, h: p.h, rocks: p.rocks.slice(), start: p.start, goal: p.goal, best: p.best };
  }

  root.FrostLogic = {
    SIZES: SIZES,
    MIN_MOVES: MIN_MOVES,
    DIRS: DIRS,
    dateSeed: dateSeed,
    sizeForSeed: sizeForSeed,
    slide: slide,
    solve: solve,
    generate: generate,
    validatePuzzle: validatePuzzle,
    copyPuzzle: copyPuzzle,
  };
})(globalThis);
