// Pure game logic for Pebble Merge. No DOM access. Attaches its API to globalThis.PebbleLogic.
// The tray is 16 numbers in reading order: 0 is empty, 1..MAX_TIER is a pebble size.
// Directions: 0 up, 1 right, 2 down, 3 left.
(function (root) {
  "use strict";
  var SIZE = 4;
  var CELLS = SIZE * SIZE;
  var NAMES = [
    "",
    "Grain",
    "Chip",
    "Pebble",
    "Cobble",
    "Stone",
    "Rock",
    "Boulder",
    "Crag",
    "Tor",
    "Hill",
    "Peak",
  ];
  var MAX_TIER = NAMES.length - 1;
  var DIR_NAMES = ["up", "right", "down", "left"];

  // One step of mulberry32 on an explicit state, so a game can save its place in the sequence.
  function nextRandom(state) {
    var a = (state + 0x6d2b79f5) >>> 0;
    var t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return { value: ((t ^ (t >>> 14)) >>> 0) / 4294967296, state: a };
  }

  function emptyBoard() {
    var b = [];
    for (var i = 0; i < CELLS; i++) b.push(0);
    return b;
  }

  function emptyCells(board) {
    var out = [];
    for (var i = 0; i < CELLS; i++) if (board[i] === 0) out.push(i);
    return out;
  }

  // Places one new pebble (a Grain four times in five, otherwise a Chip) in a seeded empty cell.
  function spawn(board, rng) {
    var free = emptyCells(board);
    if (free.length === 0) return { board: board.slice(), rng: rng, index: -1 };
    var a = nextRandom(rng);
    var b = nextRandom(a.state);
    var index = free[Math.floor(a.value * free.length)];
    var next = board.slice();
    next[index] = b.value < 0.8 ? 1 : 2;
    return { board: next, rng: b.state, index: index };
  }

  // A fresh game: an empty tray with two seeded pebbles on it.
  function newGame(seed) {
    var s = spawn(emptyBoard(), seed >>> 0);
    s = spawn(s.board, s.rng);
    return { board: s.board, rng: s.rng, moves: 0, merges: 0 };
  }

  // Index of the i-th cell of line `n` when sliding in direction `dir`; position 0 is the far end
  // the pebbles slide toward.
  function lineIndex(dir, n, i) {
    if (dir === 0) return i * SIZE + n;
    if (dir === 2) return (SIZE - 1 - i) * SIZE + n;
    if (dir === 3) return n * SIZE + i;
    return n * SIZE + (SIZE - 1 - i);
  }

  // Slides everything one way. Two equal pebbles that meet become the next size, once per slide.
  // Returns { board, moved, merges, largest } where largest is the biggest pebble made by a merge.
  function slide(board, dir) {
    var next = emptyBoard();
    var moved = false;
    var merges = 0;
    var largest = 0;
    for (var n = 0; n < SIZE; n++) {
      var line = [];
      for (var i = 0; i < SIZE; i++) {
        var v = board[lineIndex(dir, n, i)];
        if (v !== 0) line.push(v);
      }
      var out = [];
      for (var k = 0; k < line.length; k++) {
        if (k + 1 < line.length && line[k] === line[k + 1] && line[k] < MAX_TIER) {
          out.push(line[k] + 1);
          merges += 1;
          if (line[k] + 1 > largest) largest = line[k] + 1;
          k += 1;
        } else {
          out.push(line[k]);
        }
      }
      for (var j = 0; j < SIZE; j++) {
        var idx = lineIndex(dir, n, j);
        next[idx] = j < out.length ? out[j] : 0;
        if (next[idx] !== board[idx]) moved = true;
      }
    }
    return { board: next, moved: moved, merges: merges, largest: largest };
  }

  // Plays one slide of a game, then drops a new pebble. Returns null when the slide changes
  // nothing. Never mutates.
  function move(game, dir) {
    var s = slide(game.board, dir);
    if (!s.moved) return null;
    var sp = spawn(s.board, game.rng);
    return {
      board: sp.board,
      rng: sp.rng,
      moves: game.moves + 1,
      merges: game.merges + s.merges,
      spawned: sp.index,
      largestMade: s.largest,
    };
  }

  function canMove(board) {
    for (var d = 0; d < 4; d++) if (slide(board, d).moved) return true;
    return false;
  }

  function largest(board) {
    var m = 0;
    for (var i = 0; i < CELLS; i++) if (board[i] > m) m = board[i];
    return m;
  }

  function tierName(t) {
    return NAMES[t] || "";
  }

  function validBoard(v) {
    if (!Array.isArray(v) || v.length !== CELLS) return false;
    return v.every(function (t) {
      return Number.isInteger(t) && t >= 0 && t <= MAX_TIER;
    });
  }

  function validGame(g) {
    return (
      !!g &&
      validBoard(g.board) &&
      Number.isInteger(g.rng) &&
      g.rng >= 0 &&
      g.rng <= 4294967295 &&
      Number.isInteger(g.moves) &&
      g.moves >= 0 &&
      Number.isInteger(g.merges) &&
      g.merges >= 0
    );
  }

  root.PebbleLogic = {
    SIZE: SIZE,
    CELLS: CELLS,
    MAX_TIER: MAX_TIER,
    NAMES: NAMES,
    DIR_NAMES: DIR_NAMES,
    nextRandom: nextRandom,
    emptyBoard: emptyBoard,
    spawn: spawn,
    newGame: newGame,
    slide: slide,
    move: move,
    canMove: canMove,
    largest: largest,
    tierName: tierName,
    validBoard: validBoard,
    validGame: validGame,
  };
})(globalThis);
