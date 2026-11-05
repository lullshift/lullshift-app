(function () {
  "use strict";
  var L = globalThis.MossLogic;
  var K = window.LullshiftKits.grid;
  var SIZE = L.SIZE;
  var TOTAL = SIZE * SIZE;
  var grid = new K.Grid(SIZE, SIZE, 0);
  var view = null;
  var game = null; // { seed, mode, src, start, history, moves, solved }
  var wet = [];
  var lastShift = false;
  var bestToday = null;

  function $(id) {
    return document.getElementById(id);
  }

  function cellsNow() {
    return grid.cells.slice();
  }

  function setCells(cells) {
    for (var i = 0; i < TOTAL; i++) grid.set(i % SIZE, Math.floor(i / SIZE), cells[i]);
  }

  function startGarden(seed, mode) {
    var g = L.generate(seed, SIZE);
    game = {
      seed: seed,
      mode: mode,
      src: g.src,
      start: g.cells.slice(),
      history: [],
      moves: 0,
      solved: false,
    };
    setCells(g.cells);
    grid.setCursor(g.src % SIZE, Math.floor(g.src / SIZE));
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          src: game.src,
          start: game.start,
          cells: cellsNow(),
          history: game.history,
          moves: game.moves,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function maskList(v) {
    return Array.isArray(v) && v.length === TOTAL && v.every(L.isMask);
  }

  function validState(r) {
    return (
      r &&
      r.v === 1 &&
      Number.isInteger(r.seed) &&
      Number.isInteger(r.src) &&
      r.src >= 0 &&
      r.src < TOTAL &&
      maskList(r.start) &&
      maskList(r.cells) &&
      Array.isArray(r.history)
    );
  }

  function restoreGarden(r) {
    game = {
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      src: r.src,
      start: r.start.slice(),
      history: r.history
        .filter(function (h) {
          return h && Number.isInteger(h.i) && h.i >= 0 && h.i < TOTAL && typeof h.cw === "boolean";
        })
        .slice(-L.HISTORY_LIMIT),
      moves: Number.isInteger(r.moves) && r.moves >= 0 ? r.moves : 0,
      solved: false,
    };
    setCells(r.cells);
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
    game.solved = L.isSolved(cellsNow(), SIZE, game.src);
  }

  function label(mask, x, y) {
    var i = y * SIZE + x;
    var open = L.openings(mask);
    var text = "Row " + (y + 1) + ", column " + (x + 1) + ", " + L.pieceType(mask);
    if (i === game.src) text += " at the spring";
    text += ", open " + open.join(" and ");
    text += wet[i] ? ", watered" : ", dry";
    return text;
  }

  function classes(mask, x, y) {
    var i = y * SIZE + x;
    var out = [];
    if (mask & L.N) out.push("n");
    if (mask & L.E) out.push("e");
    if (mask & L.S) out.push("s");
    if (mask & L.W) out.push("w");
    if (wet[i]) out.push("wet");
    if (i === game.src) out.push("spring");
    return out.join(" ");
  }

  function wateredNow() {
    var n = 0;
    for (var i = 0; i < wet.length; i++) if (wet[i]) n += 1;
    return n;
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function movesText(n) {
    return n + (n === 1 ? " move" : " moves");
  }

  function render() {
    wet = L.watered(cellsNow(), SIZE, game.src);
    var count = wateredNow();
    view.render();
    $("moves").textContent = "Moves: " + game.moves;
    $("wet").textContent = "Watered " + count + " of " + TOTAL;
    $("undo").disabled = game.history.length === 0 || game.solved;
    var best = $("best");
    if (game.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Best today: " + bestToday;
    } else {
      best.hidden = true;
    }
    var items = [
      { label: "Moves", value: game.moves },
      { label: "Watered", value: count + " of " + TOTAL },
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

  function turnTile(x, y, cw) {
    if (game.solved) return;
    var i = y * SIZE + x;
    var mask = grid.cells[i];
    if (!L.canRotate(mask)) {
      setMessage("This crossing already fits every way.");
      return;
    }
    grid.set(x, y, L.rotate(mask, cw));
    game.history = L.pushMove(game.history, i, cw);
    game.moves += 1;
    render();
    if (L.isSolved(cellsNow(), SIZE, game.src)) {
      game.solved = true;
      recordBest();
      setMessage("Every bed is watered in " + movesText(game.moves) + ". Well done.");
      render();
    } else {
      setMessage("Watered " + wateredNow() + " of " + TOTAL + ".");
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
    setCells(game.start);
    game.history = [];
    game.moves = 0;
    game.solved = false;
    setMessage("Garden reset.");
    render();
    save();
  }

  function newGarden(seed, mode, text) {
    startGarden(seed, mode);
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function randomSeed() {
    return K.randomSeed();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreGarden(init.restore);
    else startGarden(L.dateSeed(new Date()), "daily");
    wet = L.watered(cellsNow(), SIZE, game.src);

    view = K.bindGrid($("grid"), grid, {
      name: "Moss Paths garden, " + SIZE + " by " + SIZE,
      label: label,
      className: classes,
      onActivate: function (x, y) {
        turnTile(x, y, !lastShift);
      },
      onMove: function () {
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

    // Shift+Enter, Shift+Space and Shift+click turn counter-clockwise; this runs before the
    // button's own click handler.
    $("grid").addEventListener(
      "click",
      function (e) {
        lastShift = !!e.shiftKey;
      },
      true,
    );
    $("grid").addEventListener("keydown", function (e) {
      if ((e.key === "r" || e.key === "R") && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        turnTile(grid.cursor.x, grid.cursor.y, false);
      }
    });

    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      newGarden(L.dateSeed(new Date()), "daily", "Today's garden.");
    });
    $("fresh").addEventListener("click", function () {
      newGarden(randomSeed(), "random", "A new garden.");
    });

    if (restored) {
      if (game.solved) {
        setMessage("Every bed is watered in " + movesText(game.moves) + ".");
      }
      loadBest();
      render();
    } else {
      setMessage("Today's garden.");
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
