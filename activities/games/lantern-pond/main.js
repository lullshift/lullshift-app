// Lantern Pond: a lights-out puzzle on the grid kit (ADR-019). The kit supplies the grid model,
// the keyboard navigation and the ARIA grid; the rules stay in logic.js.
(function () {
  "use strict";
  var L = globalThis.LanternLogic;
  var K = window.LullshiftKits.grid;
  var SIZE = L.SIZE;
  var state = null;
  var grid = new K.Grid(SIZE, SIZE, false);
  var view = null;
  var bestToday = null;

  function $(id) {
    return document.getElementById(id);
  }

  function newState(seed, mode) {
    return {
      board: L.generate(seed).board,
      history: [],
      moves: 0,
      seed: seed,
      mode: mode,
      solved: false,
    };
  }

  function validState(r) {
    return (
      r &&
      Array.isArray(r.board) &&
      r.board.length === L.CELLS &&
      Array.isArray(r.history) &&
      typeof r.seed === "number" &&
      Number.isFinite(r.seed)
    );
  }

  function restoreState(r) {
    var board = r.board.map(function (v) {
      return v === true;
    });
    var history = r.history.filter(function (n) {
      return Number.isInteger(n) && n >= 0 && n < L.CELLS;
    });
    return {
      board: board,
      history: history,
      moves: typeof r.moves === "number" ? r.moves : history.length,
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      solved: L.isSolved(board),
    };
  }

  function label(lit, x, y) {
    return "Lantern row " + (y + 1) + ", column " + (x + 1) + ", " + (lit ? "lit" : "unlit");
  }

  function buildGrid() {
    view = K.bindGrid($("grid"), grid, {
      name: "Lantern pond, " + SIZE + " by " + SIZE,
      label: label,
      text: function (lit) {
        return lit ? "●" : "○";
      },
      className: function () {
        return "lantern";
      },
      pressed: function (lit) {
        return lit;
      },
      onActivate: function (x, y) {
        pressCell(y * SIZE + x);
      },
      onMove: save,
      onAction: function (action) {
        if (action === "undo") {
          undoMove();
          return true;
        }
        return false;
      },
    });
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function render() {
    for (var i = 0; i < L.CELLS; i++) grid.set(i % SIZE, Math.floor(i / SIZE), state.board[i]);
    view.render();
    $("moves").textContent = "Moves: " + state.moves;
    $("undo").disabled = state.history.length === 0 || state.solved;
    var best = $("best");
    if (state.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Your best today: " + bestToday + (bestToday === 1 ? " move" : " moves");
    } else {
      best.hidden = true;
    }
    report();
  }

  // What the host shows while the game is paused (S05-T09).
  function report() {
    var lit = 0;
    for (var i = 0; i < L.CELLS; i++) if (state.board[i]) lit += 1;
    var items = [
      { label: "Lanterns lit", value: lit + " of " + L.CELLS },
      { label: "Moves", value: state.moves },
    ];
    if (state.mode === "daily" && bestToday !== null) {
      items.push({ label: "Best today", value: bestToday });
    }
    LullshiftBridge.summary(items);
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }
  function snapshot() {
    return state
      ? {
          board: state.board,
          history: state.history,
          moves: state.moves,
          seed: state.seed,
          mode: state.mode,
          solved: state.solved,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function bestKey() {
    return "best-" + state.seed;
  }
  function loadBest() {
    bestToday = null;
    if (state.mode !== "daily") return;
    var key = bestKey();
    var seed = state.seed;
    try {
      LullshiftBridge.storage.get(key).then(
        function (v) {
          var n = parseInt(v, 10);
          if (state && state.seed === seed && Number.isFinite(n) && n > 0) {
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
    if (state.mode !== "daily") return;
    if (bestToday !== null && bestToday <= state.moves) return;
    bestToday = state.moves;
    try {
      LullshiftBridge.storage.set(bestKey(), state.moves).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function pressCell(i) {
    if (state.solved) return;
    state.board = L.press(state.board, i);
    state.history = L.pushMove(state.history, i);
    state.moves += 1;
    grid.setCursor(i % SIZE, Math.floor(i / SIZE));
    if (L.isSolved(state.board)) {
      state.solved = true;
      recordBest();
      setMessage(
        "Pond lit in " + state.moves + (state.moves === 1 ? " move." : " moves.") + " Well done.",
      );
    } else {
      setMessage("Move " + state.moves + ".");
    }
    render();
    save();
  }

  function undoMove() {
    if (state.history.length === 0 || state.solved) return;
    var r = L.undo(state.board, state.history);
    state.board = r.board;
    state.history = r.stack;
    state.moves = Math.max(0, state.moves - 1);
    setMessage("Undone. " + state.moves + (state.moves === 1 ? " move." : " moves."));
    render();
    save();
  }

  function restart() {
    state.board = L.generate(state.seed).board;
    state.history = [];
    state.moves = 0;
    state.solved = false;
    setMessage("Pond reset.");
    render();
    save();
  }

  function startPond(seed, mode) {
    state = newState(seed, mode);
    grid.setCursor(0, 0);
    setMessage(mode === "daily" ? "Today's pond." : "A new pond.");
    loadBest();
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");
    buildGrid();
    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      startPond(L.dateSeed(new Date()), "daily");
    });
    $("fresh").addEventListener("click", function () {
      startPond(K.randomSeed(), "random");
    });

    if (validState(init.restore)) {
      state = restoreState(init.restore);
      var cur = init.restore.cursor;
      if (cur && K.isIntIn(cur.x, 0, SIZE - 1) && K.isIntIn(cur.y, 0, SIZE - 1)) {
        grid.setCursor(cur.x, cur.y);
      }
      loadBest();
      if (state.solved) {
        setMessage("Pond lit in " + state.moves + (state.moves === 1 ? " move." : " moves."));
      }
      render();
      save();
    } else {
      startPond(L.dateSeed(new Date()), "daily");
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
  });
})();
