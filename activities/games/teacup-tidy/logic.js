// Pure game logic for Teacup Tidy. No DOM access. Attaches its API to globalThis.TeaLogic.
// A cup is an array of colour numbers (1..COLOURS), bottom first. Cups hold at most CAP layers.
(function (root) {
  "use strict";
  var COLOURS = 6;
  var CAP = 4;
  var EMPTY_CUPS = 2;
  var CUPS = COLOURS + EMPTY_CUPS;
  var HISTORY_LIMIT = 10000;
  var NAMES = ["", "Jasmine", "Mint", "Chai", "Berry", "Rooibos", "Sencha"];
  var SHAPES = ["", "dots", "stripes", "checks", "rings", "waves", "crosses"];

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

  function clone(cups) {
    return cups.map(function (c) {
      return c.slice();
    });
  }

  function solvedCups() {
    var cups = [];
    for (var c = 1; c <= COLOURS; c++) {
      var cup = [];
      for (var k = 0; k < CAP; k++) cup.push(c);
      cups.push(cup);
    }
    for (var e = 0; e < EMPTY_CUPS; e++) cups.push([]);
    return cups;
  }

  // Length of the run of one colour on top of a cup.
  function topRun(cup) {
    if (cup.length === 0) return 0;
    var colour = cup[cup.length - 1];
    var n = 0;
    for (var i = cup.length - 1; i >= 0 && cup[i] === colour; i--) n += 1;
    return n;
  }

  // How many layers would move if you poured `from` into `to` (0 when the pour is not allowed).
  function pourAmount(cups, from, to) {
    if (from === to) return 0;
    var a = cups[from];
    var b = cups[to];
    if (!a || !b || a.length === 0) return 0;
    var space = CAP - b.length;
    if (space <= 0) return 0;
    if (b.length > 0 && b[b.length - 1] !== a[a.length - 1]) return 0;
    return Math.min(topRun(a), space);
  }

  // Applies a pour. Returns { cups, amount } or null when it is not allowed. Never mutates.
  function pour(cups, from, to) {
    var n = pourAmount(cups, from, to);
    if (n === 0) return null;
    var next = clone(cups);
    for (var i = 0; i < n; i++) next[to].push(next[from].pop());
    return { cups: next, amount: n };
  }

  // A move is stored as one small integer: from + 16 * to + 256 * amount.
  function encode(from, to, amount) {
    return from + 16 * to + 256 * amount;
  }
  function decode(code) {
    return { from: code & 15, to: (code >> 4) & 15, amount: code >> 8 };
  }
  function isMoveCode(v) {
    if (!Number.isInteger(v) || v < 0) return false;
    var m = decode(v);
    return m.from < CUPS && m.to < CUPS && m.from !== m.to && m.amount >= 1 && m.amount <= CAP;
  }

  // Takes back a recorded move. Returns the new cups, or null when the cups do not fit the move.
  function undoMove(cups, code) {
    var m = decode(code);
    var to = cups[m.to];
    var from = cups[m.from];
    if (!to || !from || to.length < m.amount || from.length + m.amount > CAP) return null;
    var next = clone(cups);
    for (var i = 0; i < m.amount; i++) next[m.from].push(next[m.to].pop());
    return next;
  }

  function pushMove(history, code) {
    var next = history.concat([code]);
    return next.length > HISTORY_LIMIT ? next.slice(next.length - HISTORY_LIMIT) : next;
  }

  // Every cup is empty or full of one colour.
  function isSolved(cups) {
    for (var i = 0; i < cups.length; i++) {
      var cup = cups[i];
      if (cup.length === 0) continue;
      if (cup.length !== CAP) return false;
      for (var k = 1; k < cup.length; k++) if (cup[k] !== cup[0]) return false;
    }
    return true;
  }

  function sortedCount(cups) {
    var n = 0;
    for (var i = 0; i < cups.length; i++) {
      var cup = cups[i];
      if (cup.length !== CAP) continue;
      var same = true;
      for (var k = 1; k < cup.length; k++) if (cup[k] !== cup[0]) same = false;
      if (same) n += 1;
    }
    return n;
  }

  // Scrambles a solved set by exact reverse pours, so the result is solvable by construction:
  // replaying the reversed list of pours (returned as `solution`) solves it.
  // A reverse step takes `amount` layers from the top run of cup Y and puts them on cup X, where
  // X is empty or ends in another colour, and Y is left empty or still ending in that colour.
  function generate(seed) {
    var rnd = mulberry32(seed);
    for (var attempt = 0; attempt < 20; attempt++) {
      var cups = solvedCups();
      var solution = [];
      var steps = 0;
      var tries = 0;
      while (steps < 90 && tries < 4000) {
        tries += 1;
        var y = Math.floor(rnd() * CUPS);
        var x = Math.floor(rnd() * CUPS);
        if (x === y || cups[y].length === 0) continue;
        var colour = cups[y][cups[y].length - 1];
        var run = topRun(cups[y]);
        var amount = 1 + Math.floor(rnd() * Math.min(run, CAP - cups[x].length || 1));
        if (cups[x].length + amount > CAP) continue;
        if (cups[x].length > 0 && cups[x][cups[x].length - 1] === colour) continue;
        var leftover = cups[y].length - amount;
        if (leftover > 0 && cups[y][leftover - 1] !== colour) continue;
        for (var i = 0; i < amount; i++) cups[x].push(cups[y].pop());
        solution.push(encode(x, y, amount));
        steps += 1;
      }
      if (!isSolved(cups) && sortedCount(cups) <= 2) {
        solution.reverse();
        return { cups: cups, solution: solution };
      }
    }
    // Practically unreachable: fall back to a fixed hand-made scramble.
    var fallback = solvedCups();
    var fb = [];
    var plan = [
      [0, 6, 2],
      [1, 7, 1],
      [0, 7, 1],
      [1, 6, 1],
    ];
    for (var p = 0; p < plan.length; p++) {
      var step = plan[p];
      for (var j = 0; j < step[2]; j++) fallback[step[1]].push(fallback[step[0]].pop());
      fb.push(encode(step[1], step[0], step[2]));
    }
    fb.reverse();
    return { cups: fallback, solution: fb };
  }

  function validCups(v) {
    if (!Array.isArray(v) || v.length !== CUPS) return false;
    var counts = [];
    for (var c = 0; c <= COLOURS; c++) counts.push(0);
    for (var i = 0; i < v.length; i++) {
      var cup = v[i];
      if (!Array.isArray(cup) || cup.length > CAP) return false;
      for (var k = 0; k < cup.length; k++) {
        if (!Number.isInteger(cup[k]) || cup[k] < 1 || cup[k] > COLOURS) return false;
        counts[cup[k]] += 1;
      }
    }
    for (var d = 1; d <= COLOURS; d++) if (counts[d] !== CAP) return false;
    return true;
  }

  root.TeaLogic = {
    COLOURS: COLOURS,
    CAP: CAP,
    CUPS: CUPS,
    HISTORY_LIMIT: HISTORY_LIMIT,
    NAMES: NAMES,
    SHAPES: SHAPES,
    generate: generate,
    solvedCups: solvedCups,
    topRun: topRun,
    pourAmount: pourAmount,
    pour: pour,
    encode: encode,
    decode: decode,
    isMoveCode: isMoveCode,
    undoMove: undoMove,
    pushMove: pushMove,
    isSolved: isSolved,
    sortedCount: sortedCount,
    validCups: validCups,
  };
})(globalThis);
