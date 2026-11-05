(function () {
  "use strict";
  var K = window.LullshiftKits.grid;
  var L = globalThis.PetalGridLogic;
  var state = null;
  var cells = [];
  var focusIndex = 0;

  function $(id) {
    return document.getElementById(id);
  }

  function cap(s) {
    return s.charAt(0).toUpperCase() + s.slice(1);
  }

  function begin(size, seed, mode) {
    var puzzle = L.generate(size, seed);
    state = {
      version: 1,
      mode: mode,
      puzzle: puzzle,
      cells: L.startCells(puzzle),
      stack: [],
      solved: false,
    };
    focusIndex = 0;
  }

  function restoreState(r) {
    if (!K.isRecord(r) || r.version !== 1 || !L.validPuzzle(r.puzzle)) return null;
    if (!L.validCells(r.puzzle, r.cells) || !Array.isArray(r.stack)) return null;
    var n = r.puzzle.size * r.puzzle.size;
    for (var i = 0; i < r.stack.length; i++) {
      var e = r.stack[i];
      if (!K.isRecord(e) || !K.isIntIn(e.i, 0, n - 1) || !K.isIntIn(e.prev, 0, r.puzzle.size)) {
        return null;
      }
    }
    focusIndex = K.isIntIn(r.focus, 0, n - 1) ? r.focus : 0;
    return {
      version: 1,
      mode: r.mode === "daily" ? "daily" : "random",
      puzzle: r.puzzle,
      cells: r.cells,
      stack: r.stack,
      solved: L.isSolved(r.puzzle.size, r.cells),
    };
  }

  function snapshot() {
    if (!state) return undefined;
    return {
      version: 1,
      mode: state.mode,
      puzzle: state.puzzle,
      cells: state.cells,
      stack: state.stack,
      focus: focusIndex,
    };
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function say(text) {
    $("message").textContent = text;
  }

  function size() {
    return state.puzzle.size;
  }

  function cellLabel(i, bad) {
    var n = size();
    var v = state.cells[i];
    var text = "Row " + (Math.floor(i / n) + 1) + ", column " + ((i % n) + 1) + ", ";
    if (v === 0) text += "empty";
    else text += L.petalName(v) + " (" + v + ")";
    if (state.puzzle.givens[i] !== 0) text += ", given";
    if (bad) text += ", conflict";
    return text;
  }

  function buildBoard() {
    var board = $("board");
    board.textContent = "";
    cells = [];
    var n = size();
    board.className = "board size" + n;
    board.setAttribute("aria-label", "Flower bed, " + n + " by " + n);
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
        var dot = document.createElement("span");
        dot.className = "dot";
        var num = document.createElement("span");
        num.className = "num";
        b.appendChild(dot);
        b.appendChild(num);
        cell.appendChild(b);
        row.appendChild(cell);
        cells.push(b);
      }
      board.appendChild(row);
    }
    var legend = $("legend");
    legend.textContent = "";
    for (var k = 1; k <= n; k++) {
      var li = document.createElement("li");
      var d = document.createElement("span");
      d.className = "dot k" + k;
      d.setAttribute("aria-hidden", "true");
      var t = document.createElement("span");
      t.textContent = k + " " + L.petalName(k);
      li.appendChild(d);
      li.appendChild(t);
      legend.appendChild(li);
    }
  }

  function render() {
    var n = size();
    var bad = L.conflicts(n, state.cells);
    var badSet = {};
    bad.forEach(function (i) {
      badSet[i] = true;
    });
    for (var i = 0; i < cells.length; i++) {
      var v = state.cells[i];
      var b = cells[i];
      var given = state.puzzle.givens[i] !== 0;
      b.className = (given ? "given" : "free") + (badSet[i] ? " bad" : "") + (v ? " filled" : "");
      b.firstChild.className = "dot" + (v ? " k" + v : "");
      b.lastChild.textContent = v ? String(v) : "";
      b.setAttribute("aria-label", cellLabel(i, !!badSet[i]));
      b.tabIndex = i === focusIndex ? 0 : -1;
    }
    var filled = L.filledCount(state.cells);
    $("filled").textContent = "Filled " + filled + " of " + n * n;
    $("conflicts").textContent = "Conflicts: " + bad.length;
    $("undo").disabled = state.stack.length === 0;
    $("clear").disabled = state.stack.length === 0;
    $("size").textContent = "Size: " + n + " x " + n;
    $("size").setAttribute(
      "aria-label",
      "Size: " + n + " by " + n + ". Press to switch to " + (n === 4 ? "6 by 6" : "4 by 4"),
    );
    LullshiftBridge.summary([
      { label: "Filled", value: filled + " of " + n * n },
      { label: "Conflicts", value: bad.length },
    ]);
  }

  function moveFocus(i) {
    cells[focusIndex].tabIndex = -1;
    focusIndex = i;
    cells[i].tabIndex = 0;
    cells[i].focus();
  }

  function describeConflict(i) {
    var n = size();
    var v = state.cells[i];
    var r = Math.floor(i / n);
    var c = i % n;
    var inRow = false;
    var inCol = false;
    for (var k = 0; k < n; k++) {
      if (k !== c && state.cells[r * n + k] === v) inRow = true;
      if (k !== r && state.cells[k * n + c] === v) inCol = true;
    }
    if (!inRow && !inCol) return "";
    var where = inRow && inCol ? "row " + (r + 1) + " and column " + (c + 1) : "";
    if (!where) where = inRow ? "row " + (r + 1) : "column " + (c + 1);
    return cap(L.petalName(v)) + " already appears in " + where + ".";
  }

  function put(i, value) {
    if (state.puzzle.givens[i] !== 0) {
      say("That petal was given and cannot change.");
      return;
    }
    var prev = state.cells[i];
    var next = L.place(state.puzzle, state.cells, i, value);
    if (!next) return;
    state.cells = next;
    state.stack = L.push(state.stack, i, prev);
    state.solved = L.isSolved(size(), next);
    if (state.solved) {
      say("Solved. Every row and column has each petal once. Well done.");
    } else if (value === 0) {
      say("Cleared.");
    } else {
      var warn = describeConflict(i);
      say(cap(L.petalName(value)) + " planted." + (warn ? " " + warn : ""));
    }
    render();
    save();
  }

  function doUndo() {
    if (state.stack.length === 0) return;
    var r = L.undo(state.cells, state.stack);
    state.cells = r.cells;
    state.stack = r.stack;
    state.solved = L.isSolved(size(), r.cells);
    say("Undone.");
    render();
    save();
  }

  function doClear() {
    if (state.stack.length === 0) return;
    state.cells = L.startCells(state.puzzle);
    state.stack = [];
    state.solved = false;
    say("Your petals are cleared.");
    render();
    save();
  }

  function startNew(n, seed, mode, text) {
    begin(n, seed, mode);
    buildBoard();
    say(text);
    render();
    save();
  }

  function onKey(e) {
    var t = e.target;
    if (!t || !t.dataset || t.dataset.index === undefined) return;
    var n = size();
    var i = Number(t.dataset.index);
    var r = Math.floor(i / n);
    var c = i % n;
    var next = null;
    var action = K.keyAction(e);
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
    } else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
      if (e.key === "Backspace" || e.key === "Delete" || e.key === "0") {
        e.preventDefault();
        put(i, 0);
        return;
      }
      if (/^[1-9]$/.test(e.key)) {
        e.preventDefault();
        var v = Number(e.key);
        if (v <= n) put(i, v);
        else say("This bed uses petals 1 to " + n + ".");
        return;
      }
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
      moveFocus(Number(b.dataset.index));
    });
    $("undo").addEventListener("click", doUndo);
    $("clear").addEventListener("click", doClear);
    $("daily").addEventListener("click", function () {
      startNew(size(), L.dailySeed(size()), "daily", "Today's bed.");
    });
    $("fresh").addEventListener("click", function () {
      startNew(size(), K.randomSeed(), "random", "A new bed.");
    });
    $("size").addEventListener("click", function () {
      var n = size() === 4 ? 6 : 4;
      var daily = state.mode === "daily";
      startNew(
        n,
        daily ? L.dailySeed(n) : K.randomSeed(),
        state.mode,
        "Now " + n + " by " + n + ".",
      );
    });

    var restored = restoreState(init.restore);
    if (restored) {
      state = restored;
      buildBoard();
      if (state.solved) say("Solved. Every row and column has each petal once.");
      render();
      save();
    } else {
      startNew(4, L.dailySeed(4), "daily", "Today's bed.");
    }
  }

  LullshiftBridge.start({ onInit: setup, saveState: snapshot });
})();
