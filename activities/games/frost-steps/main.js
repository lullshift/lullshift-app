(function () {
  "use strict";
  var L = globalThis.FrostLogic;
  var K = window.LullshiftKits.grid;
  // Cell values: 0 ice, 1 rock, 2 lantern, 3 walker on ice, 4 walker on the lantern.
  var ICE = 0;
  var ROCK = 1;
  var LANTERN = 2;
  var WALKER = 3;
  var WALKER_HOME = 4;
  var HISTORY_LIMIT = 200;
  var DIR_TEXT = { up: "up", down: "down", left: "left", right: "right" };

  var puzzle = null;
  var mode = "daily";
  var seed = 0;
  var grid = null;
  var view = null;
  var moves = 0;
  var trail = [];
  var bestToday = null;

  function $(id) {
    return document.getElementById(id);
  }

  function isCell(v) {
    return Number.isInteger(v) && v >= 0 && v <= 4;
  }

  function startCells(p) {
    var rocks = {};
    p.rocks.forEach(function (r) {
      rocks[r] = true;
    });
    return function (x, y) {
      var i = y * p.w + x;
      if (rocks[i]) return ROCK;
      if (i === p.start) return WALKER;
      if (i === p.goal) return LANTERN;
      return ICE;
    };
  }

  function walkerAt() {
    for (var i = 0; i < grid.cells.length; i++) {
      if (grid.cells[i] === WALKER || grid.cells[i] === WALKER_HOME) return i;
    }
    return -1;
  }

  function solved() {
    return walkerAt() === puzzle.goal;
  }

  // True when a cell list fits this puzzle: same rocks and lantern, exactly one walker.
  function layoutOk(cells) {
    if (!Array.isArray(cells) || cells.length !== puzzle.w * puzzle.h) return false;
    var rocks = {};
    puzzle.rocks.forEach(function (r) {
      rocks[r] = true;
    });
    var walkers = 0;
    for (var i = 0; i < cells.length; i++) {
      var c = cells[i];
      if (!isCell(c)) return false;
      if ((c === ROCK) !== !!rocks[i]) return false;
      if (c === WALKER || c === WALKER_HOME) {
        walkers += 1;
        if ((c === WALKER_HOME) !== (i === puzzle.goal)) return false;
      } else if ((c === LANTERN) !== (i === puzzle.goal)) {
        return false;
      }
    }
    return walkers === 1;
  }

  function historyOk(state) {
    var h = state && state.history;
    if (!h) return true;
    var all = [].concat(h.undo || [], h.redo || []);
    return all.every(layoutOk);
  }

  function makeGrid() {
    grid = new K.Grid(puzzle.w, puzzle.h, startCells(puzzle), { historyLimit: HISTORY_LIMIT });
    grid.setCursor(puzzle.start % puzzle.w, Math.floor(puzzle.start / puzzle.w));
  }

  function bind() {
    if (view) view.destroy();
    view = K.bindGrid($("grid"), grid, {
      name: "Frozen pond, " + puzzle.w + " by " + puzzle.h,
      label: label,
      className: classes,
      onActivate: function (x, y) {
        var w = walkerAt();
        var wx = w % puzzle.w;
        var wy = Math.floor(w / puzzle.w);
        if (x === wx && y !== wy) slideTo(y < wy ? "up" : "down");
        else if (y === wy && x !== wx) slideTo(x < wx ? "left" : "right");
        else if (x === wx && y === wy) {
          setMessage("This is the walker. Press an arrow key, or a slide button, to set off.");
        } else {
          setMessage("Pick a square in the walker's row or column, or use the arrow keys.");
        }
      },
      onAction: function (action) {
        if (action === "undo") {
          undoMove();
          return true;
        }
        return false;
      },
    });
  }

  function label(cell, x, y) {
    var text = "Row " + (y + 1) + ", column " + (x + 1) + ", ";
    if (cell === ROCK) return text + "rock";
    if (cell === LANTERN) return text + "warm lantern";
    if (cell === WALKER) return text + "walker";
    if (cell === WALKER_HOME) return text + "walker at the warm lantern";
    return text + "ice";
  }

  function classes(cell, x, y) {
    var out = [];
    if (cell === ROCK) out.push("rock");
    else if (cell === LANTERN) out.push("lantern");
    else if (cell === WALKER) out.push("walker");
    else if (cell === WALKER_HOME) out.push("walker", "lantern");
    else out.push("ice");
    if (trail.indexOf(y * puzzle.w + x) >= 0) out.push("trail");
    return out.join(" ");
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function movesText(n) {
    return n + (n === 1 ? " move" : " moves");
  }

  function render() {
    view.render();
    var done = solved();
    $("moves").textContent = "Moves: " + moves;
    $("size").textContent =
      (mode === "daily" ? "Today's puzzle" : "Puzzle") + ", " + puzzle.w + " by " + puzzle.h;
    var best = $("best");
    if (done) {
      best.hidden = false;
      best.textContent = "Best possible: " + movesText(puzzle.best);
    } else {
      best.hidden = true;
    }
    var bt = $("besttoday");
    if (mode === "daily" && bestToday !== null) {
      bt.hidden = false;
      bt.textContent = "Your best today: " + bestToday;
    } else {
      bt.hidden = true;
    }
    $("undo").disabled = !grid.canUndo();
    var pad = document.querySelectorAll("#pad button");
    for (var i = 0; i < pad.length; i++) pad[i].disabled = done;
    var items = [
      { label: "Moves", value: moves },
      { label: "Pond", value: puzzle.w + " by " + puzzle.h },
    ];
    if (done) items.push({ label: "Best possible", value: puzzle.best });
    else if (mode === "daily" && bestToday !== null)
      items.push({ label: "Best today", value: bestToday });
    LullshiftBridge.summary(items);
  }

  function snapshot() {
    return puzzle
      ? {
          v: 1,
          seed: seed,
          mode: mode,
          puzzle: L.copyPuzzle(puzzle),
          grid: grid.snapshot(),
          moves: moves,
        }
      : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function focusWalker() {
    var w = walkerAt();
    grid.setCursor(w % puzzle.w, Math.floor(w / puzzle.w));
    view.render();
    view.focus();
  }

  // ---- Best of the day (activity.storage) -----------------------------------------------------

  function bestKey() {
    return "best-" + seed;
  }

  function loadBest() {
    bestToday = null;
    if (mode !== "daily") return;
    var s = seed;
    try {
      LullshiftBridge.storage.get(bestKey()).then(
        function (v) {
          var n = parseInt(v, 10);
          if (s === seed && mode === "daily" && Number.isFinite(n) && n > 0) {
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
    if (mode !== "daily") return;
    if (bestToday !== null && bestToday <= moves) return;
    bestToday = moves;
    try {
      LullshiftBridge.storage.set(bestKey(), moves).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  // ---- Moves ------------------------------------------------------------------------------------

  function slideTo(dir) {
    if (solved()) return;
    var from = walkerAt();
    var r = L.slide(puzzle, from, dir);
    if (!r.moved) {
      setMessage(
        "The walker can't slide " +
          DIR_TEXT[dir] +
          " from here: " +
          (r.stop === "rock" ? "a rock is in the way." : "that is the edge of the pond."),
      );
      focusWalker();
      return;
    }
    // Remember the squares passed through, for the faint trail.
    trail = [];
    var step = { up: -puzzle.w, down: puzzle.w, left: -1, right: 1 }[dir];
    for (var p = from; p !== r.pos;) {
      p += step;
      trail.push(p);
    }
    grid.edit(function (g) {
      g.set(from % puzzle.w, Math.floor(from / puzzle.w), ICE);
      g.set(
        r.pos % puzzle.w,
        Math.floor(r.pos / puzzle.w),
        r.pos === puzzle.goal ? WALKER_HOME : WALKER,
      );
    });
    moves += 1;
    var x = r.pos % puzzle.w;
    var y = Math.floor(r.pos / puzzle.w);
    var where = "Row " + (y + 1) + ", column " + (x + 1) + ". ";
    var why =
      r.stop === "rock" ? "Stopped by a rock. " : r.stop === "edge" ? "Stopped at the edge. " : "";
    if (r.stop === "lantern") {
      recordBest();
      var extra = moves - puzzle.best;
      setMessage(
        "Slid " +
          DIR_TEXT[dir] +
          " and stopped at the warm lantern in " +
          movesText(moves) +
          ". Best possible: " +
          movesText(puzzle.best) +
          ". " +
          (extra === 0 ? "That matches it." : "You were " + extra + " over, which is fine."),
      );
    } else {
      setMessage("Slid " + DIR_TEXT[dir] + ". " + why + where + movesText(moves) + ".");
    }
    render();
    focusWalker();
    save();
  }

  function undoMove() {
    if (!grid.canUndo()) return;
    grid.undo();
    if (!layoutOk(grid.cells)) {
      // Only possible with damaged saved data: start the puzzle over.
      restart();
      return;
    }
    moves = Math.max(0, moves - 1);
    trail = [];
    setMessage("Undone. " + movesText(moves) + ".");
    render();
    focusWalker();
    save();
  }

  function restart() {
    makeGrid();
    bind();
    moves = 0;
    trail = [];
    setMessage("Back at the start.");
    render();
    focusWalker();
    save();
  }

  function startPuzzle(s, nextMode, text) {
    var p = L.generate(s);
    if (!p) p = L.generate(s + 1);
    puzzle = p;
    seed = s >>> 0;
    mode = nextMode;
    makeGrid();
    bind();
    moves = 0;
    trail = [];
    setMessage(
      text +
        " Slide the walker to the lantern. At least " +
        L.MIN_MOVES[puzzle.w] +
        " moves are needed.",
    );
    loadBest();
    render();
    save();
  }

  function restoreFrom(r) {
    if (!r || r.v !== 1 || !Number.isInteger(r.seed)) return false;
    var p = L.validatePuzzle(r.puzzle);
    if (!p) return false;
    puzzle = p;
    makeGrid();
    var snap = r.grid;
    if (!grid.restore(snap, isCell)) return false;
    if (!layoutOk(grid.cells) || !historyOk(snap)) return false;
    seed = r.seed >>> 0;
    mode = r.mode === "random" ? "random" : "daily";
    moves = Number.isInteger(r.moves) && r.moves >= 0 && r.moves < 100000 ? r.moves : 0;
    return true;
  }

  function onKey(e) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    var action = K.keyAction(e, { wasd: true });
    if (action === "up" || action === "down" || action === "left" || action === "right") {
      e.preventDefault();
      e.stopPropagation();
      slideTo(action);
    } else if (
      action === "home" ||
      action === "end" ||
      action === "pageUp" ||
      action === "pageDown"
    ) {
      // The walker moves by sliding only; keep the cursor on it.
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    if (restoreFrom(init.restore)) {
      bind();
      render();
      loadBest();
      setMessage(
        solved()
          ? "At the warm lantern in " +
              movesText(moves) +
              ". Best possible: " +
              movesText(puzzle.best) +
              "."
          : "Welcome back. Slide the walker to the lantern.",
      );
    } else {
      startPuzzle(L.dateSeed(new Date()), "daily", "Today's puzzle.");
    }

    // Arrow keys and WASD slide the walker, so they must not move the grid's own cursor. This
    // capture listener runs before the grid's key handling.
    $("board").addEventListener("keydown", onKey, true);

    document.querySelectorAll("#pad button").forEach(function (b) {
      b.addEventListener("click", function () {
        slideTo(b.dataset.dir);
      });
    });
    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      startPuzzle(L.dateSeed(new Date()), "daily", "Today's puzzle.");
    });
    $("fresh").addEventListener("click", function () {
      startPuzzle(K.randomSeed(), "random", "A new puzzle.");
    });
  }

  LullshiftBridge.start({ onInit: setup, saveState: snapshot });
})();
