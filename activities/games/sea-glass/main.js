(function () {
  "use strict";
  var L = globalThis.GlassLogic;
  var K = window.LullshiftKits.grid;
  var SIZE = L.SIZE;
  var grid = new K.Grid(SIZE, SIZE, 1);
  var view = null;
  var game = null; // { seed, mode, board, rng, score, swapsLeft, picked, done }
  var best = 0;
  var reduced = false;
  var fresh = null; // indexes that just changed, for the settle animation

  function $(id) {
    return document.getElementById(id);
  }

  function setGrid() {
    for (var i = 0; i < L.CELLS; i++) grid.set(i % SIZE, Math.floor(i / SIZE), game.board[i]);
  }

  function startRound(seed, mode) {
    var g = L.newGame(seed);
    game = {
      seed: seed,
      mode: mode,
      board: g.board,
      rng: g.rng,
      score: 0,
      swapsLeft: g.swapsLeft,
      picked: -1,
      done: false,
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
          score: game.score,
          swapsLeft: game.swapsLeft,
          picked: game.picked,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validState(r) {
    return r && r.v === 1 && Number.isInteger(r.seed) && L.validGame(r);
  }

  function restoreRound(r) {
    game = {
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      board: r.board.slice(),
      rng: r.rng,
      score: r.score,
      swapsLeft: r.swapsLeft,
      picked: Number.isInteger(r.picked) && r.picked >= 0 && r.picked < L.CELLS ? r.picked : -1,
      done: r.swapsLeft === 0,
    };
    if (game.done) game.picked = -1;
    setGrid();
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
  }

  function label(piece, x, y) {
    var text = "Row " + (y + 1) + ", column " + (x + 1) + ", " + L.pieceName(piece);
    if (game.picked === y * SIZE + x) text += ", picked";
    return text;
  }

  function classes(piece, x, y) {
    var i = y * SIZE + x;
    var out = ["g", "g" + piece];
    if (game.picked === i) out.push("picked");
    if (fresh && fresh[i]) out.push("fresh");
    return out.join(" ");
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function render() {
    setGrid();
    view.render();
    $("swaps").textContent = "Swaps left: " + game.swapsLeft;
    $("score").textContent = "Score: " + game.score;
    $("best").textContent = "Best: " + best;
    LullshiftBridge.summary([
      { label: "Swaps left", value: game.swapsLeft },
      { label: "Score", value: game.score },
      { label: "Best", value: best },
    ]);
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function loadBest() {
    try {
      LullshiftBridge.storage.get("best").then(
        function (v) {
          var n = parseInt(v, 10);
          if (Number.isInteger(n) && n > best) {
            best = n;
            render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: best stays at this session's value */
    }
  }

  function recordBest() {
    if (game.score <= best) return false;
    best = game.score;
    try {
      LullshiftBridge.storage.set("best", best).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
    return true;
  }

  function settle(changedFrom) {
    if (reduced) return;
    fresh = {};
    for (var i = 0; i < L.CELLS; i++) if (game.board[i] !== changedFrom[i]) fresh[i] = true;
  }

  function clearFresh() {
    if (!fresh) return;
    fresh = null;
    window.setTimeout(function () {
      var nodes = document.querySelectorAll(".fresh");
      for (var i = 0; i < nodes.length; i++) nodes[i].classList.remove("fresh");
    }, 400);
  }

  function trySwap(a, b) {
    var r = L.swap(game, a, b);
    if (!r) {
      if (b < 0) setMessage("There is no glass that way.");
      else setMessage("That swap makes no line. Try another one.");
      return;
    }
    var before = game.board;
    game.board = r.board;
    game.rng = r.rng;
    game.score = r.score;
    game.swapsLeft = r.swapsLeft;
    game.picked = -1;
    settle(before);
    var text =
      "Cleared " + r.cleared + (r.cleared === 1 ? " piece" : " pieces") + " for " + r.points + ".";
    if (r.cascades > 1) text += " " + r.cascades + " lines in a chain.";
    if (r.reshuffled) text += " No swaps were left, so the glass was gently reshuffled.";
    if (L.roundOver(game)) {
      game.done = true;
      var isBest = recordBest();
      text += " Round complete. Score " + game.score + (isBest ? ", your best so far." : ".");
    }
    setMessage(text);
    render();
    clearFresh();
    save();
  }

  function activate(i) {
    if (game.done) {
      setMessage("The round is complete. Start a new board whenever you like.");
      return;
    }
    if (game.picked < 0) {
      game.picked = i;
      setMessage("Picked up " + L.pieceName(game.board[i]) + ". Press an arrow key to swap it.");
    } else if (game.picked === i) {
      game.picked = -1;
      setMessage("Put it back.");
    } else if (L.areNeighbours(game.picked, i)) {
      trySwap(game.picked, i);
      return;
    } else {
      game.picked = i;
      setMessage("Picked up " + L.pieceName(game.board[i]) + ". Press an arrow key to swap it.");
    }
    render();
    save();
  }

  function newRound(seed, mode, text) {
    startRound(seed, mode);
    fresh = null;
    setMessage(text);
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    reduced = !!init.reducedMotion;
    if (reduced) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreRound(init.restore);
    else startRound(K.dailySeed(new Date(), "sea-glass"), "daily");

    view = K.bindGrid($("grid"), grid, {
      name: "Sea glass board, " + SIZE + " by " + SIZE,
      label: label,
      className: classes,
      onActivate: function (x, y) {
        activate(y * SIZE + x);
      },
      onMove: function () {
        save();
      },
      onAction: function (action) {
        if (action === "cancel" && game.picked >= 0) {
          game.picked = -1;
          setMessage("Put it back.");
          save();
          return true;
        }
        return false;
      },
    });

    // While a piece is picked up, an arrow key swaps it with the neighbour that way instead of
    // moving the cursor. This capture listener runs before the grid kit's own handler.
    $("grid").addEventListener(
      "keydown",
      function (e) {
        if (game.picked < 0 || game.done) return;
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
        trySwap(game.picked, L.neighbour(game.picked, dir));
      },
      true,
    );

    $("restart").addEventListener("click", function () {
      newRound(game.seed, game.mode, "Board reset.");
    });
    $("daily").addEventListener("click", function () {
      newRound(K.dailySeed(new Date(), "sea-glass"), "daily", "Today's board.");
    });
    $("fresh").addEventListener("click", function () {
      newRound(K.randomSeed(), "random", "A new board.");
    });

    loadBest();
    if (restored) {
      setMessage(game.done ? "The round is complete. Score " + game.score + "." : "Welcome back.");
      render();
    } else {
      setMessage("Today's board. Press Enter on a piece, then an arrow key to swap it.");
      render();
      save();
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
  });
})();
