// Calm Cards, engine lullshift.cards@1 (ADR-016). The deck arrives as `content` in onInit.
(function () {
  "use strict";
  var deck = null;
  var index = 0;
  var revealed = false;

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

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // The host validates packs; this is a cheap second look so a bad deck never breaks the page.
  function validDeck(c) {
    if (!c || typeof c !== "object" || !isText(c.title, 80)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (!Array.isArray(c.cards) || c.cards.length < 1 || c.cards.length > 60) return false;
    for (var i = 0; i < c.cards.length; i++) {
      var k = c.cards[i];
      if (!k || typeof k !== "object" || !isText(k.front, 200)) return false;
      if (k.back !== undefined && !isText(k.back, 400)) return false;
    }
    return true;
  }

  function atEnd() {
    return deck !== null && index >= deck.cards.length;
  }

  function state() {
    return { index: index, revealed: revealed };
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
    var total = deck.cards.length;
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
    var card = deck.cards[index];
    var hasBack = typeof card.back === "string";
    el.front.textContent = card.front;
    el.back.textContent = hasBack && revealed ? card.back : "";
    el.back.hidden = !(hasBack && revealed);
    el.progress.textContent = "Card " + (index + 1) + " of " + total;
    el.prev.hidden = false;
    el.prev.disabled = index === 0;
    el.more.hidden = !hasBack;
    el.more.disabled = revealed;
    el.next.hidden = false;
    el.again.hidden = true;
  }

  function report() {
    render();
    LullshiftBridge.autosave(state());
    if (deck) {
      LullshiftBridge.summary([
        {
          label: atEnd() ? "Deck" : "Card",
          value: atEnd() ? "finished" : index + 1 + " of " + deck.cards.length,
        },
      ]);
    }
  }

  function go(to) {
    if (!deck) return;
    var target = Math.max(0, Math.min(deck.cards.length, to));
    if (target === index) return;
    index = target;
    revealed = false;
    report();
  }

  el.prev.addEventListener("click", function () {
    go(index - 1);
    el.card.focus();
  });
  el.next.addEventListener("click", function () {
    go(index + 1);
    if (atEnd()) el.again.focus();
  });
  el.more.addEventListener("click", function () {
    if (!deck || atEnd() || revealed) return;
    revealed = true;
    report();
    el.card.focus();
  });
  el.again.addEventListener("click", function () {
    index = 0;
    revealed = false;
    report();
    el.card.focus();
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      go(index - 1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      go(index + 1);
    }
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      deck = validDeck(init.content) ? init.content : null;
      index = 0;
      revealed = false;
      var r = init.restore;
      if (deck && r && typeof r === "object") {
        var i = typeof r.index === "number" && isFinite(r.index) ? Math.floor(r.index) : 0;
        index = Math.max(0, Math.min(deck.cards.length, i));
        revealed =
          r.revealed === true &&
          index < deck.cards.length &&
          typeof deck.cards[index].back === "string";
      }
      report();
    },
    saveState: function () {
      return state();
    },
  });
  render();
})();
