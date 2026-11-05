// Pure game logic for Fog Garden. No DOM access. Attaches its API to globalThis.FogLogic.
// A field is plain arrays of length SIZE*SIZE: brambles (1 = bramble, or null before the first
// reveal), revealed (0/1) and marks (0/1).
(function (root) {
  "use strict";
  var SIZE = 9;
  var CELLS = SIZE * SIZE;
  var BRAMBLES = 12;

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

  function zeros() {
    var a = [];
    for (var i = 0; i < CELLS; i++) a.push(0);
    return a;
  }

  // Indexes of the up to 8 tiles around `index`.
  function neighbours(index) {
    var x = index % SIZE;
    var y = Math.floor(index / SIZE);
    var out = [];
    for (var dy = -1; dy <= 1; dy++) {
      for (var dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dy === 0) continue;
        var nx = x + dx;
        var ny = y + dy;
        if (nx >= 0 && ny >= 0 && nx < SIZE && ny < SIZE) out.push(ny * SIZE + nx);
      }
    }
    return out;
  }

  // Hides BRAMBLES brambles, never on `safeIndex` or any tile around it, so the first reveal is
  // always a clear tile that opens up the field.
  function placeBrambles(seed, safeIndex) {
    var rand = mulberry32(seed);
    var banned = {};
    banned[safeIndex] = true;
    neighbours(safeIndex).forEach(function (n) {
      banned[n] = true;
    });
    var pool = [];
    for (var i = 0; i < CELLS; i++) if (!banned[i]) pool.push(i);
    for (var j = pool.length - 1; j > 0; j--) {
      var k = Math.floor(rand() * (j + 1));
      var tmp = pool[j];
      pool[j] = pool[k];
      pool[k] = tmp;
    }
    var out = zeros();
    for (var p = 0; p < BRAMBLES; p++) out[pool[p]] = 1;
    return out;
  }

  // How many of the 8 tiles around `index` hide brambles.
  function countAround(brambles, index) {
    var n = 0;
    neighbours(index).forEach(function (m) {
      if (brambles[m]) n += 1;
    });
    return n;
  }

  function newGame(seed) {
    return {
      seed: seed,
      brambles: null,
      first: null,
      revealed: zeros(),
      marks: zeros(),
      status: "playing",
    };
  }

  // Reveals a tile, returning { game, changed }. A marked or already revealed tile is left alone.
  // The first reveal generates the brambles. A zero tile spreads to its neighbours. Revealing a
  // bramble sets status "hit" and uncovers every bramble.
  function reveal(game, index) {
    if (game.status !== "playing" || game.revealed[index] || game.marks[index]) {
      return { game: game, changed: false };
    }
    var brambles = game.brambles || placeBrambles(game.seed, index);
    var revealed = game.revealed.slice();
    var status = "playing";
    if (brambles[index]) {
      revealed[index] = 1;
      status = "hit";
    } else {
      var stack = [index];
      revealed[index] = 1;
      while (stack.length) {
        var cur = stack.pop();
        if (countAround(brambles, cur) !== 0) continue;
        neighbours(cur).forEach(function (n) {
          if (!revealed[n] && !game.marks[n] && !brambles[n]) {
            revealed[n] = 1;
            stack.push(n);
          }
        });
      }
    }
    var next = {
      seed: game.seed,
      brambles: brambles,
      first: game.first === null ? index : game.first,
      revealed: revealed,
      marks: game.marks,
      status: status,
    };
    if (status === "playing" && isWon(next)) next.status = "won";
    return { game: next, changed: true };
  }

  function isWon(game) {
    if (!game.brambles) return false;
    for (var i = 0; i < CELLS; i++) if (!game.brambles[i] && !game.revealed[i]) return false;
    return true;
  }

  // Marks or unmarks an unrevealed tile you think hides a bramble.
  function toggleMark(game, index) {
    if (game.status !== "playing" || game.revealed[index]) return { game: game, changed: false };
    var marks = game.marks.slice();
    marks[index] = marks[index] ? 0 : 1;
    return {
      game: {
        seed: game.seed,
        brambles: game.brambles,
        first: game.first,
        revealed: game.revealed,
        marks: marks,
        status: game.status,
      },
      changed: true,
    };
  }

  // Same field (same brambles), everything covered again, then the first safe tile is opened
  // again so a retry also starts gently.
  function tryAgain(game) {
    var fresh = {
      seed: game.seed,
      brambles: game.brambles,
      first: game.first,
      revealed: zeros(),
      marks: zeros(),
      status: "playing",
    };
    return game.first === null ? fresh : reveal(fresh, game.first).game;
  }

  function sum(a) {
    var n = 0;
    for (var i = 0; i < a.length; i++) if (a[i]) n += 1;
    return n;
  }

  function bramblesLeft(game) {
    return BRAMBLES - sum(game.marks);
  }

  // Revealed safe tiles. After a hit the bramble tile itself is not counted.
  function revealedCount(game) {
    var n = 0;
    for (var i = 0; i < CELLS; i++) {
      if (game.revealed[i] && !(game.brambles && game.brambles[i])) n += 1;
    }
    return n;
  }

  function safeTotal() {
    return CELLS - BRAMBLES;
  }

  function isBits(v) {
    return (
      Array.isArray(v) &&
      v.length === CELLS &&
      v.every(function (x) {
        return x === 0 || x === 1;
      })
    );
  }

  // Checks untrusted saved state. Returns a clean game or null.
  function validate(r) {
    if (!r || typeof r !== "object") return null;
    if (!Number.isInteger(r.seed) || !isBits(r.revealed) || !isBits(r.marks)) return null;
    if (r.brambles !== null && !isBits(r.brambles)) return null;
    if (r.brambles !== null && sum(r.brambles) !== BRAMBLES) return null;
    if (r.brambles === null && (sum(r.revealed) !== 0 || r.status !== "playing")) return null;
    var game = {
      seed: r.seed,
      brambles: r.brambles,
      first:
        r.brambles &&
        Number.isInteger(r.first) &&
        r.first >= 0 &&
        r.first < CELLS &&
        !r.brambles[r.first]
          ? r.first
          : null,
      revealed: r.revealed.slice(),
      marks: r.marks.map(function (m, i) {
        return m && !r.revealed[i] ? 1 : 0;
      }),
      status: "playing",
    };
    if (game.brambles) {
      var hit = false;
      for (var i = 0; i < CELLS; i++) if (game.brambles[i] && game.revealed[i]) hit = true;
      if (hit) game.status = "hit";
      else if (isWon(game)) game.status = "won";
    }
    return game;
  }

  root.FogLogic = {
    SIZE: SIZE,
    CELLS: CELLS,
    BRAMBLES: BRAMBLES,
    neighbours: neighbours,
    placeBrambles: placeBrambles,
    countAround: countAround,
    newGame: newGame,
    reveal: reveal,
    toggleMark: toggleMark,
    tryAgain: tryAgain,
    isWon: isWon,
    bramblesLeft: bramblesLeft,
    revealedCount: revealedCount,
    safeTotal: safeTotal,
    validate: validate,
  };
})(globalThis);
