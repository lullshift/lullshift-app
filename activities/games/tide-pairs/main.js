(function () {
  "use strict";
  var L = globalThis.TidePairsLogic;
  var K = globalThis.LullshiftKits.card;
  var PAUSE_MS = 900;
  var state = null;
  var tiles = [];
  var focusIndex = 0;
  var reduced = false;
  var timer = null;
  var best = null; // fewest turns for the current size, if known
  var lastReport = "";

  function $(id) {
    return document.getElementById(id);
  }

  function cols() {
    return L.COLUMNS[state.pairs];
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function title(name) {
    return name.charAt(0).toUpperCase() + name.slice(1);
  }

  function slotLabel(i) {
    var c = cols();
    var base = "Card row " + (Math.floor(i / c) + 1) + ", column " + ((i % c) + 1) + ", ";
    if (!L.isUp(state, i)) return base + "face down";
    return base + L.symbolAt(state, i) + (state.matched[i] ? ", matched" : ", face up");
  }

  function buildBoard() {
    var board = $("board");
    board.textContent = "";
    tiles = [];
    var c = cols();
    var rows = (state.pairs * 2) / c;
    board.className = "board cols-" + c;
    board.setAttribute("aria-label", "Tide pairs, " + rows + " by " + c);
    if (focusIndex >= state.pairs * 2) focusIndex = 0;
    for (var i = 0; i < state.pairs * 2; i++) {
      var b = document.createElement("button");
      b.type = "button";
      b.dataset.index = String(i);
      tiles.push(b);
      board.appendChild(b);
    }
  }

  function render() {
    for (var i = 0; i < tiles.length; i++) {
      var b = tiles[i];
      var up = L.isUp(state, i);
      var matched = state.matched[i];
      b.className = "tile " + (up ? (matched ? "matched" : "up") : "down");
      b.setAttribute("aria-label", slotLabel(i));
      b.tabIndex = i === focusIndex ? 0 : -1;
      b.textContent = "";
      if (up) {
        var name = L.symbolAt(state, i);
        var shape = document.createElement("span");
        shape.className = "shape s-" + name;
        shape.setAttribute("aria-hidden", "true");
        var cap = document.createElement("span");
        cap.className = "caption";
        cap.setAttribute("aria-hidden", "true");
        cap.textContent = name;
        b.appendChild(shape);
        b.appendChild(cap);
      }
    }
    var found = L.pairsFound(state);
    $("pairs").textContent = "Pairs: " + found + " of " + state.pairs;
    $("turns").textContent = "Turns: " + state.turns;
    var bestEl = $("best");
    if (best !== null) {
      bestEl.hidden = false;
      bestEl.textContent = "Best: " + best;
    } else {
      bestEl.hidden = true;
    }
    $("size").textContent = state.pairs === 8 ? "Bigger" : "Smaller";
    $("size").setAttribute(
      "aria-label",
      state.pairs === 8 ? "Bigger board, 12 pairs" : "Smaller board, 8 pairs",
    );
    report();
  }

  // What the host shows while the game is paused (S05-T09).
  function report() {
    var found = L.pairsFound(state);
    var sig = [found, state.pairs, state.turns, best].join("/");
    if (sig === lastReport) return;
    lastReport = sig;
    var items = [
      { label: "Pairs", value: found + " of " + state.pairs },
      { label: "Turns", value: state.turns },
    ];
    if (best !== null) items.push({ label: "Best", value: best });
    LullshiftBridge.summary(items);
  }

  function snapshot() {
    return state ? L.snapshot(state) : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function bestKey() {
    return "best-" + state.pairs;
  }
  function loadBest() {
    best = null;
    var pairs = state.pairs;
    try {
      LullshiftBridge.storage.get("best-" + pairs).then(
        function (v) {
          var n = parseInt(v, 10);
          if (state && state.pairs === pairs && Number.isFinite(n) && n > 0) {
            best = n;
            render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: show no best */
    }
  }
  function recordBest() {
    if (best !== null && best <= state.turns) return;
    best = state.turns;
    try {
      LullshiftBridge.storage.set(bestKey(), state.turns).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function clearTimer() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  // Turns an unmatched pair back down. Safe to call at any time.
  function settle() {
    clearTimer();
    if (!L.needsSettle(state)) return false;
    state = L.settle(state);
    render();
    save();
    return true;
  }

  function winText() {
    return (
      "All " +
      state.pairs +
      " pairs found in " +
      state.turns +
      (state.turns === 1 ? " turn." : " turns.") +
      " Well done."
    );
  }

  function pressTile(i) {
    // A pair that did not match is still showing: turn it back first, then take the press.
    if (L.needsSettle(state)) settle();
    if (L.isUp(state, i)) {
      if (state.matched[i]) setMessage(title(L.symbolAt(state, i)) + " is already matched.");
      return;
    }
    var r = L.flip(state, i);
    if (r.result === "ignored") return;
    state = r.state;
    focusIndex = i;
    var name = title(L.symbolAt(state, i));
    if (r.result === "first") {
      setMessage(name + ". Now find its pair.");
    } else if (r.result === "match") {
      if (L.isWon(state)) {
        recordBest();
        setMessage(winText());
      } else {
        setMessage(name + " matched. " + L.pairsFound(state) + " of " + state.pairs + " pairs.");
      }
    } else {
      var first = title(L.symbolAt(state, state.open[0]));
      if (reduced) {
        setMessage(first + " and " + name + " do not match. Press any card to turn them back.");
      } else {
        setMessage(first + " and " + name + " do not match.");
        timer = setTimeout(function () {
          timer = null;
          settle();
        }, PAUSE_MS);
      }
    }
    render();
    save();
  }

  function startGame(seed, pairs, mode, text) {
    clearTimer();
    state = L.deal(seed, pairs, mode);
    focusIndex = 0;
    lastReport = "";
    buildBoard();
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function onBoardKey(e) {
    var t = e.target;
    if (!t || !t.dataset || t.dataset.index === undefined) return;
    var action = K.keyAction(e);
    if (!action) return;
    var c = cols();
    var n = state.pairs * 2;
    var i = Number(t.dataset.index);
    var r = Math.floor(i / c);
    var col = i % c;
    var next = null;
    if (action === "right") next = r * c + Math.min(c - 1, col + 1);
    else if (action === "left") next = r * c + Math.max(0, col - 1);
    else if (action === "down") next = Math.min(n / c - 1, r + 1) * c + col;
    else if (action === "up") next = Math.max(0, r - 1) * c + col;
    else if (action === "home") next = r * c;
    else if (action === "end") next = r * c + c - 1;
    else if (action === "first") next = 0;
    else if (action === "last") next = n - 1;
    if (next === null) return;
    e.preventDefault();
    tiles[focusIndex].tabIndex = -1;
    focusIndex = next;
    tiles[next].tabIndex = 0;
    tiles[next].focus();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    reduced = init.reducedMotion === true;
    if (reduced) document.documentElement.classList.add("reduced");
    $("board").addEventListener("keydown", onBoardKey);
    $("board").addEventListener("click", function (e) {
      var b = e.target.closest ? e.target.closest("button.tile") : null;
      if (b) pressTile(Number(b.dataset.index));
    });
    $("daily").addEventListener("click", function () {
      startGame(
        L.dailySeed(new Date()),
        state.pairs,
        "daily",
        "Today's deal. Everyone gets the same shells today.",
      );
    });
    $("fresh").addEventListener("click", function () {
      startGame(K.randomSeed(), state.pairs, "random", "A new deal.");
    });
    $("size").addEventListener("click", function () {
      var pairs = state.pairs === 8 ? 12 : 8;
      startGame(
        K.randomSeed(),
        pairs,
        "random",
        pairs === 12 ? "A bigger board: 12 pairs." : "A smaller board: 8 pairs.",
      );
    });

    var restored = init.restore ? L.restore(init.restore) : null;
    if (restored) {
      state = restored;
      // A pair that did not match turns back when the game comes back.
      if (L.needsSettle(state)) state = L.settle(state);
      buildBoard();
      setMessage(L.isWon(state) ? winText() : "");
      loadBest();
      render();
      save();
    } else {
      startGame(
        L.dailySeed(new Date()),
        8,
        "daily",
        "Today's deal. Turn two shells to find a pair.",
      );
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
    onPause: function () {
      clearTimer();
    },
    onResume: function () {
      // The pause stopped the turn-back timer: turn an unmatched pair back down now.
      if (state) settle();
    },
  });
})();
