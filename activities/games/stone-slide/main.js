(function () {
  "use strict";
  var K = window.LullshiftKits.grid;
  var L = globalThis.StoneSlideLogic;
  var state = null;
  var cells = [];
  var focusIndex = 0;
  var best = {};

  function $(id) {
    return document.getElementById(id);
  }

  function plural(n, word) {
    return n + " " + word + (n === 1 ? "" : "s");
  }

  function begin(size, seed, mode) {
    var start = L.generate(size, seed);
    state = {
      version: 1,
      size: size,
      seed: seed,
      mode: mode,
      start: start,
      tiles: start.slice(),
      stack: [],
      solved: false,
    };
    focusIndex = 0;
  }

  function restoreState(r) {
    if (!K.isRecord(r) || r.version !== 1 || L.SIZES.indexOf(r.size) < 0) return null;
    if (!L.validTiles(r.tiles, r.size) || !L.validTiles(r.start, r.size)) return null;
    if (!Array.isArray(r.stack) || !K.isIntIn(r.seed, 0, 4294967295)) return null;
    var n = r.size * r.size;
    for (var i = 0; i < r.stack.length; i++) if (!K.isIntIn(r.stack[i], 0, n - 1)) return null;
    focusIndex = K.isIntIn(r.focus, 0, n - 1) ? r.focus : 0;
    return {
      version: 1,
      size: r.size,
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      start: r.start,
      tiles: r.tiles,
      stack: r.stack,
      solved: L.isSolved(r.tiles),
    };
  }

  function snapshot() {
    if (!state) return undefined;
    return {
      version: 1,
      size: state.size,
      seed: state.seed,
      mode: state.mode,
      start: state.start,
      tiles: state.tiles,
      stack: state.stack,
      solved: state.solved,
      focus: focusIndex,
    };
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function say(text) {
    $("message").textContent = text;
  }

  function stoneLabel(i) {
    var n = state.size;
    var v = state.tiles[i];
    var where = "row " + (Math.floor(i / n) + 1) + ", column " + ((i % n) + 1);
    if (v === 0) return "Empty space, " + where;
    return "Stone " + v + ", " + where + (L.canSlide(state.tiles, n, i) ? ", can slide" : "");
  }

  function buildBoard() {
    var board = $("board");
    board.textContent = "";
    cells = [];
    var n = state.size;
    board.className = "board size" + n;
    board.setAttribute("aria-label", "Stones, " + n + " by " + n);
    for (var r = 0; r < n; r++) {
      var row = document.createElement("div");
      row.className = "row";
      row.setAttribute("role", "row");
      for (var c = 0; c < n; c++) {
        var cell = document.createElement("div");
        cell.className = "cell";
        cell.setAttribute("role", "gridcell");
        var b = document.createElement("button");
        b.type = "button";
        b.dataset.index = String(r * n + c);
        var num = document.createElement("span");
        num.className = "num";
        b.appendChild(num);
        cell.appendChild(b);
        row.appendChild(cell);
        cells.push(b);
      }
      board.appendChild(row);
    }
  }

  function render() {
    var n = state.size;
    for (var i = 0; i < cells.length; i++) {
      var v = state.tiles[i];
      var b = cells[i];
      if (v === 0) {
        b.className = "gap";
        b.firstChild.textContent = "";
      } else {
        var movable = L.canSlide(state.tiles, n, i) ? " movable" : "";
        b.className = "stone h" + (v % 8) + " p" + (v % 4) + movable;
        b.firstChild.textContent = String(v);
      }
      b.setAttribute("aria-label", stoneLabel(i));
      b.tabIndex = i === focusIndex ? 0 : -1;
    }
    $("moves").textContent = "Moves: " + state.stack.length;
    var bb = best[n];
    $("best").hidden = !bb;
    if (bb) $("best").textContent = "Best (" + n + " x " + n + "): " + bb;
    $("undo").disabled = state.stack.length === 0 || state.solved;
    $("size").textContent = "Size: " + n + " x " + n;
    $("size").setAttribute(
      "aria-label",
      "Size: " + n + " by " + n + ". Press to switch to " + (n === 4 ? "3 by 3" : "4 by 4"),
    );
    report();
  }

  function report() {
    var items = [{ label: "Moves", value: state.stack.length }];
    if (best[state.size]) items.push({ label: "Best", value: best[state.size] });
    items.push({ label: "Board", value: state.size + " x " + state.size });
    LullshiftBridge.summary(items);
  }

  function loadBest(size) {
    try {
      LullshiftBridge.storage.get("best-" + size).then(
        function (v) {
          var n = parseInt(v, 10);
          if (Number.isFinite(n) && n > 0) {
            best[size] = n;
            if (state) render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: no best shown */
    }
  }

  function recordBest() {
    var moves = state.stack.length;
    var size = state.size;
    if (best[size] && best[size] <= moves) return;
    best[size] = moves;
    try {
      LullshiftBridge.storage.set("best-" + size, moves).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function moveFocus(i) {
    cells[focusIndex].tabIndex = -1;
    focusIndex = i;
    cells[i].tabIndex = 0;
    cells[i].focus();
  }

  function doSlide(i) {
    if (state.solved) return;
    var g = L.gapIndex(state.tiles);
    var tile = state.tiles[i];
    var next = L.slide(state.tiles, state.size, i);
    if (!next) {
      say(tile === 0 ? "That is the empty space." : "Stone " + tile + " cannot slide yet.");
      return;
    }
    state.tiles = next;
    state.stack = L.push(state.stack, g);
    var moves = state.stack.length;
    if (L.isSolved(next)) {
      state.solved = true;
      recordBest();
      say("Solved in " + plural(moves, "move") + ". Well done.");
    } else {
      say("Slid stone " + tile + ". " + plural(moves, "move") + ".");
    }
    // The slid stone now sits where the gap was, so keep the focus on it.
    focusIndex = g;
    render();
    cells[g].focus();
    save();
  }

  function doUndo() {
    if (state.solved || state.stack.length === 0) return;
    var r = L.undo(state.tiles, state.size, state.stack);
    state.tiles = r.tiles;
    state.stack = r.stack;
    say("Undone. " + plural(state.stack.length, "move") + ".");
    render();
    save();
  }

  function doRestart() {
    state.tiles = state.start.slice();
    state.stack = [];
    state.solved = false;
    say("Stones reset.");
    render();
    save();
  }

  function startNew(size, seed, mode, text) {
    begin(size, seed, mode);
    buildBoard();
    say(text);
    loadBest(size);
    render();
    save();
  }

  function onKey(e) {
    var t = e.target;
    if (!t || !t.dataset || t.dataset.index === undefined) return;
    var n = state.size;
    var i = Number(t.dataset.index);
    var action = K.keyAction(e);
    var r = Math.floor(i / n);
    var c = i % n;
    var next = null;
    if (action === "right") next = r * n + Math.min(n - 1, c + 1);
    else if (action === "left") next = r * n + Math.max(0, c - 1);
    else if (action === "down") next = Math.min(n - 1, r + 1) * n + c;
    else if (action === "up") next = Math.max(0, r - 1) * n + c;
    else if (action === "home") next = r * n;
    else if (action === "end") next = r * n + n - 1;
    else if (action === "first") next = 0;
    else if (action === "last") next = n * n - 1;
    else if (action === "undo") {
      e.preventDefault();
      doUndo();
      return;
    } else if (action === "activate") {
      e.preventDefault();
      doSlide(i);
      return;
    }
    if (next === null) return;
    e.preventDefault();
    moveFocus(next);
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");
    $("board").addEventListener("keydown", onKey);
    $("board").addEventListener("click", function (e) {
      var b = e.target.closest ? e.target.closest("button") : null;
      if (!b || b.dataset.index === undefined) return;
      var i = Number(b.dataset.index);
      cells[focusIndex].tabIndex = -1;
      focusIndex = i;
      b.tabIndex = 0;
      doSlide(i);
    });
    $("undo").addEventListener("click", doUndo);
    $("restart").addEventListener("click", doRestart);
    $("daily").addEventListener("click", function () {
      startNew(state.size, L.dailySeed(state.size), "daily", "Today's stones.");
    });
    $("fresh").addEventListener("click", function () {
      startNew(state.size, K.randomSeed(), "random", "New stones.");
    });
    $("size").addEventListener("click", function () {
      var size = state.size === 4 ? 3 : 4;
      var daily = state.mode === "daily";
      startNew(
        size,
        daily ? L.dailySeed(size) : K.randomSeed(),
        state.mode,
        "Now " + size + " by " + size + ".",
      );
    });

    var restored = restoreState(init.restore);
    if (restored) {
      state = restored;
      buildBoard();
      loadBest(state.size);
      if (state.solved) say("Solved in " + plural(state.stack.length, "move") + ".");
      render();
      save();
    } else {
      startNew(4, L.dailySeed(4), "daily", "Today's stones.");
    }
  }

  LullshiftBridge.start({ onInit: setup, saveState: snapshot });
})();
