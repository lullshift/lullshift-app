// Calm Cards, engine lullshift.cards@1 (ADR-016). The deck arrives as `content` in onInit.
(function () {
  "use strict";
  var L = globalThis.CardsLogic;
  var deck = null;
  // { index, revealed }, plus { seed, seen } when the deck sets roundSize (see logic.js).
  var state = { index: 0, revealed: false };
  // The cards of the current round: the whole deck, or a drawn round when it sets roundSize.
  var cards = [];

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    card: document.getElementById("card"),
    front: document.getElementById("front"),
    back: document.getElementById("back"),
    progress: document.getElementById("progress"),
    prev: document.getElementById("prev"),
    more: document.getElementById("more"),
    next: document.getElementById("next"),
    again: document.getElementById("again"),
  };

  function newSeed() {
    return Math.floor(Math.random() * 4294967296);
  }

  function setState(next) {
    state = next;
    cards = deck ? L.roundCards(deck, state) : [];
  }

  function atEnd() {
    return deck !== null && state.index >= cards.length;
  }

  function render() {
    if (!deck) {
      el.title.textContent = "Calm Cards";
      el.intro.hidden = true;
      el.front.textContent = "This engine needs a card pack";
      el.back.textContent = "Open a card pack to read its cards here.";
      el.back.hidden = false;
      el.progress.textContent = "";
      el.prev.hidden = true;
      el.more.hidden = true;
      el.next.hidden = true;
      el.again.hidden = true;
      return;
    }
    var total = cards.length;
    el.title.textContent = deck.title;
    el.intro.hidden = !deck.intro;
    el.intro.textContent = deck.intro || "";
    if (atEnd()) {
      el.front.textContent = "That's the deck";
      el.back.textContent = "";
      el.back.hidden = true;
      el.progress.textContent = total + " cards read";
      el.prev.hidden = false;
      el.prev.disabled = false;
      el.more.hidden = true;
      el.next.hidden = true;
      el.again.hidden = false;
      return;
    }
    var card = cards[state.index];
    var hasBack = typeof card.back === "string";
    el.front.textContent = card.front;
    el.back.textContent = hasBack && state.revealed ? card.back : "";
    el.back.hidden = !(hasBack && state.revealed);
    el.progress.textContent = "Card " + (state.index + 1) + " of " + total;
    el.prev.hidden = false;
    el.prev.disabled = state.index === 0;
    el.more.hidden = !hasBack;
    el.more.disabled = state.revealed;
    el.next.hidden = false;
    el.again.hidden = true;
  }

  function report() {
    render();
    LullshiftBridge.autosave(state);
    if (deck) {
      LullshiftBridge.summary([
        {
          label: atEnd() ? "Deck" : "Card",
          value: atEnd() ? "finished" : state.index + 1 + " of " + cards.length,
        },
      ]);
    }
  }

  // Moves within the round, keeping the round fields as they are.
  function moveTo(index, revealed) {
    var next = {};
    for (var key in state) next[key] = state[key];
    next.index = index;
    next.revealed = revealed;
    state = next;
  }

  function go(to) {
    if (!deck) return;
    var target = Math.max(0, Math.min(cards.length, to));
    if (target === state.index) return;
    moveTo(target, false);
    report();
  }

  el.prev.addEventListener("click", function () {
    go(state.index - 1);
    el.card.focus();
  });
  el.next.addEventListener("click", function () {
    go(state.index + 1);
    if (atEnd()) el.again.focus();
  });
  el.more.addEventListener("click", function () {
    if (!deck || atEnd() || state.revealed) return;
    moveTo(state.index, true);
    report();
    el.card.focus();
  });
  el.again.addEventListener("click", function () {
    if (deck) setState(L.nextRound(deck, state, newSeed()));
    report();
    el.card.focus();
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      go(state.index - 1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      go(state.index + 1);
    }
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      deck = L.validDeck(init.content) ? init.content : null;
      if (deck) setState(L.restore(deck, init.restore) || L.freshState(deck, newSeed()));
      else setState({ index: 0, revealed: false });
      report();
    },
    saveState: function () {
      return state;
    },
  });
  render();
})();
