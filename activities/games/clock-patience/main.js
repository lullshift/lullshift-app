(function () {
  "use strict";
  var L = globalThis.ClockLogic;
  var K = window.LullshiftKits.card;
  var NS = "http://www.w3.org/2000/svg";
  var STEP_MS = 700;

  var game = null; // ClockLogic state
  var mode = "daily";
  var timer = null;
  var cells = [];
  var reduced = false;

  function $(id) {
    return document.getElementById(id);
  }

  function el(tag, attrs, cls) {
    var n = document.createElementNS(NS, tag);
    for (var k in attrs) n.setAttribute(k, attrs[k]);
    if (cls) n.setAttribute("class", cls);
    return n;
  }

  // Suit art: leaves, acorns, berries, shells.
  function suitIcon(s) {
    var svg = el(
      "svg",
      { viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false" },
      "icon suit" + s,
    );
    if (s === 0) {
      svg.appendChild(
        el("path", { d: "M12 2.5C19.5 6 20.5 14 12 21.5 3.5 14 4.5 6 12 2.5Z" }, "a"),
      );
      svg.appendChild(el("path", { d: "M12 6V20M12 11L8.5 8.5M12 14L15.5 11.5" }, "line"));
    } else if (s === 1) {
      svg.appendChild(
        el("path", { d: "M6.5 11.5H17.5C17.3 17.5 15 20.5 12 22 9 20.5 6.7 17.5 6.5 11.5Z" }, "b"),
      );
      svg.appendChild(el("path", { d: "M4.5 11.5C4.5 6 8 4 12 4S19.5 6 19.5 11.5Z" }, "a"));
      svg.appendChild(el("path", { d: "M12 4V1.8" }, "line"));
    } else if (s === 2) {
      svg.appendChild(el("circle", { cx: "8", cy: "15.5", r: "4.3" }, "a"));
      svg.appendChild(el("circle", { cx: "16", cy: "15.5", r: "4.3" }, "a"));
      svg.appendChild(el("circle", { cx: "12", cy: "9.2", r: "4.3" }, "a"));
      svg.appendChild(el("path", { d: "M12 4.8C12 3 13.5 2 15.5 2.2" }, "line"));
    } else {
      svg.appendChild(el("path", { d: "M12 21.5L2.8 11C6 3.5 18 3.5 21.2 11Z" }, "a"));
      svg.appendChild(
        el(
          "path",
          { d: "M12 21.5V6.2M12 21.5L7 7.8M12 21.5L17 7.8M12 21.5L4.6 10.6M12 21.5L19.4 10.6" },
          "line",
        ),
      );
    }
    return svg;
  }

  function build() {
    var face = $("face");
    for (var p = 0; p < L.PILES; p++) {
      var li = document.createElement("li");
      li.className = p === L.CENTRE ? "pile centre" : "pile pos-" + (p + 1);
      var sr = document.createElement("span");
      sr.className = "sr";
      var label = document.createElement("span");
      label.className = "label";
      label.setAttribute("aria-hidden", "true");
      label.textContent = L.rankText(p);
      var down = document.createElement("span");
      down.className = "down";
      down.setAttribute("aria-hidden", "true");
      var ups = document.createElement("span");
      ups.className = "ups";
      ups.setAttribute("aria-hidden", "true");
      li.appendChild(sr);
      li.appendChild(label);
      li.appendChild(down);
      li.appendChild(ups);
      face.appendChild(li);
      cells[p] = { li: li, sr: sr, down: down, ups: ups, shown: 0 };
    }
  }

  function describe(p) {
    var d = game.down[p].length;
    var u = game.up[p];
    var text = L.pileName(p) + ": " + d + " face down";
    if (u.length) {
      text +=
        ", " +
        u.length +
        " face up: " +
        u
          .map(function (id) {
            return L.SUITS[L.suit(id)];
          })
          .join(", ");
    }
    if (p === game.cur && !game.over) text += ", current pile";
    return text;
  }

  function render() {
    for (var p = 0; p < L.PILES; p++) {
      var c = cells[p];
      var d = game.down[p].length;
      c.down.textContent = d === 0 ? "done" : d + " down";
      c.down.className = d === 0 ? "down none" : "down";
      if (c.shown !== game.up[p].length || c.suits !== game.up[p].join(",")) {
        c.ups.textContent = "";
        game.up[p].forEach(function (id) {
          c.ups.appendChild(suitIcon(L.suit(id)));
        });
        c.shown = game.up[p].length;
        c.suits = game.up[p].join(",");
      }
      c.sr.textContent = describe(p);
      var cls = p === L.CENTRE ? "pile centre" : "pile pos-" + (p + 1);
      if (p === game.cur && !game.over) cls += " current";
      if (d === 0) cls += " done";
      c.li.className = cls;
      if (p === game.cur && !game.over) c.li.setAttribute("aria-current", "true");
      else c.li.removeAttribute("aria-current");
    }
    $("deal").textContent = mode === "daily" ? "Today's deal" : "Deal " + (game.seed % 100000);
    $("turned").textContent = "Turned: " + game.turned + " of " + L.CARDS;
    $("where").textContent = game.over ? "Finished" : "Now at: " + L.pileName(game.cur);
    $("last").textContent = game.last
      ? "Last card: " +
        L.cardName(game.last.card) +
        ", placed under the " +
        L.pileName(game.last.to) +
        "."
      : "";
    var running = timer !== null;
    $("turn").textContent = running ? "Pause" : "Turn";
    $("turn").disabled = game.over;
    $("one").disabled = game.over || running;
    var done = 0;
    for (var q = 0; q < L.PILES; q++) if (game.down[q].length === 0) done += 1;
    LullshiftBridge.summary(
      game.over
        ? [
            {
              label: "Result",
              value: L.won(game) ? "All piles done" : L.pilesLeft(game) + " left",
            },
            { label: "Turned", value: game.turned + " of " + L.CARDS },
          ]
        : [
            { label: "Turned", value: game.turned + " of " + L.CARDS },
            { label: "Piles done", value: done + " of " + L.PILES },
          ],
    );
  }

  function snapshot() {
    return game ? { v: 1, seed: game.seed, mode: mode, turned: game.turned } : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function stopAuto() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function finishUp() {
    stopAuto();
    render();
    setMessage(
      L.resultText(game) + " Chance decides this one. Try another deal whenever you like.",
    );
    save();
    $("fresh").focus();
  }

  // One turn. Returns false when the game is over.
  function turnOne() {
    var r = L.step(game);
    if (!r) return false;
    var from = L.pileName(game.last.from);
    setMessage(
      "Turned " +
        L.cardName(r.card) +
        " from the " +
        from +
        ". It goes under the " +
        L.pileName(r.to) +
        "." +
        (game.over ? "" : " Now at the " + L.pileName(game.cur) + "."),
    );
    render();
    save();
    return true;
  }

  function tick() {
    // Line up the next step first, so the screen shows Pause while cards are turning.
    timer = setTimeout(tick, STEP_MS);
    turnOne();
    if (game.over) finishUp();
  }

  function toggleAuto() {
    if (game.over) return;
    if (timer !== null) {
      stopAuto();
      render();
      setMessage("Paused. Press Turn to carry on.");
      return;
    }
    timer = setTimeout(tick, 0);
    $("turn").textContent = "Pause";
    $("one").disabled = true;
    setMessage("Turning cards. Press Pause whenever you like.");
  }

  function oneCard() {
    if (game.over || timer !== null) return;
    turnOne();
    if (game.over) finishUp();
  }

  function startDeal(seed, nextMode, text) {
    stopAuto();
    game = L.deal(seed);
    mode = nextMode;
    setMessage(text + " Press Turn to begin at the King pile in the centre.");
    render();
    save();
    $("turn").focus();
  }

  function onPause() {
    if (timer !== null) {
      stopAuto();
      if (game) render();
    }
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    reduced = !!init.reducedMotion;
    if (reduced) document.documentElement.classList.add("reduced");
    build();
    $("face").setAttribute("role", "list");

    var r = L.validate(init.restore && init.restore.v === 1 ? init.restore : null);
    if (r) {
      game = L.play(r.seed, r.turned);
      mode = r.mode;
      render();
      if (game.over) setMessage(L.resultText(game) + " Chance decides this one.");
      else setMessage("Welcome back. Press Turn to carry on.");
    } else {
      startDeal(L.dateSeed(new Date()), "daily", "Today's deal.");
    }

    $("turn").addEventListener("click", toggleAuto);
    $("one").addEventListener("click", oneCard);
    $("restart").addEventListener("click", function () {
      startDeal(game.seed, mode, "Same deal, dealt again.");
    });
    $("daily").addEventListener("click", function () {
      startDeal(L.dateSeed(new Date()), "daily", "Today's deal.");
    });
    $("fresh").addEventListener("click", function () {
      startDeal(K.randomSeed(), "random", "A new deal.");
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && timer !== null) {
        e.preventDefault();
        toggleAuto();
      }
    });
    document.addEventListener("visibilitychange", function () {
      if (document.hidden) onPause();
    });
    if (!game.over) $("turn").focus();
  }

  LullshiftBridge.start({ onInit: setup, saveState: snapshot, onPause: onPause });
})();
