// Pure game logic for Lantern Pond. No DOM access. Attaches its API to globalThis.LanternLogic.
(function (root) {
  "use strict";
  var SIZE = 5;
  var CELLS = SIZE * SIZE;

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

  function solvedBoard() {
    var b = [];
    for (var i = 0; i < CELLS; i++) b.push(true);
    return b;
  }

  function isSolved(board) {
    for (var i = 0; i < CELLS; i++) if (!board[i]) return false;
    return true;
  }

  // Returns a new board with cell `index` and its in-bounds orthogonal neighbours flipped.
  function press(board, index) {
    var next = board.slice();
    var r = Math.floor(index / SIZE);
    var c = index % SIZE;
    var targets = [
      [r, c],
      [r - 1, c],
      [r + 1, c],
      [r, c - 1],
      [r, c + 1],
    ];
    for (var k = 0; k < targets.length; k++) {
      var tr = targets[k][0];
      var tc = targets[k][1];
      if (tr >= 0 && tr < SIZE && tc >= 0 && tc < SIZE) {
        var i = tr * SIZE + tc;
        next[i] = !next[i];
      }
    }
    return next;
  }

  // Starts from all-lit and applies 6-10 seeded presses, so the puzzle is always solvable.
  // Never returns an already-solved board. `presses` is the list of generating presses.
  function generate(seed) {
    var rand = mulberry32(seed);
    for (;;) {
      var count = 6 + Math.floor(rand() * 5);
      var board = solvedBoard();
      var presses = [];
      for (var n = 0; n < count; n++) {
        var idx = Math.floor(rand() * CELLS);
        presses.push(idx);
        board = press(board, idx);
      }
      if (!isSolved(board)) return { board: board, presses: presses };
    }
  }

  // Undo stack helpers. The stack is an array of pressed cell indices.
  function pushMove(stack, index) {
    return stack.concat([index]);
  }
  // Pressing the same cell again reverses a press, so undo is one more press.
  function undo(board, stack) {
    if (stack.length === 0) return { board: board, stack: stack };
    var last = stack[stack.length - 1];
    return { board: press(board, last), stack: stack.slice(0, -1) };
  }

  function dateSeed(date) {
    var d = date || new Date();
    return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();
  }

  root.LanternLogic = {
    SIZE: SIZE,
    CELLS: CELLS,
    mulberry32: mulberry32,
    solvedBoard: solvedBoard,
    isSolved: isSolved,
    press: press,
    generate: generate,
    pushMove: pushMove,
    undo: undo,
    dateSeed: dateSeed,
  };
})(globalThis);
