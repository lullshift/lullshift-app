// Pure game logic for Fog Garden. No DOM access. Attaches its API to globalThis.FogLogic.
// A field is plain arrays of length size*size: brambles (1 = bramble, or null before the first
// reveal), revealed (0/1) and marks (0/1). A game carries its `size` (7, 9 or 12). Functions that
// take a size default to 9, the original field, so older saved games (which have no size) load.
(function (root) {
  "use strict";
  var SIZE = 9;
  var CELLS = SIZE * SIZE;
  var BRAMBLES = 12;
  // Three calm sizes: small, medium, large.
  var SIZES = [
    { size: 7, brambles: 8 },
    { size: 9, brambles: 12 },
    { size: 12, brambles: 24 },
  ];

  function sizeOK(n) {
    return SIZES.some(function (s) {
      return s.size === n;
    });
  }
  function bramblesFor(size) {
    for (var i = 0; i < SIZES.length; i++) if (SIZES[i].size === size) return SIZES[i].brambles;
    return BRAMBLES;
  }
  function sizeOf(game) {
    return sizeOK(game.size) ? game.size : SIZE;
  }

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

  function zeros(size) {
    var a = [];
    var n = (size || SIZE) * (size || SIZE);
    for (var i = 0; i < n; i++) a.push(0);
    return a;
  }

  // Indexes of the up to 8 tiles around `index`.
  function neighbours(index, size) {
    var SIZE = size || 9;
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
  function placeBrambles(seed, safeIndex, size) {
    var sz = sizeOK(size) ? size : SIZE;
    var rand = mulberry32(seed);
    var banned = {};
    banned[safeIndex] = true;
    neighbours(safeIndex, sz).forEach(function (n) {
      banned[n] = true;
    });
    var pool = [];
    for (var i = 0; i < sz * sz; i++) if (!banned[i]) pool.push(i);
    for (var j = pool.length - 1; j > 0; j--) {
      var k = Math.floor(rand() * (j + 1));
      var tmp = pool[j];
      pool[j] = pool[k];
      pool[k] = tmp;
    }
    var out = zeros(sz);
    for (var p = 0; p < bramblesFor(sz); p++) out[pool[p]] = 1;
    return out;
  }

  // How many of the 8 tiles around `index` hide brambles.
  function countAround(brambles, index, size) {
    var n = 0;
    neighbours(index, size).forEach(function (m) {
      if (brambles[m]) n += 1;
    });
    return n;
  }

  function newGame(seed, size) {
    var sz = sizeOK(size) ? size : SIZE;
    return {
      seed: seed,
      size: sz,
      brambles: null,
      first: null,
      revealed: zeros(sz),
      marks: zeros(sz),
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
    var sz = sizeOf(game);
    var brambles = game.brambles || placeBrambles(game.seed, index, sz);
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
        if (countAround(brambles, cur, sz) !== 0) continue;
        neighbours(cur, sz).forEach(function (n) {
          if (!revealed[n] && !game.marks[n] && !brambles[n]) {
            revealed[n] = 1;
            stack.push(n);
          }
        });
      }
    }
    var next = {
      seed: game.seed,
      size: sz,
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
    for (var i = 0; i < game.revealed.length; i++) {
      if (!game.brambles[i] && !game.revealed[i]) return false;
    }
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
        size: sizeOf(game),
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
  // A daily field: the same seed and the same opening for everyone, so the field does not depend
  // on where the first tile is revealed. The centre tile and its neighbours start revealed.
  function centreOf(size) {
    var sz = sizeOK(size) ? size : SIZE;
    return Math.floor(sz / 2) * sz + Math.floor(sz / 2);
  }
  var CENTRE = centreOf(SIZE);
  function dailyGame(seed, size) {
    return reveal(newGame(seed, size), centreOf(size)).game;
  }

  function tryAgain(game) {
    var fresh = {
      seed: game.seed,
      size: sizeOf(game),
      brambles: game.brambles,
      first: game.first,
      revealed: zeros(sizeOf(game)),
      marks: zeros(sizeOf(game)),
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
    return bramblesFor(sizeOf(game)) - sum(game.marks);
  }

  // Revealed safe tiles. After a hit the bramble tile itself is not counted.
  function revealedCount(game) {
    var n = 0;
    for (var i = 0; i < game.revealed.length; i++) {
      if (game.revealed[i] && !(game.brambles && game.brambles[i])) n += 1;
    }
    return n;
  }

  function safeTotal(size) {
    var sz = sizeOK(size) ? size : SIZE;
    return sz * sz - bramblesFor(sz);
  }

  function isBits(v, cells) {
    return (
      Array.isArray(v) &&
      v.length === cells &&
      v.every(function (x) {
        return x === 0 || x === 1;
      })
    );
  }

  // Checks untrusted saved state. Returns a clean game or null.
  function validate(r) {
    if (!r || typeof r !== "object") return null;
    // Saves from before sizes existed have no size field: they are the 9 by 9 field.
    var sz = r.size === undefined ? SIZE : r.size;
    if (!sizeOK(sz)) return null;
    var cells = sz * sz;
    if (!Number.isInteger(r.seed) || !isBits(r.revealed, cells) || !isBits(r.marks, cells)) {
      return null;
    }
    if (r.brambles !== null && !isBits(r.brambles, cells)) return null;
    if (r.brambles !== null && sum(r.brambles) !== bramblesFor(sz)) return null;
    if (r.brambles === null && (sum(r.revealed) !== 0 || r.status !== "playing")) return null;
    var game = {
      seed: r.seed,
      size: sz,
      brambles: r.brambles,
      first:
        r.brambles &&
        Number.isInteger(r.first) &&
        r.first >= 0 &&
        r.first < cells &&
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
      for (var i = 0; i < cells; i++) if (game.brambles[i] && game.revealed[i]) hit = true;
      if (hit) game.status = "hit";
      else if (isWon(game)) game.status = "won";
    }
    return game;
  }

  root.FogLogic = {
    SIZE: SIZE,
    CELLS: CELLS,
    BRAMBLES: BRAMBLES,
    SIZES: SIZES,
    bramblesFor: bramblesFor,
    centreOf: centreOf,
    neighbours: neighbours,
    placeBrambles: placeBrambles,
    countAround: countAround,
    CENTRE: CENTRE,
    newGame: newGame,
    dailyGame: dailyGame,
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
