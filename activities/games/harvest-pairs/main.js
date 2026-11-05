(function () {
  "use strict";
  var L = globalThis.HarvestLogic;
  var K = window.LullshiftKits.card;
  var DRAW = 29; // the draw pile button, one past the waste entry
  var NS = "http://www.w3.org/2000/svg";

  var st = null; // the game: { seed, attempt, verified, pyr, stock, waste, log }
  var mode = "daily";
  var sel = -1; // picked-up entry, or -1
  var cur = DRAW; // keyboard cursor entry
  var buttons = [];
  var reduced = false;

  function $(id) {
    return document.getElementById(id);
  }

  // ---- Suit art: leaves, acorns, berries, shells ------------------------------------------------

  function el(tag, attrs, cls) {
    var n = document.createElementNS(NS, tag);
    for (var k in attrs) n.setAttribute(k, attrs[k]);
    if (cls) n.setAttribute("class", cls);
    return n;
  }

  function suitIcon(s) {
    var svg = el(
      "svg",
      { viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false" },
      "icon",
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

  // ---- Buttons ------------------------------------------------------------------------------------

  function makeButton(entry) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "card";
    b.dataset.entry = String(entry);
    var top = document.createElement("span");
    top.className = "top";
    var rank = document.createElement("span");
    rank.className = "rank";
    var word = document.createElement("span");
    word.className = "word";
    top.appendChild(rank);
    top.appendChild(word);
    var holder = document.createElement("span");
    holder.className = "art";
    b.appendChild(top);
    b.appendChild(holder);
    b.addEventListener("click", function () {
      cur = entry;
      press(entry);
    });
    b._base = "card";
    buttons[entry] = b;
    return b;
  }

  function build() {
    var field = $("field");
    for (var s = 0; s < L.SLOTS; s++) {
      var b = makeButton(s);
      b._base = "card row-" + (L.ROW_OF[s] + 1) + " col-" + (2 * L.COL_OF[s] - L.ROW_OF[s] + 7);
      field.appendChild(b);
    }
    var piles = $("piles");
    piles.appendChild(makeButton(DRAW));
    piles.appendChild(makeButton(L.WASTE));
    buttons[DRAW]._base = "card draw";
  }

  function paint(b, id, entryClass) {
    var s = L.suit(id);
    b.className = b._base + " suit" + s + (entryClass ? " " + entryClass : "");
    b.children[0].children[0].textContent = L.rankText(id);
    b.children[0].children[1].textContent = L.SUIT_ONE[s];
    if (b._suit !== s) {
      b.children[1].textContent = "";
      b.children[1].appendChild(suitIcon(s));
      b._suit = s;
    }
  }

  function plain(b, text, cls) {
    b.className = b._base + " " + cls;
    b.children[0].children[0].textContent = "";
    b.children[1].textContent = "";
    b._suit = -1;
    b.children[0].children[1].textContent = text;
  }

  // ---- Geometry for keyboard moves -----------------------------------------------------------------

  function rowOf(e) {
    return e < L.SLOTS ? L.ROW_OF[e] : 7;
  }
  function xOf(e) {
    if (e < L.SLOTS) return 2 * L.COL_OF[e] - L.ROW_OF[e];
    return e === DRAW ? -1 : 1;
  }

  function isFree(e) {
    if (e === DRAW) return true;
    if (e === L.WASTE) return st.waste.length > 0;
    return L.isUncovered(st.pyr, e);
  }

  function freeEntries() {
    var out = [];
    for (var e = 0; e <= DRAW; e++) if (isFree(e)) out.push(e);
    return out;
  }

  function nearest(from, list) {
    var best = -1;
    var score = Infinity;
    for (var i = 0; i < list.length; i++) {
      var d = Math.abs(rowOf(list[i]) - rowOf(from)) * 100 + Math.abs(xOf(list[i]) - xOf(from));
      if (d < score) {
        score = d;
        best = list[i];
      }
    }
    return best;
  }

  function step(action) {
    var list = freeEntries();
    var here = cur;
    var row = rowOf(here);
    var inRow = list
      .filter(function (e) {
        return rowOf(e) === row;
      })
      .sort(function (a, b) {
        return xOf(a) - xOf(b);
      });
    var i = inRow.indexOf(here);
    if (action === "left") return inRow[Math.max(0, i - 1)];
    if (action === "right") return inRow[Math.min(inRow.length - 1, i + 1)];
    if (action === "home") return inRow[0];
    if (action === "end") return inRow[inRow.length - 1];
    if (action === "first") return list[0];
    if (action === "last") return DRAW;
    var dir = action === "up" ? -1 : 1;
    for (var r = row + dir; r >= 0 && r <= 7; r += dir) {
      var rowList = list.filter(function (e) {
        return rowOf(e) === r;
      });
      if (rowList.length) {
        var best = rowList[0];
        for (var j = 1; j < rowList.length; j++) {
          if (Math.abs(xOf(rowList[j]) - xOf(here)) < Math.abs(xOf(best) - xOf(here))) {
            best = rowList[j];
          }
        }
        return best;
      }
    }
    return here;
  }

  // ---- Labels and rendering ------------------------------------------------------------------------

  function setMessage(text) {
    $("message").textContent = text;
  }

  function entryLabel(e) {
    var id = L.cardAt(st, e);
    var name = L.cardName(id);
    if (L.rank(id) === 13) name += ", a King, can go alone";
    return name;
  }

  function render() {
    var cleared = L.cleared(st);
    // Keep the cursor on something that still exists.
    if (!isFree(cur)) cur = nearest(cur, freeEntries());
    if (sel >= 0 && L.cardAt(st, sel) < 0) sel = -1;
    var e;
    for (e = 0; e < L.SLOTS; e++) {
      var b = buttons[e];
      var id = st.pyr[e];
      if (id < 0) {
        b.hidden = true;
        b.disabled = true;
        b.tabIndex = -1;
        continue;
      }
      b.hidden = false;
      var free = L.isUncovered(st.pyr, e);
      paint(b, id, free ? "free" : "covered");
      b.disabled = !free;
      b.tabIndex = free && e === cur ? 0 : -1;
      if (free) {
        b.setAttribute("aria-label", "Row " + (L.ROW_OF[e] + 1) + ", " + entryLabel(e));
        b.setAttribute("aria-pressed", sel === e ? "true" : "false");
      } else {
        b.setAttribute(
          "aria-label",
          "Row " + (L.ROW_OF[e] + 1) + ", " + L.cardName(id) + ", covered",
        );
        b.removeAttribute("aria-pressed");
      }
    }
    var w = buttons[L.WASTE];
    if (st.waste.length) {
      paint(w, st.waste[st.waste.length - 1], "free");
      w.disabled = false;
      w.setAttribute(
        "aria-label",
        "Waste pile, " + st.waste.length + " cards, top card " + entryLabel(L.WASTE),
      );
      w.setAttribute("aria-pressed", sel === L.WASTE ? "true" : "false");
    } else {
      plain(w, "Waste", "empty");
      w.disabled = true;
      w.setAttribute("aria-label", "Waste pile, empty");
      w.removeAttribute("aria-pressed");
    }
    w.tabIndex = !w.disabled && cur === L.WASTE ? 0 : -1;
    var d = buttons[DRAW];
    if (st.stock.length) {
      plain(d, "Draw " + st.stock.length, "back");
      d.setAttribute(
        "aria-label",
        "Draw pile, " + st.stock.length + " cards. Press to draw a card.",
      );
    } else if (st.waste.length) {
      plain(d, "Turn over", "back");
      d.setAttribute("aria-label", "Draw pile is empty. Press to turn the waste pile over.");
    } else {
      plain(d, "Empty", "empty");
      d.setAttribute("aria-label", "Draw pile, empty");
    }
    d.removeAttribute("aria-pressed");
    d.disabled = st.stock.length === 0 && st.waste.length === 0;
    d.tabIndex = cur === DRAW && !d.disabled ? 0 : -1;
    if (d.disabled && cur === DRAW) {
      var first = freeEntries().filter(function (x) {
        return x !== DRAW;
      })[0];
      if (first !== undefined) {
        cur = first;
        buttons[cur].tabIndex = 0;
      }
    }

    var left = L.left(st);
    var pairs = st.log.filter(function (m) {
      return m[0] === 2;
    }).length;
    $("left").textContent = "Field: " + left + " of " + L.SLOTS + " left";
    $("stockstat").textContent = "Draw pile: " + st.stock.length;
    $("deal").textContent = mode === "daily" ? "Today's deal" : "Deal " + (st.seed % 100000);
    $("undo").disabled = st.log.length === 0;
    $("board").classList.toggle("done", cleared);
    LullshiftBridge.summary([
      { label: "Field left", value: left + " of " + L.SLOTS },
      { label: "Draw pile", value: st.stock.length },
      { label: "Removals", value: pairs },
    ]);
  }

  function focusCursor() {
    if (buttons[cur] && !buttons[cur].disabled) buttons[cur].focus();
  }

  // ---- Saving --------------------------------------------------------------------------------------

  function snapshot() {
    return st ? { v: 1, game: L.copy(st), mode: mode, sel: sel, cur: cur } : undefined;
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  // ---- Actions -------------------------------------------------------------------------------------

  function press(e) {
    if (L.cleared(st)) return;
    if (e === DRAW) {
      sel = -1;
      if (st.stock.length) {
        L.draw(st);
        setMessage("Drew " + L.cardName(st.waste[st.waste.length - 1]) + ".");
      } else if (st.waste.length) {
        L.turnOver(st);
        setMessage("Turned the waste pile over. " + st.stock.length + " cards to draw.");
      } else {
        setMessage("Nothing to draw.");
      }
      render();
      focusCursor();
      save();
      return;
    }
    var id = L.cardAt(st, e);
    if (id < 0) return;
    if (L.rank(id) === 13) {
      var name = L.cardName(id);
      sel = -1;
      L.remove(st, e, -1);
      render();
      afterRemovalAnnounce(name + " went alone. ");
      focusCursor();
      save();
      return;
    }
    if (sel < 0) {
      sel = e;
      setMessage("Picked up " + L.cardName(id) + ". Needs a " + (13 - L.rank(id)) + ".");
      render();
      focusCursor();
      save();
      return;
    }
    if (sel === e) {
      sel = -1;
      setMessage("Put " + L.cardName(id) + " back.");
      render();
      focusCursor();
      save();
      return;
    }
    var why = L.whyNot(st, sel, e);
    if (why === null) {
      var pair = L.cardName(L.cardAt(st, sel)) + " and " + L.cardName(id);
      L.remove(st, sel, e);
      sel = -1;
      render();
      afterRemovalAnnounce(pair + " paired. ");
    } else {
      sel = e;
      setMessage(why + " Picked up " + L.cardName(id) + " instead.");
      render();
    }
    focusCursor();
    save();
  }

  function afterRemovalAnnounce(prefix) {
    if (L.cleared(st)) {
      setMessage(prefix + "Every field card is cleared. Well done.");
      return;
    }
    var o = L.outlook(st, 20000);
    if (o.result === "dead") {
      setMessage(
        prefix +
          "No way to clear the rest remains from here. Undo takes moves back, or start a new deal. That is fine.",
      );
    } else {
      setMessage(prefix + L.left(st) + " field cards left.");
    }
  }

  function undoMove() {
    if (st.log.length === 0) return;
    sel = -1;
    if (!L.undo(st)) {
      st.log = [];
      setMessage("Could not undo any further.");
    } else {
      setMessage("Undone.");
    }
    render();
    focusCursor();
    save();
  }

  function restart() {
    var d = L.plainDeal(st.seed, st.attempt);
    d.verified = st.verified;
    st = d;
    sel = -1;
    cur = DRAW;
    setMessage("Same deal, dealt again.");
    render();
    save();
  }

  function startDeal(seed, nextMode) {
    $("message").textContent = "Dealing.";
    st = L.newGame(seed);
    mode = nextMode;
    sel = -1;
    cur = DRAW;
    setMessage(
      (nextMode === "daily" ? "Today's deal. " : "A new deal. ") +
        (st.verified
          ? "It can be cleared."
          : "It could not be checked, so it may not be clearable."),
    );
    render();
    save();
  }

  function onKey(e) {
    if (e.altKey) return;
    var action = K.keyAction(e);
    if (action === "undo") {
      e.preventDefault();
      undoMove();
      return;
    }
    if (action === "cancel") {
      if (sel >= 0) {
        e.preventDefault();
        sel = -1;
        setMessage("Put the card back.");
        render();
        focusCursor();
        save();
      }
      return;
    }
    if (e.key === "d" || e.key === "D") {
      if (e.ctrlKey || e.metaKey) return;
      e.preventDefault();
      cur = DRAW;
      press(DRAW);
      return;
    }
    if (
      action === "left" ||
      action === "right" ||
      action === "up" ||
      action === "down" ||
      action === "home" ||
      action === "end" ||
      action === "first" ||
      action === "last"
    ) {
      e.preventDefault();
      var next = step(action);
      if (next !== undefined && next !== cur) {
        cur = next;
        render();
        focusCursor();
      }
    }
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    reduced = !!init.reducedMotion;
    if (reduced) document.documentElement.classList.add("reduced");
    build();

    var r = init.restore;
    var game = r && r.v === 1 ? L.validate(r.game) : null;
    if (game) {
      st = game;
      mode = r.mode === "random" ? "random" : "daily";
      sel = Number.isInteger(r.sel) ? r.sel : -1;
      cur = Number.isInteger(r.cur) && r.cur >= 0 && r.cur <= DRAW ? r.cur : DRAW;
      render();
      setMessage(
        L.cleared(st)
          ? "Every field card is cleared."
          : "Welcome back. Your deal is as you left it.",
      );
    } else {
      startDeal(L.dateSeed(new Date()), "daily");
    }

    $("board").addEventListener("keydown", onKey);
    $("undo").addEventListener("click", undoMove);
    $("restart").addEventListener("click", restart);
    $("daily").addEventListener("click", function () {
      startDeal(L.dateSeed(new Date()), "daily");
    });
    $("fresh").addEventListener("click", function () {
      startDeal(K.randomSeed(), "random");
    });
  }

  LullshiftBridge.start({ onInit: setup, saveState: snapshot });
})();
