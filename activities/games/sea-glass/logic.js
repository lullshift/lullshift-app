// Pure game logic for Sea Glass. No DOM access. Attaches its API to globalThis.GlassLogic.
// The board is 64 numbers in reading order: 1..PIECES is a piece of sea glass. It is never empty
// between swaps. A game is { board, rng, score, swapsLeft }; rng is the saved PRNG state.
(function (root) {
  "use strict";
  var SIZE = 8;
  var CELLS = SIZE * SIZE;
  var PIECES = 6;
  var ROUND_SWAPS = 25;
  var FORMS = ["", "round", "diamond", "triangle", "square", "hexagon", "star"];
  var COLOURS = ["", "blue", "green", "amber", "rose", "violet", "frost"];
  var MAX_CASCADES = 60;

  // One step of mulberry32 on an explicit state, so a game can save its place in the sequence.
  function nextRandom(state) {
    var a = (state + 0x6d2b79f5) >>> 0;
    var t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return { value: ((t ^ (t >>> 14)) >>> 0) / 4294967296, state: a };
  }

  function pieceName(p) {
    return p >= 1 && p <= PIECES ? COLOURS[p] + " " + FORMS[p] : "";
  }

  function emptyBoard() {
    var b = [];
    for (var i = 0; i < CELLS; i++) b.push(0);
    return b;
  }

  // A board with no ready-made line of three, filled from the seeded generator.
  function fillBoard(rng) {
    var board = emptyBoard();
    for (var i = 0; i < CELLS; i++) {
      var x = i % SIZE;
      var y = Math.floor(i / SIZE);
      var banned = [];
      if (x >= 2 && board[i - 1] === board[i - 2]) banned.push(board[i - 1]);
      if (y >= 2 && board[i - SIZE] === board[i - 2 * SIZE]) banned.push(board[i - SIZE]);
      var choices = [];
      for (var p = 1; p <= PIECES; p++) if (banned.indexOf(p) < 0) choices.push(p);
      var r = nextRandom(rng);
      rng = r.state;
      board[i] = choices[Math.floor(r.value * choices.length)];
    }
    return { board: board, rng: rng };
  }

  // Index of the neighbour of `i` in direction dir (0 up, 1 right, 2 down, 3 left), or -1.
  function neighbour(i, dir) {
    var x = i % SIZE;
    var y = Math.floor(i / SIZE);
    if (dir === 0) return y > 0 ? i - SIZE : -1;
    if (dir === 1) return x < SIZE - 1 ? i + 1 : -1;
    if (dir === 2) return y < SIZE - 1 ? i + SIZE : -1;
    return x > 0 ? i - 1 : -1;
  }

  function areNeighbours(a, b) {
    for (var d = 0; d < 4; d++) if (neighbour(a, d) === b) return true;
    return false;
  }

  // Marks every piece that sits in a straight line of three or more. Returns an array of booleans.
  function findLines(board) {
    var hit = [];
    for (var i = 0; i < CELLS; i++) hit.push(false);
    var x;
    var y;
    var run;
    var k;
    for (y = 0; y < SIZE; y++) {
      x = 0;
      while (x < SIZE) {
        run = 1;
        while (
          x + run < SIZE &&
          board[y * SIZE + x + run] === board[y * SIZE + x] &&
          board[y * SIZE + x] > 0
        ) {
          run += 1;
        }
        if (run >= 3) for (k = 0; k < run; k++) hit[y * SIZE + x + k] = true;
        x += run;
      }
    }
    for (x = 0; x < SIZE; x++) {
      y = 0;
      while (y < SIZE) {
        run = 1;
        while (
          y + run < SIZE &&
          board[(y + run) * SIZE + x] === board[y * SIZE + x] &&
          board[y * SIZE + x] > 0
        ) {
          run += 1;
        }
        if (run >= 3) for (k = 0; k < run; k++) hit[(y + k) * SIZE + x] = true;
        y += run;
      }
    }
    return hit;
  }

  function anyLine(board) {
    return findLines(board).some(Boolean);
  }

  function swapped(board, a, b) {
    var next = board.slice();
    next[a] = board[b];
    next[b] = board[a];
    return next;
  }

  // A swap is allowed when the two cells touch and the swap makes at least one line.
  function swapMakesLine(board, a, b) {
    if (!areNeighbours(a, b) || board[a] === board[b]) return false;
    return anyLine(swapped(board, a, b));
  }

  function hasMove(board) {
    for (var i = 0; i < CELLS; i++) {
      var r = neighbour(i, 1);
      var d = neighbour(i, 2);
      if (r >= 0 && swapMakesLine(board, i, r)) return true;
      if (d >= 0 && swapMakesLine(board, i, d)) return true;
    }
    return false;
  }

  // Clears lines, lets pieces above fall, and brings new pieces from the seeded generator,
  // repeating while the new arrangement makes lines of its own. Points are 10 per piece, times
  // the step of the chain (1, 2, 3...).
  function resolve(board, rng) {
    var points = 0;
    var cleared = 0;
    var cascades = 0;
    var current = board.slice();
    while (cascades < MAX_CASCADES) {
      var hit = findLines(current);
      var n = hit.filter(Boolean).length;
      if (n === 0) break;
      cascades += 1;
      cleared += n;
      points += n * 10 * cascades;
      for (var x = 0; x < SIZE; x++) {
        var kept = [];
        for (var y = SIZE - 1; y >= 0; y--) {
          if (!hit[y * SIZE + x]) kept.push(current[y * SIZE + x]);
        }
        for (var fy = SIZE - 1; fy >= 0; fy--) {
          var slot = SIZE - 1 - fy;
          if (slot < kept.length) {
            current[fy * SIZE + x] = kept[slot];
          } else {
            var r = nextRandom(rng);
            rng = r.state;
            current[fy * SIZE + x] = 1 + Math.floor(r.value * PIECES);
          }
        }
      }
    }
    return { board: current, rng: rng, points: points, cleared: cleared, cascades: cascades };
  }

  // Shuffles the pieces of a stuck board (seeded) until it has no ready line and at least one
  // swap that makes a line. Falls back to a freshly filled board.
  function reshuffle(board, rng) {
    for (var attempt = 0; attempt < 40; attempt++) {
      var next = board.slice();
      for (var i = CELLS - 1; i > 0; i--) {
        var r = nextRandom(rng);
        rng = r.state;
        var j = Math.floor(r.value * (i + 1));
        var t = next[i];
        next[i] = next[j];
        next[j] = t;
      }
      if (!anyLine(next) && hasMove(next)) return { board: next, rng: rng };
    }
    for (;;) {
      var f = fillBoard(rng);
      rng = f.rng;
      if (hasMove(f.board)) return f;
    }
  }

  function newGame(seed) {
    var rng = seed >>> 0;
    for (;;) {
      var f = fillBoard(rng);
      rng = f.rng;
      if (hasMove(f.board)) return { board: f.board, rng: rng, score: 0, swapsLeft: ROUND_SWAPS };
    }
  }

  // Plays one swap. Returns null when it is not allowed (or the round is over). Never mutates.
  // The result adds { cleared, cascades, points, reshuffled } describing what happened.
  function swap(game, a, b) {
    if (game.swapsLeft <= 0) return null;
    if (
      !Number.isInteger(a) ||
      !Number.isInteger(b) ||
      a < 0 ||
      b < 0 ||
      a >= CELLS ||
      b >= CELLS
    ) {
      return null;
    }
    if (!swapMakesLine(game.board, a, b)) return null;
    var r = resolve(swapped(game.board, a, b), game.rng);
    var board = r.board;
    var rng = r.rng;
    var reshuffled = false;
    if (!hasMove(board)) {
      var s = reshuffle(board, rng);
      board = s.board;
      rng = s.rng;
      reshuffled = true;
    }
    return {
      board: board,
      rng: rng,
      score: game.score + r.points,
      swapsLeft: game.swapsLeft - 1,
      cleared: r.cleared,
      cascades: r.cascades,
      points: r.points,
      reshuffled: reshuffled,
    };
  }

  function roundOver(game) {
    return game.swapsLeft <= 0;
  }

  function validBoard(v) {
    if (!Array.isArray(v) || v.length !== CELLS) return false;
    return v.every(function (p) {
      return Number.isInteger(p) && p >= 1 && p <= PIECES;
    });
  }

  function validGame(g) {
    return (
      !!g &&
      validBoard(g.board) &&
      Number.isInteger(g.rng) &&
      g.rng >= 0 &&
      g.rng <= 4294967295 &&
      Number.isInteger(g.score) &&
      g.score >= 0 &&
      Number.isInteger(g.swapsLeft) &&
      g.swapsLeft >= 0 &&
      g.swapsLeft <= ROUND_SWAPS
    );
  }

  root.GlassLogic = {
    SIZE: SIZE,
    CELLS: CELLS,
    PIECES: PIECES,
    ROUND_SWAPS: ROUND_SWAPS,
    FORMS: FORMS,
    COLOURS: COLOURS,
    nextRandom: nextRandom,
    pieceName: pieceName,
    fillBoard: fillBoard,
    neighbour: neighbour,
    areNeighbours: areNeighbours,
    findLines: findLines,
    anyLine: anyLine,
    swapMakesLine: swapMakesLine,
    hasMove: hasMove,
    resolve: resolve,
    reshuffle: reshuffle,
    newGame: newGame,
    swap: swap,
    roundOver: roundOver,
    validBoard: validBoard,
    validGame: validGame,
  };
})(globalThis);
