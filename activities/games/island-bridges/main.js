// Island Bridges: join numbered islands with bridges, on the grid kit.
(function () {
  "use strict";
  var L = globalThis.IslandLogic;
  var K = window.LullshiftKits.grid;

  var size = 7;
  var grid = null;
  var view = null;
  var puzzle = null;
  var game = null; // { seed, mode, size, bridges, history, moves, selected, won }
  var spans = [];
  var bestToday = null;

  function $(id) {
    return document.getElementById(id);
  }

  function idx(x, y) {
    return y * size + x;
  }

  function buildBoard(newSize) {
    size = newSize;
    if (view) view.destroy();
    grid = new K.Grid(size, size, 0);
    $("grid").setAttribute("data-size", String(size));
    view = K.bindGrid($("grid"), grid, {
      name: "Island Bridges sea, " + size + " by " + size,
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
        if (action === "cancel" && game.selected !== -1) {
          game.selected = -1;
          setMessage("Island released.");
          render();
          save();
          return true;
        }
        return false;
      },
    });
  }

  function islandIndex(x, y) {
    return L.islandAt(puzzle, x, y);
  }

  function bridgeWords(i) {
    var parts = [];
    ["up", "right", "down", "left"].forEach(function (dir) {
      var j = L.neighbourIsland(puzzle, i, dir);
      if (j === -1) return;
      var c = L.countBetween(game.bridges, i, j);
      if (c > 0) parts.push(c + " " + L.DIRS[dir].word);
    });
    return parts.length ? ": " + parts.join(", ") : "";
  }

  function label(v, x, y) {
    var base = "Row " + (y + 1) + ", column " + (x + 1) + ", ";
    var i = islandIndex(x, y);
    if (i !== -1) {
      var isl = puzzle.islands[i];
      var d = L.degree(game.bridges, i);
      var t = base + "island " + isl.n + ", " + d + " of " + isl.n + " bridges" + bridgeWords(i);
      if (d === isl.n) t += ", complete";
      if (i === game.selected)
        t += ", selected. Arrow keys add a bridge, Shift plus arrow removes one";
      return t;
    }
    var b = spans[idx(x, y)];
    if (b) {
      return (
        base +
        (b.c === 2 ? "double" : "single") +
        " bridge, " +
        (b.o === "h" ? "east to west" : "north to south")
      );
    }
    return base + "water";
  }

  function text(v, x, y) {
    var i = islandIndex(x, y);
    return i === -1 ? "" : String(puzzle.islands[i].n);
  }

  function classes(v, x, y) {
    var i = islandIndex(x, y);
    var out = [];
    if (i !== -1) {
      out.push("island");
      var d = L.degree(game.bridges, i);
      if (d === puzzle.islands[i].n) out.push("done");
      if (i === game.selected) out.push("sel");
      else if (game.selected !== -1 && isReach(i)) out.push("reach");
    } else {
      var b = spans[idx(x, y)];
      if (b) out.push(b.o + b.c);
    }
    return out.join(" ");
  }

  function isReach(j) {
    var s = game.selected;
    if (s === -1) return false;
    return ["up", "right", "down", "left"].some(function (dir) {
      return L.neighbourIsland(puzzle, s, dir) === j;
    });
  }

  function setMessage(t) {
    $("message").textContent = t;
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
      bridges: [],
      history: [],
      moves: 0,
      selected: -1,
      won: false,
    };
    size = sz;
    spans = [];
    if (!grid || grid.width !== sz) buildBoard(sz);
    var f = puzzle.islands[0];
    grid.setCursor(f.x, f.y);
    spans = L.cellBridges(puzzle, game.bridges);
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          mode: game.mode,
          size: game.size,
          bridges: game.bridges,
          history: game.history,
          moves: game.moves,
          selected: game.selected,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function validHistory(h, p) {
    return (
      Array.isArray(h) &&
      h.every(function (m) {
        return (
          m &&
          Number.isInteger(m.a) &&
          Number.isInteger(m.b) &&
          m.a >= 0 &&
          m.b < p.islands.length &&
          m.a < m.b &&
          (m.from === 0 || m.from === 1 || m.from === 2) &&
          (m.to === 0 || m.to === 1 || m.to === 2)
        );
      })
    );
  }

  function validState(r) {
    if (!r || r.v !== 1 || !Number.isInteger(r.seed) || !L.isSize(r.size)) return false;
    var p = L.generate(r.seed, r.size);
    return L.validBridges(p, r.bridges) && validHistory(r.history, p);
  }

  function restoreGame(r) {
    startPuzzle(r.seed, r.mode === "daily" ? "daily" : "random", r.size);
    game.bridges = L.cloneBridges(r.bridges);
    game.history = r.history.slice(-L.HISTORY_LIMIT);
    game.moves = Number.isInteger(r.moves) && r.moves >= 0 ? r.moves : 0;
    if (Number.isInteger(r.selected) && r.selected >= 0 && r.selected < puzzle.islands.length) {
      game.selected = r.selected;
    }
    var c = r.cursor;
    if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
    game.won = L.isSolved(puzzle, game.bridges);
    spans = L.cellBridges(puzzle, game.bridges);
  }

  function render() {
    spans = L.cellBridges(puzzle, game.bridges);
    view.render();
    var k = puzzle.islands.length;
    var done = L.doneCount(puzzle, game.bridges);
    var groups = L.groupCount(puzzle, game.bridges);
    $("moves").textContent = "Moves: " + game.moves;
    $("done").textContent = "Complete " + done + " of " + k;
    $("groups").textContent = groups === 1 ? "One group" : groups + " groups";
    $("undo").disabled = game.history.length === 0;
    $("size").value = String(size);
    var best = $("best");
    if (game.mode === "daily" && bestToday !== null) {
      best.hidden = false;
      best.textContent = "Best today: " + bestToday;
    } else {
      best.hidden = true;
    }
    var items = [
      { label: "Complete", value: done + " of " + k },
      { label: "Groups", value: groups },
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

  function reasonText(r) {
    switch (r.reason) {
      case "no island":
        return "There is no island that way.";
      case "too close":
        return "These islands are side by side, with no room for a bridge.";
      case "two already":
        return "There are already two bridges here.";
      case "full":
        return "That island already has all its bridges.";
      case "crossed":
        return "A bridge would cross another bridge.";
      case "none":
        return "There is no bridge to remove there.";
      default:
        return "That does not work.";
    }
  }

  function checkWin() {
    var solved = L.isSolved(puzzle, game.bridges);
    if (solved && !game.won) {
      game.won = true;
      recordBest();
      setMessage("All islands are joined in " + movesText(game.moves) + ". Well done.");
    } else if (!solved) {
      game.won = false;
    }
  }

  function applyResult(r, verb) {
    if (!r.ok) {
      setMessage(reasonText(r));
      return;
    }
    game.bridges = r.bridges;
    game.history = L.pushMove(
      game.history,
      game.selected !== -1 ? game.selected : r.j,
      r.j,
      r.from,
      r.to,
    );
    game.moves += 1;
    var j = r.j;
    setMessage(
      verb +
        " " +
        r.to +
        (r.to === 1 ? " bridge" : " bridges") +
        " to the island at row " +
        (puzzle.islands[j].y + 1) +
        ", column " +
        (puzzle.islands[j].x + 1) +
        ".",
    );
    checkWin();
  }

  function bridgeStep(dir, remove) {
    var s = game.selected;
    if (s === -1) return;
    var r = remove
      ? L.removeBridge(puzzle, game.bridges, s, dir)
      : L.addBridge(puzzle, game.bridges, s, dir);
    applyResult(r, "Now");
  }

  function activate(x, y) {
    var i = islandIndex(x, y);
    if (i !== -1) {
      if (game.selected === i) {
        game.selected = -1;
        setMessage("Island released.");
      } else if (game.selected !== -1 && dirToward(game.selected, i)) {
        bridgeStep(dirToward(game.selected, i), false);
      } else {
        game.selected = i;
        setMessage(
          "Island " +
            puzzle.islands[i].n +
            " selected. Arrow keys add a bridge, Shift plus arrow removes one.",
        );
      }
    } else if (spans[idx(x, y)]) {
      var b = spans[idx(x, y)];
      var saved = game.selected;
      game.selected = b.a;
      bridgeStep(b.o === "h" ? "right" : "down", true);
      game.selected = saved;
    } else if (game.selected !== -1) {
      game.selected = -1;
      setMessage("Island released.");
    } else {
      setMessage("Press Enter on an island to select it.");
    }
    render();
    save();
  }

  function dirToward(from, to) {
    var found = null;
    ["up", "right", "down", "left"].forEach(function (dir) {
      if (L.neighbourIsland(puzzle, from, dir) === to) found = dir;
    });
    return found;
  }

  function onKeyCapture(e) {
    if (!game || !grid || !$("grid").contains(e.target)) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (game.selected === -1) return;
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
    if (!dir) return;
    e.preventDefault();
    e.stopPropagation();
    bridgeStep(dir, e.shiftKey);
    render();
    save();
  }

  function undoMove() {
    if (game.history.length === 0) return;
    var r = L.undo(game.bridges, game.history);
    game.bridges = r.bridges;
    game.history = r.history;
    game.moves = Math.max(0, game.moves - 1);
    game.won = L.isSolved(puzzle, game.bridges);
    setMessage("Undone. " + movesText(game.moves) + ".");
    render();
    save();
  }

  function restart() {
    game.bridges = [];
    game.history = [];
    game.moves = 0;
    game.selected = -1;
    game.won = false;
    setMessage("Bridges cleared.");
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

  function chosenSize() {
    var n = parseInt($("size").value, 10);
    return L.isSize(n) ? n : size;
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var restored = validState(init.restore);
    if (restored) restoreGame(init.restore);
    else startPuzzle(L.dateSeed(new Date(), 7), "daily", 7);

    document.addEventListener("keydown", onKeyCapture, true);

    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      var s = chosenSize();
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " sea.");
    });
    $("fresh").addEventListener("click", function () {
      var s = chosenSize();
      newPuzzle(K.randomSeed(), "random", s, "A new " + s + " by " + s + " sea.");
    });
    $("size").addEventListener("change", function () {
      var s = chosenSize();
      newPuzzle(L.dateSeed(new Date(), s), "daily", s, "Today's " + s + " by " + s + " sea.");
    });

    if (restored) {
      if (game.won) setMessage("All islands are joined in " + movesText(game.moves) + ".");
      loadBest();
      render();
    } else {
      setMessage("Today's 7 by 7 sea. Press Enter on an island to select it.");
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
