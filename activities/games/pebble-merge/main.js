(function () {
  "use strict";
  var L = globalThis.PebbleLogic;
  var K = window.LullshiftKits.grid;
  var SIZE = L.SIZE;
  var grid = new K.Grid(SIZE, SIZE, 0);
  var view = null;
  var game = null; // { seed, mode, board, rng, moves, merges, prev, bestTier }
  var bestTier = 0; // best pebble made on this tray today (daily only)
  var swipe = null;

  function $(id) {
    return document.getElementById(id);
  }

  function setGrid() {
    for (var i = 0; i < L.CELLS; i++) grid.set(i % SIZE, Math.floor(i / SIZE), game.board[i]);
  }

  function startTray(seed, mode) {
    var g = L.newGame(seed);
    game = {
      seed: seed,
      mode: mode,
      board: g.board,
      rng: g.rng,
      moves: 0,
      merges: 0,
      prev: null,
    };
    setGrid();
    grid.setCursor(0, 0);
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          board: game.board,
          rng: game.rng,
          moves: game.moves,
          merges: game.merges,
          prev: game.prev,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validState(r) {
    return (
      r &&
      r.v === 1 &&
      Number.isInteger(r.seed) &&
      L.validGame(r) &&
      (r.prev === null || r.prev === undefined || L.validGame(r.prev))
    );
  }

  function restoreTray(r) {
    game = {
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      board: r.board.slice(),
      rng: r.rng,
      moves: r.moves,
      merges: r.merges,
      prev: r.prev
        ? {
            board: r.prev.board.slice(),
            rng: r.prev.rng,
            moves: r.prev.moves,
            merges: r.prev.merges,
          }
        : null,
    };
    setGrid();
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
  }

  function label(tier, x, y) {
    var where = "Row " + (y + 1) + ", column " + (x + 1) + ", ";
    return where + (tier === 0 ? "empty" : L.tierName(tier) + ", size " + tier);
  }

  function classes(tier) {
    return tier === 0 ? "empty" : "p p" + tier;
  }

  function cellText(tier) {
    return tier === 0 ? "" : L.tierName(tier) + " " + tier;
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function full() {
    return !L.canMove(game.board);
  }

  function render() {
    setGrid();
    view.render();
    var top = L.largest(game.board);
    $("moves").textContent = "Slides: " + game.moves;
    $("largest").textContent = "Largest: " + (top ? L.tierName(top) : "none yet");
    var best = $("best");
    if (game.mode === "daily" && bestTier > 0) {
      best.hidden = false;
      best.textContent = "Best today: " + L.tierName(bestTier);
    } else {
      best.hidden = true;
    }
    $("undo").disabled = !game.prev;
    var items = [
      { label: "Slides", value: game.moves },
      { label: "Largest", value: top ? L.tierName(top) : "none yet" },
    ];
    if (game.mode === "daily" && bestTier > 0) {
      items.push({ label: "Best today", value: L.tierName(bestTier) });
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
    bestTier = 0;
    if (game.mode !== "daily") return;
    var seed = game.seed;
    try {
      LullshiftBridge.storage.get(bestKey()).then(
        function (v) {
          var n = parseInt(v, 10);
          if (game && game.seed === seed && Number.isInteger(n) && n > 0 && n <= L.MAX_TIER) {
            bestTier = Math.max(bestTier, n);
            render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: show nothing */
    }
  }

  function recordBest(tier) {
    if (game.mode !== "daily" || tier <= bestTier) return;
    bestTier = tier;
    try {
      LullshiftBridge.storage.set(bestKey(), tier).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function slideTray(dir) {
    var before = { board: game.board, rng: game.rng, moves: game.moves, merges: game.merges };
    var r = L.move(game, dir);
    if (!r) {
      if (full()) setMessage("The tray is full. Start a new tray whenever you like.");
      else setMessage("Nothing slides that way.");
      return;
    }
    game.prev = before;
    game.board = r.board;
    game.rng = r.rng;
    game.moves = r.moves;
    game.merges = r.merges;
    var top = L.largest(game.board);
    recordBest(top);
    if (full()) {
      setMessage(
        "The tray is full. Your largest is " + L.tierName(top) + ". Start a new tray, or undo.",
      );
    } else if (r.largestMade > 0) {
      setMessage("Two pebbles became a " + L.tierName(r.largestMade) + ".");
    } else {
      setMessage("Slid " + L.DIR_NAMES[dir] + ".");
    }
    render();
    save();
  }

  function undoSlide() {
    if (!game.prev) return;
    game.board = game.prev.board.slice();
    game.rng = game.prev.rng;
    game.moves = game.prev.moves;
    game.merges = game.prev.merges;
    game.prev = null;
    setMessage("Undone.");
    render();
    save();
  }

  function newTray(seed, mode, text) {
    startTray(seed, mode);
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreTray(init.restore);
    else startTray(K.dailySeed(new Date(), "pebble-merge"), "daily");

    view = K.bindGrid($("grid"), grid, {
      name: "Pebble tray, " + SIZE + " by " + SIZE,
      label: label,
      text: cellText,
      className: classes,
      onAction: function (action) {
        if (action === "undo") {
          undoSlide();
          return true;
        }
        return false;
      },
    });

    // Arrow keys slide the whole tray, so they must not also move the grid cursor. This capture
    // listener runs before the grid kit's own handler and stops the event there.
    $("grid").addEventListener(
      "keydown",
      function (e) {
        if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
        var dir =
          e.key === "ArrowUp"
            ? 0
            : e.key === "ArrowRight"
              ? 1
              : e.key === "ArrowDown"
                ? 2
                : e.key === "ArrowLeft"
                  ? 3
                  : -1;
        if (dir < 0) return;
        e.preventDefault();
        e.stopPropagation();
        slideTray(dir);
      },
      true,
    );

    // Pointer users can swipe across the tray.
    $("grid").addEventListener("pointerdown", function (e) {
      swipe = { x: e.clientX, y: e.clientY };
    });
    $("grid").addEventListener("pointerup", function (e) {
      if (!swipe) return;
      var dx = e.clientX - swipe.x;
      var dy = e.clientY - swipe.y;
      swipe = null;
      if (Math.max(Math.abs(dx), Math.abs(dy)) < 40) return;
      if (Math.abs(dx) > Math.abs(dy)) slideTray(dx > 0 ? 1 : 3);
      else slideTray(dy > 0 ? 2 : 0);
    });

    $("undo").addEventListener("click", undoSlide);
    $("daily").addEventListener("click", function () {
      newTray(K.dailySeed(new Date(), "pebble-merge"), "daily", "Today's tray.");
    });
    $("fresh").addEventListener("click", function () {
      newTray(K.randomSeed(), "random", "A new tray.");
    });

    if (restored) {
      setMessage(
        full() ? "The tray is full. Start a new tray whenever you like." : "Welcome back.",
      );
      loadBest();
      render();
    } else {
      setMessage("Today's tray. Use the arrow keys to slide the pebbles.");
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
