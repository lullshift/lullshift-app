(function () {
  "use strict";
  var L = globalThis.MossLogic;
  var K = window.LullshiftKits.grid;
  var SIZE = L.SIZE;
  var TOTAL = SIZE * SIZE;
  var grid = null;
  var view = null;
  var game = null; // { seed, mode, size, src, start, history, moves, solved }
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

  // (Re)builds the grid model and its view for a garden size.
  function buildBoard(size) {
    SIZE = size;
    TOTAL = size * size;
    if (view) view.destroy();
    grid = new K.Grid(size, size, 0);
    $("grid").setAttribute("data-size", String(size));
    $("grid").style.setProperty("--n", String(size));
    view = K.bindGrid($("grid"), grid, {
      name: "Moss Paths garden, " + size + " by " + size,
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
  }

  function startGarden(seed, mode, size) {
    if (size !== SIZE || !grid) buildBoard(size);
    var g = L.generate(seed, size);
    game = {
      seed: seed,
      mode: mode,
      size: size,
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
          size: SIZE,
          src: game.src,
          start: game.start,
          cells: cellsNow(),
          history: game.history,
          moves: game.moves,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  // Saves from before sizes existed have no size: they are 6 by 6.
  function savedSize(r) {
    return r && r.size === undefined ? 6 : r && r.size;
  }

  function maskList(v, total) {
    return Array.isArray(v) && v.length === total && v.every(L.isMask);
  }

  function validState(r) {
    var size = savedSize(r);
    return !!(
      r &&
      r.v === 1 &&
      L.isSize(size) &&
      Number.isInteger(r.seed) &&
      Number.isInteger(r.src) &&
      r.src >= 0 &&
      r.src < size * size &&
      maskList(r.start, size * size) &&
      maskList(r.cells, size * size) &&
      Array.isArray(r.history)
    );
  }

  function restoreGarden(r) {
    buildBoard(savedSize(r));
    game = {
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      size: SIZE,
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
    if (game && i === game.src) text += " at the spring";
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
    if (game && i === game.src) out.push("spring");
    return out.join(" ");
  }

  function wateredNow() {
    var n = 0;
    for (var i = 0; i < wet.length; i++) if (wet[i]) n += 1;
    return n;
  }

  function nextSize() {
    return L.SIZES[(L.SIZES.indexOf(SIZE) + 1) % L.SIZES.length];
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
    var next = nextSize();
    $("size").textContent = "Size: " + SIZE + " x " + SIZE;
    $("size").setAttribute(
      "aria-label",
      "Size: " + SIZE + " by " + SIZE + ". Press to switch to " + next + " by " + next,
    );
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

  // 6 by 6 keeps the key it always had; other sizes include the size.
  function bestKey() {
    return SIZE === 6 ? "best-" + game.seed : "best-" + SIZE + "-" + game.seed;
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
    var crossing = !L.canRotate(mask);
    grid.set(x, y, L.rotate(mask, cw));
    game.history = L.pushMove(game.history, i, cw);
    if (!crossing) game.moves += 1; // turning a crossing is not a move
    render();
    if (L.isSolved(cellsNow(), SIZE, game.src)) {
      game.solved = true;
      recordBest();
      setMessage("Every bed is watered in " + movesText(game.moves) + ". Well done.");
      render();
    } else if (crossing) {
      setMessage(
        "Crossing turned. It looks the same from every side. Watered " +
          wateredNow() +
          " of " +
          TOTAL +
          ".",
      );
    } else {
      setMessage("Watered " + wateredNow() + " of " + TOTAL + ".");
    }
    save();
  }

  function undoMove() {
    if (game.history.length === 0 || game.solved) return;
    var last = game.history[game.history.length - 1];
    var counted = L.countsAsMove(cellsNow()[last.i]);
    var r = L.undo(cellsNow(), game.history);
    setCells(r.cells);
    game.history = r.history;
    if (counted) game.moves = Math.max(0, game.moves - 1);
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

  function newGarden(seed, mode, text, size) {
    startGarden(seed, mode, size || SIZE);
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
    else startGarden(L.dateSeed(new Date(), 6), "daily", 6);
    wet = L.watered(cellsNow(), SIZE, game.src);

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
      newGarden(L.dateSeed(new Date(), SIZE), "daily", "Today's garden.");
    });
    $("fresh").addEventListener("click", function () {
      newGarden(randomSeed(), "random", "A new garden.");
    });
    $("size").addEventListener("click", function () {
      var n = nextSize();
      if (game.mode === "daily") {
        newGarden(L.dateSeed(new Date(), n), "daily", "Today's " + n + " by " + n + " garden.", n);
      } else {
        newGarden(randomSeed(), "random", "A new " + n + " by " + n + " garden.", n);
      }
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
