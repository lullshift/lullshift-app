// Firefly Catch: UI, canvas drawing and wiring. The rules live in logic.js.
(function () {
  "use strict";
  var A = globalThis.LullshiftKits.arcade;
  var L = globalThis.FireflyLogic;
  var state = L.create(A.randomSeed(), false);
  var held = { left: false, right: false };
  var lastSummary = "";
  var lastStatus = "";
  var canvas = document.getElementById("field");
  var ctx = canvas.getContext("2d");
  var session = null;
  var reduced = false;

  function $(id) {
    return document.getElementById(id);
  }

  function dir() {
    return (held.right ? 1 : 0) - (held.left ? 1 : 0);
  }

  function say(text) {
    $("message").textContent = text;
  }

  function secondsLeft() {
    return Math.ceil(state.timeLeft);
  }

  function updateText(force) {
    var status =
      "Caught " + state.caught + ". Best " + state.best + ". Time left " + secondsLeft() + ".";
    if (status !== lastStatus) {
      lastStatus = status;
      $("status").textContent = status;
    }
    var key = state.caught + "|" + state.best + "|" + secondsLeft();
    if (force || key !== lastSummary) {
      lastSummary = key;
      LullshiftBridge.summary([
        { label: "Caught", value: state.caught },
        { label: "Best", value: state.best },
        { label: "Time left", value: secondsLeft() + " s" },
      ]);
    }
    $("again").textContent = state.over ? "Play again" : "New round";
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
          if (Number.isFinite(n) && n > state.best && n <= 100000) {
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
      state.caught +
        (state.caught === 1 ? " firefly caught." : " fireflies caught.") +
        " Press Play again for another round.",
    );
    saveBest();
    updateText(true);
    if (session) session.saveNow();
  }

  function update(dt) {
    var wasOver = state.over;
    var caught = L.step(state, dt, dir());
    if (caught > 0 && !state.over) {
      if (Math.floor(state.caught / 10) > Math.floor((state.caught - caught) / 10)) {
        say(state.caught + " fireflies caught.");
      }
    }
    if (state.over && !wasOver) endRound();
    updateText(false);
  }

  // ---- drawing -----------------------------------------------------------------------------
  function drawBackground() {
    var g = ctx.createLinearGradient(0, 0, 0, L.H);
    g.addColorStop(0, "#1d2140");
    g.addColorStop(0.6, "#4a3a6b");
    g.addColorStop(1, "#c07a6a");
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, L.W, L.H);
    ctx.fillStyle = "rgba(255,255,255,0.7)";
    for (var i = 0; i < 18; i++) {
      var sx = (i * 67 + 23) % L.W;
      var sy = (i * 41 + 11) % 110;
      ctx.fillRect(sx, sy, 2, 2);
    }
    ctx.fillStyle = "#2a2a4a";
    ctx.beginPath();
    ctx.moveTo(0, L.H);
    ctx.lineTo(0, 235);
    ctx.quadraticCurveTo(100, 205, 200, 240);
    ctx.quadraticCurveTo(300, 270, L.W, 225);
    ctx.lineTo(L.W, L.H);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = "#1a2b34";
    ctx.fillRect(0, L.H - 14, L.W, 14);
  }

  function drawFly(f) {
    var pulse = reduced ? 1 : 0.75 + 0.25 * Math.sin(state.time * 4 + f.phase);
    var alpha = f.away ? 0.45 : 1;
    ctx.globalAlpha = alpha;
    var glow = ctx.createRadialGradient(f.x, f.y, 0, f.x, f.y, 14);
    glow.addColorStop(0, "rgba(255,240,150," + (0.9 * pulse).toFixed(2) + ")");
    glow.addColorStop(1, "rgba(255,240,150,0)");
    ctx.fillStyle = glow;
    ctx.fillRect(f.x - 14, f.y - 14, 28, 28);
    ctx.fillStyle = "#fff6b0";
    ctx.beginPath();
    ctx.arc(f.x, f.y, 3, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  function drawJar() {
    var x = state.jarX - L.JAR_W / 2;
    var y = L.JAR_TOP;
    ctx.fillStyle = "rgba(200,225,240,0.22)";
    ctx.strokeStyle = "rgba(220,240,255,0.9)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + 4, y + L.JAR_H - 6);
    ctx.quadraticCurveTo(x + 6, y + L.JAR_H, x + 14, y + L.JAR_H);
    ctx.lineTo(x + L.JAR_W - 14, y + L.JAR_H);
    ctx.quadraticCurveTo(x + L.JAR_W - 6, y + L.JAR_H, x + L.JAR_W - 4, y + L.JAR_H - 6);
    ctx.lineTo(x + L.JAR_W, y);
    ctx.stroke();
    ctx.fill();
    ctx.fillStyle = "rgba(255,246,176,0.55)";
    var n = Math.min(state.caught, 12);
    for (var i = 0; i < n; i++) {
      ctx.fillRect(x + 10 + ((i * 11) % (L.JAR_W - 20)), y + 14 + ((i * 7) % 18), 2, 2);
    }
  }

  function render() {
    drawBackground();
    for (var i = 0; i < state.flies.length; i++) drawFly(state.flies[i]);
    drawJar();
    if (state.over) {
      ctx.fillStyle = "rgba(20,20,40,0.6)";
      ctx.fillRect(0, 100, L.W, 80);
      ctx.fillStyle = "#fff6b0";
      ctx.font = "600 24px system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(
        state.caught + (state.caught === 1 ? " firefly caught" : " fireflies caught"),
        L.W / 2,
        148,
      );
    }
  }

  // ---- input -------------------------------------------------------------------------------
  function keyDir(e) {
    var k = e.code;
    if (k === "ArrowLeft" || k === "KeyA") return "left";
    if (k === "ArrowRight" || k === "KeyD") return "right";
    return null;
  }

  document.addEventListener("keydown", function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    var d = keyDir(e);
    if (d) {
      held[d] = true;
      e.preventDefault();
      return;
    }
    if ((e.key === "Enter" || e.key === " ") && session && session.confirm()) {
      e.preventDefault();
    } else if (e.key === "Escape" && session) {
      session.pause();
    }
  });
  document.addEventListener("keyup", function (e) {
    var d = keyDir(e);
    if (d) held[d] = false;
  });
  window.addEventListener("blur", clearHeld);
  function clearHeld() {
    held.left = false;
    held.right = false;
  }

  function holdButton(id, side) {
    var b = $(id);
    function on(e) {
      held[side] = true;
      if (e.preventDefault) e.preventDefault();
    }
    function off() {
      held[side] = false;
    }
    b.addEventListener("pointerdown", on);
    b.addEventListener("pointerup", off);
    b.addEventListener("pointercancel", off);
    b.addEventListener("pointerleave", off);
    // Keyboard users move the jar with the arrow keys; Enter on the button nudges it a little.
    b.addEventListener("click", function (e) {
      if (e.detail === 0) {
        state.jarX = Math.min(
          L.W - L.JAR_W / 2,
          Math.max(L.JAR_W / 2, state.jarX + (side === "left" ? -30 : 30)),
        );
        render();
      }
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
    var restored = L.validate(info.restore);
    state = restored || L.create(A.randomSeed(), reduced);
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
    onPausedChange: function (paused) {
      if (paused) clearHeld();
    },
  });

  holdButton("left", "left");
  holdButton("right", "right");
  $("again").addEventListener("click", newRound);
  canvas.focus();
})();
