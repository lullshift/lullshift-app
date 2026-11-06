(function () {
  "use strict";
  var L = globalThis.TeaLogic;
  var K = window.LullshiftKits.grid;
  var CUPS = L.CUPS;
  var CAP = L.CAP;
  var grid = new K.Grid(CUPS, CAP, 0);
  var view = null;
  var game = null; // { seed, mode, start, cups, history, picked, solved }
  var bestToday = null;

  function $(id) {
    return document.getElementById(id);
  }

  function setGrid() {
    for (var c = 0; c < CUPS; c++) {
      for (var y = 0; y < CAP; y++) {
        var layer = CAP - 1 - y;
        grid.set(c, y, game.cups[c][layer] || 0);
      }
    }
  }

  function startPuzzle(seed, mode) {
    var g = L.generate(seed);
    game = {
      seed: seed,
      mode: mode,
      start: g.cups.map(function (c) {
        return c.slice();
      }),
      cups: g.cups,
      history: [],
      picked: -1,
      solved: false,
    };
    setGrid();
    grid.setCursor(0, CAP - 1);
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          start: game.start,
          cups: game.cups,
          history: game.history,
          picked: game.picked,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validState(r) {
    return (
      r &&
      r.v === 1 &&
      Number.isInteger(r.seed) &&
      L.validCups(r.start) &&
      L.validCups(r.cups) &&
      Array.isArray(r.history) &&
      r.history.length <= L.HISTORY_LIMIT &&
      r.history.every(L.isMoveCode)
    );
  }

  function restorePuzzle(r) {
    game = {
      seed: r.seed,
      mode: r.mode === "daily" ? "daily" : "random",
      start: r.start.map(function (c) {
        return c.slice();
      }),
      cups: r.cups.map(function (c) {
        return c.slice();
      }),
      history: r.history.slice(),
      picked: Number.isInteger(r.picked) && r.picked >= 0 && r.picked < CUPS ? r.picked : -1,
      solved: false,
    };
    if (game.picked >= 0 && game.cups[game.picked].length === 0) game.picked = -1;
    game.solved = L.isSolved(game.cups);
    if (game.solved) game.picked = -1;
    setGrid();
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
  }

  function label(value, x, y) {
    var layer = CAP - 1 - y;
    var text = "Cup " + (x + 1) + " of " + CUPS + ", layer " + (layer + 1) + " from the bottom, ";
    text += value === 0 ? "empty" : L.NAMES[value] + ", " + L.SHAPES[value];
    if (game.picked === x) text += ", cup picked up";
    return text;
  }

  function classes(value, x, y) {
    var out = ["lay"];
    out.push(value === 0 ? "void" : "t" + value);
    if (y === CAP - 1) out.push("base");
    if (y === 0) out.push("rim");
    if (game.picked === x) out.push("picked");
    return out.join(" ");
  }

  function cellText(value) {
    return value === 0 ? "" : L.NAMES[value].charAt(0);
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function movesText(n) {
    return n + (n === 1 ? " move" : " moves");
  }

  function render() {
    setGrid();
    view.render();
    var moves = game.history.length;
    var done = L.sortedCount(game.cups);
    $("moves").textContent = "Moves: " + moves;
    $("sorted").textContent = "Sorted " + done + " of " + L.COLOURS;
    $("undo").disabled = moves === 0;
    var best = $("best");
    if (game.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Best today: " + bestToday;
    } else {
      best.hidden = true;
    }
    var items = [
      { label: "Moves", value: moves },
      { label: "Sorted", value: done + " of " + L.COLOURS },
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
    var moves = game.history.length;
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

  function activateCup(c) {
    if (game.solved) return;
    if (game.picked < 0) {
      if (game.cups[c].length === 0) {
        setMessage("Cup " + (c + 1) + " is empty. Pick a cup that holds tea.");
        return;
      }
      game.picked = c;
      var top = game.cups[c][game.cups[c].length - 1];
      setMessage(
        "Picked up " + L.NAMES[top] + " from cup " + (c + 1) + ". Choose a cup to pour into.",
      );
      render();
      save();
      return;
    }
    if (game.picked === c) {
      game.picked = -1;
      setMessage("Put cup " + (c + 1) + " back.");
      render();
      save();
      return;
    }
    var from = game.picked;
    var result = L.pour(game.cups, from, c);
    if (!result) {
      var target = game.cups[c];
      if (target.length >= CAP) setMessage("Cup " + (c + 1) + " is full.");
      else setMessage("Cup " + (c + 1) + " holds a different tea on top.");
      return;
    }
    game.history = L.pushMove(game.history, L.encode(from, c, result.amount));
    game.cups = result.cups;
    game.picked = -1;
    var poured =
      "Poured " + result.amount + (result.amount === 1 ? " layer" : " layers") + " from cup ";
    if (L.isSolved(game.cups)) {
      game.solved = true;
      render();
      recordBest();
      setMessage("Every cup holds one tea. Solved in " + movesText(game.history.length) + ".");
      render();
    } else {
      setMessage(poured + (from + 1) + " to cup " + (c + 1) + ".");
      render();
    }
    save();
  }

  function undoMove() {
    if (game.history.length === 0) return;
    var code = game.history[game.history.length - 1];
    var cups = L.undoMove(game.cups, code);
    if (!cups) {
      game.history = [];
      setMessage("Could not undo that move.");
      render();
      save();
      return;
    }
    game.cups = cups;
    game.history = game.history.slice(0, -1);
    game.picked = -1;
    game.solved = false;
    setMessage("Undone. " + movesText(game.history.length) + ".");
    render();
    save();
  }

  function restart() {
    game.cups = game.start.map(function (c) {
      return c.slice();
    });
    game.history = [];
    game.picked = -1;
    game.solved = false;
    setMessage("Puzzle reset.");
    render();
    save();
  }

  function newPuzzle(seed, mode, text) {
    startPuzzle(seed, mode);
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restorePuzzle(init.restore);
    else startPuzzle(K.dailySeed(new Date(), "teacup-tidy"), "daily");

    view = K.bindGrid($("grid"), grid, {
      name: "Teacup Tidy, " + CUPS + " cups of " + CAP + " layers",
      label: label,
      text: cellText,
      className: classes,
      onActivate: function (x) {
        activateCup(x);
      },
      onMove: function () {
        save();
      },
      onAction: function (action) {
        if (action === "undo") {
          undoMove();
          return true;
        }
        if (action === "cancel" && game.picked >= 0) {
          game.picked = -1;
          setMessage("Put the cup back.");
          save();
          return true;
        }
        return false;
      },
    });

    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      newPuzzle(K.dailySeed(new Date(), "teacup-tidy"), "daily", "Today's puzzle.");
    });
    $("fresh").addEventListener("click", function () {
      newPuzzle(K.randomSeed(), "random", "A new puzzle.");
    });

    if (restored) {
      if (game.solved)
        setMessage("Every cup holds one tea. Solved in " + movesText(game.history.length) + ".");
      loadBest();
      render();
    } else {
      setMessage("Today's puzzle.");
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
