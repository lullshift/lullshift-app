// Brook Links: draw a stream between each pair of matching stones, on the grid kit.
(function () {
  "use strict";
  var L = globalThis.BrookLogic;
  var K = window.LullshiftKits.grid;

  var size = 5;
  var grid = null;
  var view = null;
  var puzzle = null;
  // { seed, mode, size, paths, history, moves, drawing, stroke, won }
  var game = null;
  var arms = [];
  var bestToday = null;
  var HEAD_NONE = -1;

  function $(id) {
    return document.getElementById(id);
  }

  function idx(x, y) {
    return y * size + x;
  }

  function buildBoard(newSize) {
    size = newSize;
    if (view) view.destroy();
    grid = new K.Grid(size, size, -1);
    $("grid").setAttribute("data-size", String(size));
    view = K.bindGrid($("grid"), grid, {
      name: "Brook Links garden, " + size + " by " + size,
      label: label,
      text: text,
      className: classes,
      onActivate: activate,
      onMove: function () {
        save();
      },
      onAction: function (action) {
        if (action === "undo") {
          undoMove();
          return true;
        }
        if (action === "cancel") {
          if (game.drawing !== HEAD_NONE) {
            endStroke();
            setMessage("Stopped drawing.");
            render();
            save();
            return true;
          }
        }
        return false;
      },
    });
  }

  function computeArms() {
    arms = [];
    for (var i = 0; i < size * size; i++) arms.push(0);
    game.paths.forEach(function (p) {
      for (var k = 0; k < p.length; k++) {
        if (k > 0) arms[p[k]] |= armTo(p[k], p[k - 1]);
        if (k < p.length - 1) arms[p[k]] |= armTo(p[k], p[k + 1]);
      }
    });
  }

  // Which side of cell a touches cell b: 1 north, 2 east, 4 south, 8 west.
  function armTo(a, b) {
    if (b === a - size) return 1;
    if (b === a + 1) return 2;
    if (b === a + size) return 4;
    return 8;
  }

  function headCell() {
    if (game.drawing === HEAD_NONE) return -1;
    var p = game.paths[game.drawing];
    return p.length ? p[p.length - 1] : -1;
  }

  function label(v, x, y) {
    var i = idx(x, y);
    var base = "Row " + (y + 1) + ", column " + (x + 1) + ", ";
    if (v === -1) return base + "empty";
    var name = L.NAMES[v];
    var linked = L.isLinked(puzzle, game.paths, v);
    var status = linked ? "linked" : "not linked";
    if (L.stoneColor(puzzle, i) === v) {
      var t = base + name + " stone " + L.LETTERS[v] + ", " + status;
      if (i === headCell()) t += ", drawing from here";
      return t;
    }
    var t2 = base + name + " stream";
    if (i === headCell()) t2 += ", end of the stream you are drawing";
    return t2;
  }

  function text(v, x, y) {
    return v !== -1 && L.stoneColor(puzzle, idx(x, y)) === v ? L.LETTERS[v] : "";
  }

  function classes(v, x, y) {
    var i = idx(x, y);
    var out = [];
    if (v !== -1) out.push("c" + v);
    if (L.stoneColor(puzzle, i) !== -1) out.push("stone");
    var a = arms[i] || 0;
    if (a & 1) out.push("n");
    if (a & 2) out.push("e");
    if (a & 4) out.push("s");
    if (a & 8) out.push("w");
    if (i === headCell()) out.push("head");
    if (v !== -1 && L.isLinked(puzzle, game.paths, v)) out.push("linked");
    return out.join(" ");
  }

  function syncCells() {
    var o = L.owners(puzzle, game.paths);
    for (var i = 0; i < o.length; i++) grid.set(i % size, Math.floor(i / size), o[i]);
    computeArms();
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function movesText(n) {
    return n + (n === 1 ? " move" : " moves");
  }

  function startPuzzle(seed, mode, sz) {
    puzzle = L.generate(seed, sz);
    game = {
      seed: seed,
      mode: mode,
      size: sz,
      paths: L.blankPaths(puzzle),
      history: [],
      moves: 0,
      drawing: HEAD_NONE,
      stroke: false,
      won: false,
    };
    size = sz;
    arms = [];
    if (!grid || grid.width !== sz) buildBoard(sz);
    grid.setCursor(puzzle.stones[0].a % size, Math.floor(puzzle.stones[0].a / size));
    syncCells();
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          size: game.size,
          paths: game.paths,
          history: game.history,
          moves: game.moves,
          drawing: game.drawing,
          stroke: game.stroke,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validState(r) {
    if (!r || r.v !== 1 || !Number.isInteger(r.seed) || !L.isSize(r.size)) return false;
    var p = L.generate(r.seed, r.size);
    if (!L.validPaths(p, r.paths)) return false;
    if (!Array.isArray(r.history)) return false;
    return r.history.every(function (h) {
      return L.validPaths(p, h);
    });
  }

  function restoreGame(r) {
    startPuzzle(r.seed, r.mode === "daily" ? "daily" : "random", r.size);
    game.paths = L.clonePaths(r.paths);
    game.history = r.history.slice(-L.HISTORY_LIMIT);
    game.moves = Number.isInteger(r.moves) && r.moves >= 0 ? r.moves : 0;
    var d = r.drawing;
    if (Number.isInteger(d) && d >= 0 && d < puzzle.stones.length && game.paths[d].length > 0) {
      game.drawing = d;
      game.stroke = r.stroke === true && game.history.length > 0;
    }
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
    game.won = L.isSolved(puzzle, game.paths);
    syncCells();
  }

  function render() {
    syncCells();
    view.render();
    var linked = L.linkedCount(puzzle, game.paths);
    var k = puzzle.stones.length;
    var cov = L.coveredCount(puzzle, game.paths);
    var total = size * size;
    var full = L.isFullGarden(puzzle, game.paths);
    $("moves").textContent = "Moves: " + game.moves;
    $("linked").textContent = "Linked " + linked + " of " + k;
    $("covered").textContent = full ? "Full garden" : "Covered " + cov + " of " + total;
    $("undo").disabled = game.history.length === 0;
    $("size").textContent = "Size: " + size + " x " + size;
    var nextSize = L.SIZES[(L.SIZES.indexOf(size) + 1) % L.SIZES.length];
    $("size").setAttribute(
      "aria-label",
      "Size: " + size + " by " + size + ". Press to switch to " + nextSize + " by " + nextSize,
    );
    var best = $("best");
    if (game.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Best today: " + bestToday;
    } else {
      best.hidden = true;
    }
    var items = [
      { label: "Linked", value: linked + " of " + k },
      { label: "Covered", value: cov + " of " + total },
      { label: "Moves", value: game.moves },
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

  // A stroke is everything between starting at a stone and stopping; undo takes back a stroke.
  function beginStroke() {
    if (!game.stroke) {
      game.history = L.pushSnapshot(game.history, game.paths);
      game.stroke = true;
    }
  }

  function endStroke() {
    if (game.stroke) {
      var top = game.history[game.history.length - 1];
      if (top && L.samePaths(top, game.paths)) {
        game.history = game.history.slice(0, -1);
      } else {
        game.moves += 1;
      }
      game.stroke = false;
    }
    game.drawing = HEAD_NONE;
  }

  function moveCursorTo(cell) {
    grid.setCursor(cell % size, Math.floor(cell / size));
  }

  function checkWin() {
    var solved = L.isSolved(puzzle, game.paths);
    if (solved && !game.won) {
      game.won = true;
      recordBest();
      var full = L.isFullGarden(puzzle, game.paths);
      setMessage(
        "Every pair is linked in " +
          movesText(game.moves) +
          ". Well done." +
          (full ? " Full garden bonus: every cell has a stream." : ""),
      );
    } else if (!solved) {
      game.won = false;
    }
  }

  function startAt(cell) {
    var r = L.startPath(puzzle, game.paths, cell);
    if (!r) return;
    endStroke();
    game.drawing = r.color;
    beginStroke();
    game.paths = r.paths;
    if (headCell() !== -1) moveCursorTo(headCell());
    setMessage(
      "Drawing the " +
        L.NAMES[r.color] +
        " stream. Arrow keys extend it, Backspace retracts, Enter stops.",
    );
  }

  function tryStep(dir) {
    var r = L.step(puzzle, game.paths, game.drawing, dir);
    if (!r.ok) {
      var why =
        r.reason === "edge"
          ? "That is the edge of the garden."
          : r.reason === "another stream"
            ? "Another stream is in the way."
            : r.reason === "own stream"
              ? "This stream is already there."
              : r.reason === "already linked"
                ? "This stream is linked. Backspace retracts it."
                : "Nothing to extend.";
      setMessage(why);
      return false;
    }
    game.paths = r.paths;
    var head = headCell();
    if (head !== -1) moveCursorTo(head);
    if (r.event === "linked") {
      var name = L.NAMES[game.drawing];
      endStroke();
      setMessage("The " + name + " stream is linked.");
      checkWin();
    } else if (r.event === "retracted") {
      setMessage("Retracted.");
    } else {
      setMessage("");
    }
    return true;
  }

  function activate(x, y) {
    var cell = idx(x, y);
    var o = grid.cells[cell];
    if (game.drawing !== HEAD_NONE) {
      var c = game.drawing;
      var head = headCell();
      if (cell === head) {
        endStroke();
        setMessage("Stopped drawing.");
      } else if (L.adjacent(head, cell, size)) {
        var dir =
          cell === head - size
            ? "up"
            : cell === head + 1
              ? "right"
              : cell === head + size
                ? "down"
                : "left";
        tryStep(dir);
      } else if (o === c && game.paths[c].indexOf(cell) !== -1) {
        game.paths = L.truncateTo(game.paths, c, cell).paths;
        setMessage("Cut back to here.");
      } else {
        endStroke();
        if (L.stoneColor(puzzle, cell) !== -1) startAt(cell);
        else setMessage("Stopped drawing.");
      }
    } else if (L.stoneColor(puzzle, cell) !== -1) {
      startAt(cell);
    } else if (o !== -1) {
      // A cell on a finished or half-drawn stream: carry on drawing from here.
      game.drawing = o;
      beginStroke();
      game.paths = L.truncateTo(game.paths, o, cell).paths;
      setMessage("Drawing the " + L.NAMES[o] + " stream from here.");
    } else {
      setMessage("Press Enter on a stone to start a stream.");
    }
    render();
    save();
  }

  function onKeyCapture(e) {
    if (!game || !grid || !$("grid").contains(e.target)) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (game.drawing === HEAD_NONE) return;
    var dir =
      e.key === "ArrowUp"
        ? "up"
        : e.key === "ArrowDown"
          ? "down"
          : e.key === "ArrowLeft"
            ? "left"
            : e.key === "ArrowRight"
              ? "right"
              : null;
    if (dir) {
      e.preventDefault();
      e.stopPropagation();
      tryStep(dir);
      render();
      save();
    } else if (e.key === "Backspace") {
      e.preventDefault();
      e.stopPropagation();
      var r = L.retract(game.paths, game.drawing);
      if (r.ok) {
        game.paths = r.paths;
        var head = headCell();
        if (head === -1) {
          endStroke();
          setMessage("Stream cleared.");
        } else {
          moveCursorTo(head);
          setMessage("Retracted.");
        }
        game.won = L.isSolved(puzzle, game.paths);
        render();
        save();
      }
    }
  }

  function undoMove() {
    if (game.history.length === 0) return;
    var r = L.undo(game.paths, game.history);
    game.paths = r.paths;
    game.history = r.history;
    game.moves = Math.max(0, game.moves - (game.stroke ? 0 : 1));
    game.stroke = false;
    game.drawing = HEAD_NONE;
    game.won = L.isSolved(puzzle, game.paths);
    setMessage("Undone. " + movesText(game.moves) + ".");
    render();
    save();
  }

  function restart() {
    game.paths = L.blankPaths(puzzle);
    game.history = [];
    game.moves = 0;
    game.drawing = HEAD_NONE;
    game.stroke = false;
    game.won = false;
    setMessage("Garden cleared.");
    render();
    save();
  }

  function newPuzzle(seed, mode, sz, msg) {
    startPuzzle(seed, mode, sz);
    setMessage(msg);
    loadBest();
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreGame(init.restore);
    else startPuzzle(L.dateSeed(new Date(), 5), "daily", 5);

    document.addEventListener("keydown", onKeyCapture, true);

    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      var s = size;
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " garden.");
    });
    $("fresh").addEventListener("click", function () {
      var s = size;
      newPuzzle(K.randomSeed(), "random", s, "A new " + s + " by " + s + " garden.");
    });
    $("size").addEventListener("click", function () {
      var s = L.SIZES[(L.SIZES.indexOf(size) + 1) % L.SIZES.length];
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " garden.");
    });

    if (restored) {
      if (game.won) setMessage("Every pair is linked in " + movesText(game.moves) + ".");
      loadBest();
      render();
    } else {
      setMessage("Today's 5 by 5 garden. Press Enter on a stone to start a stream.");
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
