(function () {
  "use strict";
  var L = globalThis.LanternLogic;
  var SIZE = L.SIZE;
  var state = null;
  var cells = [];
  var focusIndex = 0;
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
      typeof r.seed === "number"
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

  function label(i) {
    var r = Math.floor(i / SIZE) + 1;
    var c = (i % SIZE) + 1;
    return "Lantern row " + r + ", column " + c + ", " + (state.board[i] ? "lit" : "unlit");
  }

  function buildGrid() {
    var grid = $("grid");
    grid.textContent = "";
    cells = [];
    for (var r = 0; r < SIZE; r++) {
      var row = document.createElement("div");
      row.className = "row";
      row.setAttribute("role", "row");
      for (var c = 0; c < SIZE; c++) {
        var i = r * SIZE + c;
        var cell = document.createElement("div");
        cell.className = "cell";
        cell.setAttribute("role", "gridcell");
        var b = document.createElement("button");
        b.type = "button";
        b.className = "lantern";
        b.dataset.index = String(i);
        b.tabIndex = i === focusIndex ? 0 : -1;
        cell.appendChild(b);
        row.appendChild(cell);
        cells.push(b);
      }
      grid.appendChild(row);
    }
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function render() {
    for (var i = 0; i < L.CELLS; i++) {
      var b = cells[i];
      var lit = state.board[i];
      b.setAttribute("aria-pressed", lit ? "true" : "false");
      b.setAttribute("aria-label", label(i));
      b.textContent = lit ? "●" : "○";
      b.tabIndex = i === focusIndex ? 0 : -1;
    }
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
    focusIndex = i;
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
    focusIndex = 0;
    setMessage(mode === "daily" ? "Today's pond." : "A new pond.");
    loadBest();
    render();
    save();
  }

  function onGridKey(e) {
    var t = e.target;
    if (!t || !t.dataset || t.dataset.index === undefined) return;
    var i = Number(t.dataset.index);
    var r = Math.floor(i / SIZE);
    var c = i % SIZE;
    var next = null;
    if (e.key === "ArrowRight") next = r * SIZE + Math.min(SIZE - 1, c + 1);
    else if (e.key === "ArrowLeft") next = r * SIZE + Math.max(0, c - 1);
    else if (e.key === "ArrowDown") next = Math.min(SIZE - 1, r + 1) * SIZE + c;
    else if (e.key === "ArrowUp") next = Math.max(0, r - 1) * SIZE + c;
    else if (e.key === "Home") next = r * SIZE;
    else if (e.key === "End") next = r * SIZE + SIZE - 1;
    if (next === null) return;
    e.preventDefault();
    cells[focusIndex].tabIndex = -1;
    focusIndex = next;
    cells[next].tabIndex = 0;
    cells[next].focus();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");
    buildGrid();
    $("grid").addEventListener("keydown", onGridKey);
    $("grid").addEventListener("click", function (e) {
      var b = e.target.closest ? e.target.closest("button.lantern") : null;
      if (b) pressCell(Number(b.dataset.index));
    });
    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      startPond(L.dateSeed(new Date()), "daily");
    });
    $("fresh").addEventListener("click", function () {
      startPond(Math.floor(Math.random() * 2147483647) + 1, "random");
    });

    if (validState(init.restore)) {
      state = restoreState(init.restore);
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
