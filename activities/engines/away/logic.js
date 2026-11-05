// Pure rules for Away Prompts (engine lullshift.away@1). No DOM, no clock of its own.
// Attaches its API to globalThis.AwayLogic.
(function (root) {
  "use strict";

  var CATEGORY_LABELS = {
    water: "Water",
    walk: "Walk",
    window: "Window",
    tidy: "Tidy",
    chat: "Chat",
    rest: "Rest",
  };

  var SAMPLE = {
    title: "A few sample ideas",
    intro: "Install a prompt pack to fill this in. Here are a few ideas to show how it works.",
    prompts: [
      { text: "Fill a glass of water and drink it slowly.", category: "water" },
      { text: "Look out of a window and find the farthest thing you can see.", category: "window" },
      { text: "Walk to the end of the room and back.", category: "walk" },
    ],
    shuffle: false,
    backAtDesk: "Welcome back. Your agents kept working while you were away.",
  };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // The host validates packs; this is a cheap second look so a bad pack never breaks the page.
  function validData(c) {
    if (!c || typeof c !== "object" || Array.isArray(c) || !isText(c.title, 80)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (c.shuffle !== undefined && typeof c.shuffle !== "boolean") return false;
    if (c.backAtDesk !== undefined && !isText(c.backAtDesk, 160)) return false;
    if (!Array.isArray(c.prompts) || c.prompts.length < 1 || c.prompts.length > 60) return false;
    for (var i = 0; i < c.prompts.length; i++) {
      var p = c.prompts[i];
      if (!p || typeof p !== "object" || !isText(p.text, 160)) return false;
      if (
        p.category !== undefined &&
        !Object.prototype.hasOwnProperty.call(CATEGORY_LABELS, p.category)
      )
        return false;
    }
    return true;
  }

  // A play order: 0..n-1, shuffled (Fisher-Yates) when asked. `rand` returns [0, 1).
  function makeOrder(n, shuffle, rand) {
    var order = [];
    for (var i = 0; i < n; i++) order.push(i);
    if (shuffle) {
      for (var j = n - 1; j > 0; j--) {
        var k = Math.min(j, Math.floor(rand() * (j + 1)));
        var t = order[j];
        order[j] = order[k];
        order[k] = t;
      }
    }
    return order;
  }

  function newState(n, shuffle, rand) {
    var order = makeOrder(n, shuffle, rand);
    var seen = [];
    for (var i = 0; i < n; i++) seen.push(false);
    seen[order[0]] = true;
    return { order: order, pos: 0, seen: seen, elapsed: 0, back: false };
  }

  function copy(s, patch) {
    return {
      order: s.order,
      pos: patch.pos !== undefined ? patch.pos : s.pos,
      seen: patch.seen !== undefined ? patch.seen : s.seen,
      elapsed: patch.elapsed !== undefined ? patch.elapsed : s.elapsed,
      back: patch.back !== undefined ? patch.back : s.back,
    };
  }

  function current(s) {
    return s.order[s.pos];
  }

  // dir is 1 (another idea) or -1 (previous idea); the list wraps round.
  function move(s, dir) {
    var n = s.order.length;
    var pos = (((s.pos + dir) % n) + n) % n;
    var seen = s.seen.slice();
    seen[s.order[pos]] = true;
    return copy(s, { pos: pos, seen: seen });
  }

  function seenCount(s) {
    var c = 0;
    for (var i = 0; i < s.seen.length; i++) if (s.seen[i]) c++;
    return c;
  }

  // Time away only grows while the person is away (not back at the desk) and the host is
  // running us. dt is clamped so a long sleep or a clock jump never adds more than 2 seconds.
  function addTime(s, dt, hostRunning) {
    if (!hostRunning || s.back) return s;
    var step = Math.min(2, Math.max(0, dt));
    return copy(s, { elapsed: s.elapsed + step });
  }

  function setBack(s, back) {
    return copy(s, { back: !!back });
  }

  function snapshot(s) {
    return {
      v: 1,
      order: s.order.slice(),
      pos: s.pos,
      seen: s.seen.slice(),
      elapsed: Math.floor(s.elapsed), // whole seconds: keeps saves stable between ticks
      back: s.back,
    };
  }

  function restore(raw, n, shuffle, rand) {
    var fresh = newState(n, shuffle, rand);
    if (!raw || typeof raw !== "object" || raw.v !== 1) return fresh;
    if (!Array.isArray(raw.order) || raw.order.length !== n) return fresh;
    var used = {};
    for (var i = 0; i < n; i++) {
      var o = raw.order[i];
      if (!Number.isInteger(o) || o < 0 || o >= n || used[o]) return fresh;
      used[o] = true;
    }
    if (!Number.isInteger(raw.pos) || raw.pos < 0 || raw.pos >= n) return fresh;
    if (!Array.isArray(raw.seen) || raw.seen.length !== n) return fresh;
    var seen = [];
    for (var j = 0; j < n; j++) seen.push(raw.seen[j] === true);
    seen[raw.order[raw.pos]] = true;
    var elapsed =
      typeof raw.elapsed === "number" && isFinite(raw.elapsed) && raw.elapsed >= 0
        ? raw.elapsed
        : 0;
    return {
      order: raw.order.slice(),
      pos: raw.pos,
      seen: seen,
      elapsed: elapsed,
      back: raw.back === true,
    };
  }

  function clock(seconds) {
    var s = Math.max(0, Math.floor(seconds));
    var h = Math.floor(s / 3600);
    var m = Math.floor((s % 3600) / 60);
    var r = s % 60;
    function p(x) {
      return x < 10 ? "0" + x : String(x);
    }
    return (h > 0 ? h + ":" + p(m) : p(m)) + ":" + p(r);
  }

  function awayText(seconds) {
    var m = Math.floor(seconds / 60);
    return m < 1 ? "Under a minute" : m + " min";
  }

  root.AwayLogic = {
    SAMPLE: SAMPLE,
    CATEGORY_LABELS: CATEGORY_LABELS,
    validData: validData,
    makeOrder: makeOrder,
    newState: newState,
    current: current,
    move: move,
    seenCount: seenCount,
    addTime: addTime,
    setBack: setBack,
    snapshot: snapshot,
    restore: restore,
    clock: clock,
    awayText: awayText,
  };
})(globalThis);
