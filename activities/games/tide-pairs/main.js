(function () {
  "use strict";
  var L = globalThis.TidePairsLogic;
  var K = globalThis.LullshiftKits.card;
  var PAUSE_MS = 900;
  var state = null;
  var table = null; // a card-kit Table: one pile per slot, one card in each
  var view = null; // its bindTable binding
  var reduced = false;
  var timer = null;
  var best = null; // fewest turns for the current size, if known
  var lastReport = "";

  function $(id) {
    return document.getElementById(id);
  }

  function cols() {
    return L.COLUMNS[state.pairs];
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function title(name) {
    return name.charAt(0).toUpperCase() + name.slice(1);
  }

  function slotIndex(pile) {
    return Number(pile.id.slice(1));
  }

  function slotLabel(card, pile) {
    var i = slotIndex(pile);
    var base = "Card " + pile.name + ", ";
    if (!L.isUp(state, i)) return base + "face down";
    return base + L.symbolAt(state, i) + (state.matched[i] ? ", matched" : ", face up");
  }

  // Builds the table and its binding for the current game, with the cursor on the first shell.
  function buildBoard() {
    if (view) view.destroy();
    var c = cols();
    var rows = (state.pairs * 2) / c;
    var board = $("board");
    board.className = "board cols-" + c;
    table = new K.Table(L.pileSpecs(state), { columns: c });
    table.focus = { pile: 0, card: 0 };
    view = K.bindTable(board, table, {
      name: "Tide pairs, " + rows + " by " + c,
      label: slotLabel,
      text: function (card, pile) {
        return L.isUp(state, slotIndex(pile)) ? card.suit : "";
      },
      className: function (card, pile) {
        var i = slotIndex(pile);
        if (!L.isUp(state, i)) return "tile down";
        return "tile s-" + card.suit + (state.matched[i] ? " matched" : " up");
      },
      onActivate: function (pileId) {
        pressTile(Number(pileId.slice(1)));
        return true;
      },
    });
  }

  // Copies the game state onto the table's cards, then redraws.
  function syncTable() {
    for (var i = 0; i < table.piles.length; i++) {
      table.piles[i].cards[0].faceUp = L.isUp(state, i);
    }
    view.render();
  }

  function render() {
    syncTable();
    var found = L.pairsFound(state);
    $("pairs").textContent = "Pairs: " + found + " of " + state.pairs;
    $("turns").textContent = "Turns: " + state.turns;
    var bestEl = $("best");
    if (best !== null) {
      bestEl.hidden = false;
      bestEl.textContent = "Best: " + best;
    } else {
      bestEl.hidden = true;
    }
    $("size").textContent = state.pairs === 8 ? "Bigger" : "Smaller";
    $("size").setAttribute(
      "aria-label",
      state.pairs === 8 ? "Bigger board, 12 pairs" : "Smaller board, 8 pairs",
    );
    report();
  }

  // What the host shows while the game is paused (S05-T09).
  function report() {
    var found = L.pairsFound(state);
    var sig = [found, state.pairs, state.turns, best].join("/");
    if (sig === lastReport) return;
    lastReport = sig;
    var items = [
      { label: "Pairs", value: found + " of " + state.pairs },
      { label: "Turns", value: state.turns },
    ];
    if (best !== null) items.push({ label: "Best", value: best });
    LullshiftBridge.summary(items);
  }

  function snapshot() {
    return state ? L.snapshot(state) : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function bestKey() {
    return "best-" + state.pairs;
  }
  function loadBest() {
    best = null;
    var pairs = state.pairs;
    try {
      LullshiftBridge.storage.get("best-" + pairs).then(
        function (v) {
          var n = parseInt(v, 10);
          if (state && state.pairs === pairs && Number.isFinite(n) && n > 0) {
            best = n;
            render();
          }
        },
        function () {},
      );
    } catch (e) {
      /* storage unavailable: show no best */
    }
  }
  function recordBest() {
    if (best !== null && best <= state.turns) return;
    best = state.turns;
    try {
      LullshiftBridge.storage.set(bestKey(), state.turns).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function clearTimer() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  // Turns an unmatched pair back down. Safe to call at any time.
  function settle() {
    clearTimer();
    if (!L.needsSettle(state)) return false;
    state = L.settle(state);
    render();
    save();
    return true;
  }

  function winText() {
    return (
      "All " +
      state.pairs +
      " pairs found in " +
      state.turns +
      (state.turns === 1 ? " turn." : " turns.") +
      " Well done."
    );
  }

  function pressTile(i) {
    // A pair that did not match is still showing: turn it back first, then take the press.
    if (L.needsSettle(state)) settle();
    if (L.isUp(state, i)) {
      if (state.matched[i]) setMessage(title(L.symbolAt(state, i)) + " is already matched.");
      return;
    }
    var r = L.flip(state, i);
    if (r.result === "ignored") return;
    state = r.state;
    table.focus = { pile: i, card: 0 };
    var name = title(L.symbolAt(state, i));
    if (r.result === "first") {
      setMessage(name + ". Now find its pair.");
    } else if (r.result === "match") {
      if (L.isWon(state)) {
        recordBest();
        setMessage(winText());
      } else {
        setMessage(name + " matched. " + L.pairsFound(state) + " of " + state.pairs + " pairs.");
      }
    } else {
      var first = title(L.symbolAt(state, state.open[0]));
      if (reduced) {
        setMessage(first + " and " + name + " do not match. Press any card to turn them back.");
      } else {
        setMessage(first + " and " + name + " do not match.");
        timer = setTimeout(function () {
          timer = null;
          settle();
        }, PAUSE_MS);
      }
    }
    render();
    save();
  }

  function startGame(seed, pairs, mode, text) {
    clearTimer();
    state = L.deal(seed, pairs, mode);
    lastReport = "";
    buildBoard();
    setMessage(text);
    loadBest();
    render();
    save();
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    reduced = init.reducedMotion === true;
    if (reduced) document.documentElement.classList.add("reduced");
    $("daily").addEventListener("click", function () {
      startGame(
        L.dailySeed(new Date()),
        state.pairs,
        "daily",
        "Today's deal. Everyone gets the same shells today.",
      );
    });
    $("fresh").addEventListener("click", function () {
      startGame(K.randomSeed(), state.pairs, "random", "A new deal.");
    });
    $("size").addEventListener("click", function () {
      var pairs = state.pairs === 8 ? 12 : 8;
      startGame(
        K.randomSeed(),
        pairs,
        "random",
        pairs === 12 ? "A bigger board: 12 pairs." : "A smaller board: 8 pairs.",
      );
    });

    var restored = init.restore ? L.restore(init.restore) : null;
    if (restored) {
      state = restored;
      // A pair that did not match turns back when the game comes back.
      if (L.needsSettle(state)) state = L.settle(state);
      buildBoard();
      setMessage(L.isWon(state) ? winText() : "");
      loadBest();
      render();
      save();
    } else {
      startGame(
        L.dailySeed(new Date()),
        8,
        "daily",
        "Today's deal. Turn two shells to find a pair.",
      );
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
    onPause: function () {
      clearTimer();
    },
    onResume: function () {
      // The pause stopped the turn-back timer: turn an unmatched pair back down now.
      if (state) settle();
    },
  });
})();
