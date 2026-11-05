(function () {
  "use strict";
  var L = globalThis.RidgeWalkLogic;
  var K = globalThis.LullshiftKits.card;
  var state = null;
  var focusRidge = 0;
  var lastReport = "";

  function $(id) {
    return document.getElementById(id);
  }

  function plural(n, one, many) {
    return n + " " + (n === 1 ? one : many);
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function endMessage() {
    if (L.isWon(state)) return "The ridges are clear. You walked the whole path. Well done.";
    if (L.isStuck(state)) {
      return (
        "The path ends here: " + L.cardsLeft(state) + " cards left. Try again or start a new deal."
      );
    }
    return "";
  }

  function el(tag, className, text) {
    var e = document.createElement(tag);
    if (className) e.className = className;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function cardFace(card, extra) {
    var c = el("span", "card" + (L.isRed(card) ? " red" : "") + (extra ? " " + extra : ""));
    c.appendChild(el("span", "big", L.cardText(card)));
    return c;
  }

  function nearestFilled(from, dir) {
    for (var i = from + dir; i >= 0 && i < L.RIDGES; i += dir) {
      if (state.ridges[i].length) return i;
    }
    return -1;
  }

  function ensureFocusRidge() {
    if (state.ridges[focusRidge] && state.ridges[focusRidge].length) return;
    var r = nearestFilled(focusRidge, 1);
    if (r < 0) r = nearestFilled(focusRidge, -1);
    if (r >= 0) focusRidge = r;
  }

  function ridgeLabel(r) {
    var cards = state.ridges[r];
    if (!cards.length) return "Ridge " + (r + 1) + ", cleared";
    var top = cards[cards.length - 1];
    return (
      "Ridge " +
      (r + 1) +
      ", " +
      plural(cards.length, "card", "cards") +
      ", top card " +
      L.cardName(top) +
      (L.canPlay(state, r) ? ", can be played" : ", does not fit the path card")
    );
  }

  function render() {
    ensureFocusRidge();
    var hadFocus = $("ridges").contains(document.activeElement);
    var ridges = $("ridges");
    ridges.textContent = "";
    for (var r = 0; r < L.RIDGES; r++) {
      var cards = state.ridges[r];
      var col = el("div", "ridge");
      if (!cards.length) {
        var slot = el("div", "slot");
        slot.setAttribute("aria-hidden", "true");
        col.appendChild(slot);
      } else {
        for (var i = 0; i < cards.length - 1; i++) {
          var under = cardFace(cards[i], "under");
          under.setAttribute("aria-hidden", "true");
          col.appendChild(under);
        }
        var top = cards[cards.length - 1];
        var b = el(
          "button",
          "card" + (L.isRed(top) ? " red" : "") + (L.canPlay(state, r) ? " fits" : ""),
        );
        b.type = "button";
        b.dataset.ridge = String(r);
        b.tabIndex = r === focusRidge ? 0 : -1;
        b.setAttribute("aria-label", ridgeLabel(r));
        b.appendChild(el("span", "big", L.cardText(top)));
        col.appendChild(b);
      }
      ridges.appendChild(col);
    }

    var path = $("path");
    path.textContent = "";
    var top2 = L.pathTop(state);
    var stockN = state.stock.length;
    var stack = el("div", "stack" + (stockN ? "" : " empty"));
    stack.appendChild(el("span", "", stockN ? String(stockN) : "none"));
    stack.setAttribute("aria-hidden", "true");
    path.appendChild(stack);
    path.appendChild(top2 === null ? el("span", "slot") : cardFace(top2));
    path.setAttribute(
      "aria-label",
      "Path card: " +
        (top2 === null ? "none" : L.cardName(top2)) +
        ". Stock: " +
        plural(stockN, "card", "cards"),
    );

    var left = L.cardsLeft(state);
    $("left").textContent = "Left: " + left;
    $("stock").textContent = "Stock: " + stockN;
    $("undo").disabled = state.moves.length === 0;
    $("flip").disabled = !L.canFlip(state) || L.isWon(state);
    if (hadFocus) focusTop();
    report(left, stockN);
  }

  function focusTop() {
    var b = document.querySelector('#ridges button[data-ridge="' + focusRidge + '"]');
    if (b) b.focus();
  }

  // What the host shows while the game is paused (S05-T09).
  function report(left, stockN) {
    var sig = left + "/" + stockN;
    if (sig === lastReport) return;
    lastReport = sig;
    LullshiftBridge.summary([
      { label: "Left", value: left },
      { label: "Stock", value: stockN },
    ]);
  }

  function snapshot() {
    return state ? L.snapshot(state) : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function afterMove(announce) {
    var end = endMessage();
    setMessage(end || announce);
    render();
    save();
  }

  function playRidge(r) {
    if (L.isWon(state)) return;
    var cards = state.ridges[r];
    if (!cards.length) return;
    var card = cards[cards.length - 1];
    var next = L.apply(state, r);
    if (!next) {
      var pt = L.pathTop(state);
      setMessage(
        L.cardName(card) +
          " does not fit: it must be one higher or one lower than " +
          (pt === null ? "the path card" : L.cardName(pt)) +
          ".",
      );
      return;
    }
    focusRidge = r;
    state = next;
    afterMove(
      "Played " + L.cardName(card) + ". " + plural(L.cardsLeft(state), "card", "cards") + " left.",
    );
  }

  function flip() {
    var next = L.apply(state, L.FLIP);
    if (!next) return;
    state = next;
    var top = L.pathTop(state);
    afterMove(
      "Flipped " +
        L.cardName(top) +
        ". " +
        plural(state.stock.length, "card", "cards") +
        " in the stock.",
    );
  }

  function undo() {
    var prev = L.undo(state);
    if (!prev) return;
    state = prev;
    afterMove("Undone.");
  }

  function newGame(seed, text) {
    state = L.deal(seed);
    focusRidge = 0;
    lastReport = "";
    setMessage(text);
    render();
    save();
  }

  function onRidgesKey(e) {
    var action = K.keyAction(e);
    if (e.key === "f" || e.key === "F") {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      e.preventDefault();
      flip();
      return;
    }
    if (!action) return;
    var next;
    if (action === "left") next = nearestFilled(focusRidge, -1);
    else if (action === "right") next = nearestFilled(focusRidge, 1);
    else if (action === "home" || action === "first") next = nearestFilled(-1, 1);
    else if (action === "end" || action === "last") next = nearestFilled(L.RIDGES, -1);
    else if (action === "undo") {
      e.preventDefault();
      undo();
      return;
    } else return;
    e.preventDefault();
    if (next >= 0) {
      focusRidge = next;
      render();
      focusTop();
    }
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");
    $("ridges").addEventListener("keydown", onRidgesKey);
    $("ridges").addEventListener("click", function (e) {
      var b = e.target.closest ? e.target.closest("button.card") : null;
      if (!b) return;
      focusRidge = Number(b.dataset.ridge);
      playRidge(focusRidge);
      focusTop();
    });
    $("undo").addEventListener("click", undo);
    $("flip").addEventListener("click", flip);
    $("retry").addEventListener("click", function () {
      newGame(state.seed, "Same deal, from the start.");
    });
    $("fresh").addEventListener("click", function () {
      newGame(K.randomSeed(), "A new deal.");
    });

    var restored = init.restore ? L.restore(init.restore) : null;
    if (restored) {
      state = restored;
      setMessage(endMessage());
      render();
      save();
    } else {
      newGame(K.randomSeed(), "Play a ridge card one higher or one lower than the path card.");
    }
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
  });
})();
