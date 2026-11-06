(function () {
  "use strict";
  var L = globalThis.FogLogic;
  var K = window.LullshiftKits.grid;
  var size = 9;
  var grid = null;
  var view = null;
  var game = null;
  var markMode = false;
  var BRAMBLE_MARK = String.fromCharCode(0x2736);
  var MARKED_MARK = String.fromCharCode(0x273f);
  var DASH = String.fromCharCode(0x2014);
  var HIT_TEXT =
    "A bramble " + DASH + " the fog closes in. Try again on this field, or start a new one.";
  var WON_TEXT = "Every safe tile is clear. The garden is yours.";

  function $(id) {
    return document.getElementById(id);
  }

  function setMessage(text) {
    $("message").textContent = text;
  }

  function snapshot() {
    return game
      ? {
          v: 1,
          seed: game.seed,
          size: size,
          brambles: game.brambles,
          first: game.first,
          revealed: game.revealed,
          marks: game.marks,
          status: game.status,
          markMode: markMode,
          cursor: { x: grid.cursor.x, y: grid.cursor.y },
        }
      : undefined;
  }

  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  function indexOf(x, y) {
    return y * size + x;
  }

  function nextSize() {
    var sizes = L.SIZES;
    for (var i = 0; i < sizes.length; i++) {
      if (sizes[i].size === size) return sizes[(i + 1) % sizes.length];
    }
    return sizes[0];
  }

  function centre() {
    return Math.floor(size / 2);
  }

  function tileKind(i) {
    if (game.revealed[i]) return game.brambles && game.brambles[i] ? "bramble" : "clear";
    if (game.status === "hit" && game.brambles && game.brambles[i]) return "bramble";
    if (game.marks[i]) return "marked";
    return "fog";
  }

  function label(_cell, x, y) {
    var i = indexOf(x, y);
    var where = "Row " + (y + 1) + ", column " + (x + 1) + ", ";
    var kind = tileKind(i);
    if (kind === "bramble") return where + "bramble";
    if (kind === "marked") return where + "marked as a bramble";
    if (kind === "fog") return where + "hidden in fog";
    var n = L.countAround(game.brambles, i, size);
    if (n === 0) return where + "clear, no brambles nearby";
    return where + "clear, " + n + (n === 1 ? " bramble" : " brambles") + " nearby";
  }

  function text(_cell, x, y) {
    var i = indexOf(x, y);
    var kind = tileKind(i);
    if (kind === "bramble") return BRAMBLE_MARK;
    if (kind === "marked") return MARKED_MARK;
    if (kind === "clear") {
      var n = L.countAround(game.brambles, i, size);
      return n === 0 ? "" : String(n);
    }
    return "";
  }

  function classes(_cell, x, y) {
    var i = indexOf(x, y);
    var kind = tileKind(i);
    if (kind === "clear") return "clear n" + L.countAround(game.brambles, i, size);
    return kind;
  }

  function isZero(v) {
    return v === 0;
  }

  function render() {
    view.render();
    var left = L.bramblesLeft(game);
    var shown = L.revealedCount(game);
    $("left").textContent = "Brambles left: " + left;
    $("shown").textContent = "Revealed " + shown + " of " + L.safeTotal(size);
    var next = nextSize();
    $("size").textContent = "Size: " + size + " x " + size;
    $("size").setAttribute(
      "aria-label",
      "Size: " +
        size +
        " by " +
        size +
        ", " +
        L.bramblesFor(size) +
        " brambles. Press to switch to " +
        next.size +
        " by " +
        next.size,
    );
    var mark = $("mark");
    mark.setAttribute("aria-pressed", markMode ? "true" : "false");
    mark.disabled = game.status !== "playing";
    $("again").disabled = game.brambles === null && game.marks.every(isZero);
    LullshiftBridge.summary([
      { label: "Brambles left", value: left },
      { label: "Revealed", value: shown + " of " + L.safeTotal(size) },
    ]);
  }

  function reveal(x, y) {
    if (game.status !== "playing") return;
    var i = indexOf(x, y);
    if (game.marks[i]) {
      setMessage("That tile is marked. Unmark it first to reveal it.");
      return;
    }
    if (game.revealed[i]) return;
    game = L.reveal(game, i).game;
    if (game.status === "hit") {
      setMessage(HIT_TEXT);
      render();
      $("again").focus();
    } else if (game.status === "won") {
      setMessage(WON_TEXT);
      render();
    } else {
      setMessage("Revealed " + L.revealedCount(game) + " of " + L.safeTotal(size) + ".");
      render();
    }
    save();
  }

  function mark(x, y) {
    if (game.status !== "playing") return;
    var i = indexOf(x, y);
    if (game.revealed[i]) {
      setMessage("A cleared tile cannot be marked.");
      return;
    }
    game = L.toggleMark(game, i).game;
    setMessage(game.marks[i] ? "Marked. " + L.bramblesLeft(game) + " left." : "Mark removed.");
    render();
    save();
  }

  function activate(x, y) {
    if (markMode) mark(x, y);
    else reveal(x, y);
  }

  // (Re)builds the grid model and its view for a field size. The cursor starts in the middle.
  function buildBoard(sz) {
    size = sz;
    if (view) view.destroy();
    grid = new K.Grid(size, size, 0);
    grid.setCursor(centre(), centre());
    $("grid").setAttribute("data-size", String(size));
    $("grid").style.setProperty("--n", String(size));
    view = K.bindGrid($("grid"), grid, {
      name: "Fog Garden field, " + size + " by " + size,
      label: label,
      text: text,
      className: classes,
      onActivate: activate,
      onMove: save,
    });
  }

  function dailySeed() {
    return K.dailySeed(new Date(), size === 9 ? "fog-garden" : "fog-garden-" + size);
  }

  function setup(init) {
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (init.reducedMotion) document.documentElement.classList.add("reduced");

    var r = init.restore;
    var restored = r && r.v === 1 ? L.validate(r) : null;
    game = restored || L.newGame(K.randomSeed(), 9);
    buildBoard(game.size);
    if (restored) {
      markMode = r.markMode === true;
      var c = r.cursor;
      if (c && Number.isInteger(c.x) && Number.isInteger(c.y)) grid.setCursor(c.x, c.y);
    }

    $("grid").addEventListener("keydown", function (e) {
      var k = e.key.toLowerCase();
      if ((k === "f" || k === "m") && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        mark(grid.cursor.x, grid.cursor.y);
      }
    });
    $("grid").addEventListener("contextmenu", function (e) {
      e.preventDefault();
      mark(grid.cursor.x, grid.cursor.y);
    });
    $("mark").addEventListener("click", function () {
      markMode = !markMode;
      setMessage(markMode ? "Mark mode on. Activating a tile marks it." : "Mark mode off.");
      render();
      save();
    });
    $("again").addEventListener("click", function () {
      game = L.tryAgain(game);
      setMessage("Same field, fog restored.");
      render();
      save();
      view.focus();
    });
    $("fresh").addEventListener("click", function () {
      game = L.newGame(K.randomSeed(), size);
      grid.setCursor(centre(), centre());
      setMessage("A new field. Reveal a tile to begin.");
      render();
      save();
      view.focus();
    });

    $("daily").addEventListener("click", function () {
      game = L.dailyGame(dailySeed(), size);
      grid.setCursor(centre(), centre());
      setMessage("Today's field. The middle is already clear.");
      render();
      save();
      view.focus();
    });

    $("size").addEventListener("click", function () {
      var n = nextSize();
      buildBoard(n.size);
      game = L.newGame(K.randomSeed(), n.size);
      setMessage(
        "Now " +
          n.size +
          " by " +
          n.size +
          " with " +
          n.brambles +
          " brambles. Reveal a tile to begin.",
      );
      render();
      save();
    });

    if (restored && game.status === "hit") setMessage(HIT_TEXT);
    else if (restored && game.status === "won") setMessage(WON_TEXT);
    else if (!restored) setMessage("Reveal a tile to begin. The first one is always safe.");
    render();
    if (!restored) save();
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
  });
})();
