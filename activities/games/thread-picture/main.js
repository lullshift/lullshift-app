// Thread Picture: a picture-by-numbers puzzle on the grid kit.
(function () {
  "use strict";
  var L = globalThis.ThreadLogic;
  var K = window.LullshiftKits.grid;

  var size = 5;
  var grid = null;
  var view = null;
  var puzzle = null;
  var game = null; // { seed, mode, size, history, moves, solved }
  var tool = "fill"; // what Enter, Space and a click do: "fill" or "mark"
  var bestToday = null;
  var lastShift = false;

  function $(id) {
    return document.getElementById(id);
  }

  function setCells(cells) {
    for (var i = 0; i < cells.length; i++) grid.set(i % size, Math.floor(i / size), cells[i]);
  }

  function cellsNow() {
    return grid.cells.slice();
  }

  function clueText(clue) {
    return clue.join(" ");
  }

  function buildBoard(newSize) {
    size = newSize;
    if (view) view.destroy();
    grid = new K.Grid(size, size, 0);
    $("grid").setAttribute("data-size", String(size));
    $("grid").parentNode.setAttribute("data-size", String(size));
    view = K.bindGrid($("grid"), grid, {
      name: "Thread Picture, " + size + " by " + size,
      label: label,
      className: classes,
      onActivate: function (x, y) {
        if (lastShift) apply(x, y, "mark");
        else apply(x, y, tool);
        lastShift = false;
      },
      onMove: function () {
        highlight();
        save();
      },
      onAction: function (action) {
        if (action === "undo") {
          undoMove();
          return true;
        }
        return false;
      },
    });
    buildClues();
  }

  function buildClues() {
    var cols = $("colclues");
    var rows = $("rowclues");
    cols.textContent = "";
    rows.textContent = "";
    cols.style.gridTemplateColumns = "repeat(" + size + ", 1fr)";
    rows.style.gridTemplateRows = "repeat(" + size + ", 1fr)";
    var i;
    for (i = 0; i < size; i++) {
      var c = document.createElement("div");
      c.className = "clue col";
      c.setAttribute("aria-hidden", "true");
      puzzle.cols[i].forEach(function (n) {
        var s = document.createElement("span");
        s.textContent = String(n);
        c.appendChild(s);
      });
      cols.appendChild(c);
      var r = document.createElement("div");
      r.className = "clue row";
      r.setAttribute("aria-hidden", "true");
      r.textContent = clueText(puzzle.rows[i]);
      rows.appendChild(r);
    }
  }

  function highlight() {
    var kids = $("colclues").children;
    var rkids = $("rowclues").children;
    for (var i = 0; i < kids.length; i++) {
      kids[i].classList.toggle("on", i === grid.cursor.x);
      rkids[i].classList.toggle("on", i === grid.cursor.y);
    }
  }

  function stateWord(v) {
    return v === 1 ? "filled" : v === 2 ? "marked empty" : "unknown";
  }

  function label(v, x, y) {
    return (
      "Row " +
      (y + 1) +
      ", column " +
      (x + 1) +
      ", " +
      stateWord(v) +
      ". Row clue " +
      puzzle.rows[y].join(", ") +
      ". Column clue " +
      puzzle.cols[x].join(", ") +
      "."
    );
  }

  function classes(v) {
    return v === 1 ? "filled" : v === 2 ? "empty" : "";
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function movesText(n) {
    return n + (n === 1 ? " move" : " moves");
  }

  function startPuzzle(seed, mode, sz) {
    if (sz !== size || !grid) {
      puzzle = L.generate(seed, sz);
      buildBoard(sz);
    } else {
      puzzle = L.generate(seed, sz);
      buildClues();
    }
    game = { seed: seed, mode: mode, size: sz, history: [], moves: 0, solved: false };
    setCells(L.blankCells(sz));
    grid.setCursor(0, 0);
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          size: game.size,
          cells: cellsNow(),
          history: game.history,
          moves: game.moves,
          tool: tool,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validState(r) {
    return (
      r &&
      r.v === 1 &&
      Number.isInteger(r.seed) &&
      L.isSize(r.size) &&
      Array.isArray(r.cells) &&
      r.cells.length === r.size * r.size &&
      r.cells.every(L.isCellState) &&
      Array.isArray(r.history)
    );
  }

  function restoreGame(r) {
    startPuzzle(r.seed, r.mode === "daily" ? "daily" : "random", r.size);
    setCells(r.cells);
    game.history = r.history
      .filter(function (h) {
        return (
          h &&
          Number.isInteger(h.i) &&
          h.i >= 0 &&
          h.i < size * size &&
          L.isCellState(h.from) &&
          L.isCellState(h.to)
        );
      })
      .slice(-L.HISTORY_LIMIT);
    game.moves = Number.isInteger(r.moves) && r.moves >= 0 ? r.moves : 0;
    if (r.tool === "mark") tool = "mark";
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
    game.solved = L.isSolved(cellsNow(), size, puzzle.rows, puzzle.cols);
  }

  function render() {
    view.render();
    highlight();
    var filled = L.filledCount(grid.cells);
    $("moves").textContent = "Moves: " + game.moves;
    $("filled").textContent = "Filled " + filled + " of " + puzzle.total;
    $("undo").disabled = game.history.length === 0 || game.solved;
    $("tool").textContent = tool === "fill" ? "Tool: Fill" : "Tool: Mark";
    $("tool").setAttribute("aria-pressed", tool === "mark" ? "true" : "false");
    $("size").value = String(size);
    var best = $("best");
    if (game.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Best today: " + bestToday;
    } else {
      best.hidden = true;
    }
    var items = [
      { label: "Moves", value: game.moves },
      { label: "Filled", value: filled + " of " + puzzle.total },
    ];
    if (game.mode === "daily" && bestToday !== null) {
      items.push({ label: "Best today", value: bestToday });
    }
    LullshiftBridge.summary(items);
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function bestKey() {
    return "best-" + game.seed;
  }

  function loadBest() {
    bestToday = null;
    if (game.mode !== "daily") return;
    var seed = game.seed;
    try {
      LullshiftBridge.storage.get(bestKey()).then(
        function (v) {
          var n = parseInt(v, 10);
          if (game && game.seed === seed && Number.isFinite(n) && n > 0) {
            bestToday = n;
            render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: show nothing */
    }
  }

  function recordBest() {
    if (game.mode !== "daily") return;
    if (bestToday !== null && bestToday <= game.moves) return;
    bestToday = game.moves;
    try {
      LullshiftBridge.storage.set(bestKey(), game.moves).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function apply(x, y, which) {
    if (game.solved) return;
    var i = y * size + x;
    var before = grid.cells[i];
    var next = which === "mark" ? L.toggleMark(grid.cells, i) : L.toggleFill(grid.cells, i);
    var after = next[i];
    if (after === before) return;
    grid.set(x, y, after);
    game.history = L.pushMove(game.history, i, before, after);
    game.moves += 1;
    render();
    if (L.isSolved(cellsNow(), size, puzzle.rows, puzzle.cols)) {
      game.solved = true;
      recordBest();
      setMessage("Picture complete in " + movesText(game.moves) + ". Well done.");
      render();
    } else {
      setMessage("Row " + (y + 1) + ", column " + (x + 1) + " " + stateWord(after) + ".");
    }
    save();
  }

  function undoMove() {
    if (game.history.length === 0 || game.solved) return;
    var r = L.undo(cellsNow(), game.history);
    setCells(r.cells);
    game.history = r.history;
    game.moves = Math.max(0, game.moves - 1);
    setMessage("Undone. " + movesText(game.moves) + ".");
    render();
    save();
  }

  function restart() {
    setCells(L.blankCells(size));
    game.history = [];
    game.moves = 0;
    game.solved = false;
    setMessage("Picture cleared.");
    render();
    save();
  }

  function newPuzzle(seed, mode, sz, text) {
    startPuzzle(seed, mode, sz);
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function chosenSize() {
    var n = parseInt($("size").value, 10);
    return L.isSize(n) ? n : size;
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreGame(init.restore);
    else startPuzzle(L.dateSeed(new Date(), 5), "daily", 5);

    $("grid").addEventListener(
      "click",
      function (e) {
        lastShift = !!e.shiftKey;
      },
      true,
    );
    $("grid").addEventListener("keydown", function (e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      var k = e.key;
      if (k === "x" || k === "X") {
        e.preventDefault();
        apply(grid.cursor.x, grid.cursor.y, "mark");
      } else if (k === "f" || k === "F") {
        e.preventDefault();
        apply(grid.cursor.x, grid.cursor.y, "fill");
      }
    });

    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      var s = chosenSize();
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " puzzle.");
    });
    $("fresh").addEventListener("click", function () {
      var s = chosenSize();
      newPuzzle(K.randomSeed(), "random", s, "A new " + s + " by " + s + " puzzle.");
    });
    $("size").addEventListener("change", function () {
      var s = chosenSize();
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " puzzle.");
    });
    $("tool").addEventListener("click", function () {
      tool = tool === "fill" ? "mark" : "fill";
      setMessage(tool === "fill" ? "Tool: fill cells." : "Tool: mark cells empty.");
      render();
      save();
    });

    if (restored) {
      if (game.solved) setMessage("Picture complete in " + movesText(game.moves) + ".");
      loadBest();
      render();
    } else {
      setMessage("Today's 5 by 5 puzzle.");
      loadBest();
      render();
      save();
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
  });
})();
