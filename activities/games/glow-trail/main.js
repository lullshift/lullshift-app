// Glow Trail: UI, canvas drawing and wiring. The rules live in logic.js.
(function () {
  "use strict";
  var A = globalThis.LullshiftKits.arcade;
  var L = globalThis.GlowTrailLogic;
  var state = L.create(A.randomSeed(), false, 0);
  var lastStatus = "";
  var lastSummary = "";
  var canvas = document.getElementById("field");
  var ctx = canvas.getContext("2d");
  var session = null;
  var reduced = false;
  var DIRS = { up: 0, right: 1, down: 2, left: 3 };

  function $(id) {
    return document.getElementById(id);
  }

  function say(text) {
    $("message").textContent = text;
  }

  function bestText() {
    return state.best > 0 ? String(state.best) : "none yet";
  }

  function updateText(force) {
    var left = L.secondsLeft(state);
    var status =
      "Trail " +
      state.trail.length +
      ". Longest " +
      state.longest +
      ". Best " +
      bestText() +
      ". Time left " +
      left +
      ".";
    if (status !== lastStatus) {
      lastStatus = status;
      $("status").textContent = status;
    }
    var key = state.longest + "|" + state.best + "|" + left;
    if (force || key !== lastSummary) {
      lastSummary = key;
      LullshiftBridge.summary([
        { label: "Longest", value: state.longest },
        { label: "Best", value: state.best > 0 ? state.best : "-" },
        { label: "Time left", value: left + " s" },
      ]);
    }
    $("again").textContent = state.over ? "Play again" : "New round";
    $("slower").setAttribute("aria-pressed", state.slower ? "true" : "false");
  }

  function saveBest() {
    try {
      LullshiftBridge.storage.set("best", state.best).then(
        function () {},
        function () {},
      );
    } catch (e) {
      /* storage unavailable: the best still lives in the saved state */
    }
  }

  function loadBest() {
    try {
      LullshiftBridge.storage.get("best").then(
        function (v) {
          var n = parseInt(v, 10);
          if (Number.isFinite(n) && n > state.best && n <= 120) {
            state.best = n;
            updateText(true);
          }
        },
        function () {},
      );
    } catch (e) {
      /* ignore */
    }
  }

  function endRound() {
    say(
      "Round complete. Longest trail " +
        state.longest +
        ". " +
        (state.newBest ? "That is your best." : "Best " + bestText() + ".") +
        " Press Play again for another round.",
    );
    saveBest();
    updateText(true);
    if (session) session.saveNow();
  }

  function update() {
    var wasOver = state.over;
    var before = state.longest;
    var ev = L.step(state);
    if (ev & L.EASED) say("The trail eases back to its start. Carry on.");
    else if (ev & L.COLLECTED && state.longest > before && state.longest % 5 === 0) {
      say("Trail " + state.longest + ".");
    }
    if (state.over && !wasOver) endRound();
    updateText(false);
  }

  // ---- drawing -----------------------------------------------------------------------------
  function cellRect(c) {
    return [c[0] * L.CELL, c[1] * L.CELL];
  }

  function drawBackground() {
    ctx.fillStyle = "#13213a";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = "#1d3050";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (var x = 1; x < L.COLS; x++) {
      ctx.moveTo(x * L.CELL + 0.5, 0);
      ctx.lineTo(x * L.CELL + 0.5, canvas.height);
    }
    for (var y = 1; y < L.ROWS; y++) {
      ctx.moveTo(0, y * L.CELL + 0.5);
      ctx.lineTo(canvas.width, y * L.CELL + 0.5);
    }
    ctx.stroke();
  }

  function drawDew(d) {
    var p = cellRect(d);
    var cx = p[0] + L.CELL / 2;
    var cy = p[1] + L.CELL / 2;
    ctx.fillStyle = "#8fd3ff";
    ctx.beginPath();
    ctx.moveTo(cx, cy - 7);
    ctx.quadraticCurveTo(cx + 7, cy + 1, cx, cy + 7);
    ctx.quadraticCurveTo(cx - 7, cy + 1, cx, cy - 7);
    ctx.fill();
    ctx.fillStyle = "rgba(255,255,255,0.8)";
    ctx.fillRect(cx - 2, cy - 1, 2, 3);
  }

  function render() {
    drawBackground();
    for (var j = 0; j < state.dew.length; j++) drawDew(state.dew[j]);
    var n = state.trail.length;
    for (var i = n - 1; i >= 0; i--) {
      var p = cellRect(state.trail[i]);
      var f = 1 - i / (n + 1);
      ctx.fillStyle = "rgba(255,240,160," + (0.25 + 0.7 * f).toFixed(2) + ")";
      ctx.beginPath();
      ctx.arc(p[0] + L.CELL / 2, p[1] + L.CELL / 2, 4 + 4 * f, 0, Math.PI * 2);
      ctx.fill();
    }
    var h = cellRect(state.trail[0]);
    var hx = h[0] + L.CELL / 2;
    var hy = h[1] + L.CELL / 2;
    var pulse = reduced ? 1 : 0.8 + 0.2 * Math.sin(state.ticks / 8);
    var glow = ctx.createRadialGradient(hx, hy, 0, hx, hy, 16);
    glow.addColorStop(0, "rgba(255,240,150," + (0.9 * pulse).toFixed(2) + ")");
    glow.addColorStop(1, "rgba(255,240,150,0)");
    ctx.fillStyle = glow;
    ctx.fillRect(hx - 16, hy - 16, 32, 32);
    ctx.fillStyle = "#fffbe0";
    ctx.beginPath();
    ctx.arc(hx, hy, 5, 0, Math.PI * 2);
    ctx.fill();
    if (state.over) {
      ctx.fillStyle = "rgba(10,20,40,0.65)";
      ctx.fillRect(0, 110, canvas.width, 80);
      ctx.fillStyle = "#fff6b0";
      ctx.font = "600 22px system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText("Longest trail " + state.longest, canvas.width / 2, 158);
    }
  }

  // ---- input -------------------------------------------------------------------------------
  function keyDir(e) {
    switch (e.code) {
      case "ArrowUp":
      case "KeyW":
        return DIRS.up;
      case "ArrowRight":
      case "KeyD":
        return DIRS.right;
      case "ArrowDown":
      case "KeyS":
        return DIRS.down;
      case "ArrowLeft":
      case "KeyA":
        return DIRS.left;
      default:
        return null;
    }
  }

  function steer(d) {
    if (session && !session.paused) L.steer(state, d);
  }

  document.addEventListener("keydown", function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    var d = keyDir(e);
    if (d !== null) {
      steer(d);
      e.preventDefault();
      return;
    }
    if ((e.key === "Enter" || e.key === " ") && session && session.confirm()) {
      e.preventDefault();
    } else if (e.key === "Escape" && session) {
      session.pause();
    }
  });

  function padButton(id, d) {
    $(id).addEventListener("click", function () {
      steer(d);
    });
  }

  function newRound() {
    state = L.restart(state, A.randomSeed());
    lastStatus = "";
    say(state.best > 0 ? "New round." : "");
    updateText(true);
    render();
    if (session) session.saveNow();
  }

  function setup(info) {
    reduced = !!info.reducedMotion;
    document.documentElement.setAttribute("data-theme", info.theme === "dark" ? "dark" : "light");
    document.documentElement.classList.toggle("reduced", reduced);
    state = L.validate(info.restore) || L.create(A.randomSeed(), false, 0);
    loadBest();
    updateText(true);
    render();
  }

  session = A.runArcade({
    bridge: LullshiftBridge,
    onInit: setup,
    update: update,
    render: render,
    save: function () {
      return L.snapshot(state);
    },
    prompt: function (text) {
      $("prompt").textContent = text;
    },
  });

  padButton("up", DIRS.up);
  padButton("right", DIRS.right);
  padButton("down", DIRS.down);
  padButton("left", DIRS.left);
  $("slower").addEventListener("click", function () {
    L.setSlower(state, !state.slower);
    say(state.slower ? "Slower pace." : "Normal pace.");
    updateText(true);
    if (session) session.saveNow();
  });
  $("again").addEventListener("click", newRound);
  canvas.focus();
})();
