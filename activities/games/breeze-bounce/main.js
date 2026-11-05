// Breeze Bounce: UI, canvas drawing and wiring. The rules live in logic.js.
(function () {
  "use strict";
  var A = globalThis.LullshiftKits.arcade;
  var L = globalThis.BreezeBounceLogic;
  var state = L.create(A.randomSeed(), false, 0);
  var held = { left: false, right: false };
  var puffs = []; // cosmetic only; advanced in update so a pause freezes it
  var lastStatus = "";
  var lastSummary = "";
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

  function bestText() {
    return state.best > 0 ? L.formatTime(state.best) : "none yet";
  }

  function updateText(force) {
    var total = state.dands.length;
    var status =
      "Released " +
      L.released(state) +
      " of " +
      total +
      ". Drifts " +
      state.drifts +
      ". Time " +
      L.formatTime(state.ticks) +
      ". Best " +
      bestText() +
      ".";
    if (status !== lastStatus) {
      lastStatus = status;
      $("status").textContent = status;
    }
    var key = L.remaining(state) + "|" + Math.floor(state.ticks / 60) + "|" + state.drifts;
    if (force || key !== lastSummary) {
      lastSummary = key;
      LullshiftBridge.summary([
        { label: "Left", value: L.remaining(state) },
        { label: "Time", value: L.formatTime(state.ticks) },
        { label: "Drifts", value: state.drifts },
        { label: "Best", value: state.best > 0 ? L.formatTime(state.best) : "-" },
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
          if (Number.isFinite(n) && n > 0 && n <= 60 * 60 * 24 * 7) {
            if (state.best === 0 || n < state.best) {
              state.best = n;
              updateText(true);
            }
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
      "All dandelions released in " +
        L.formatTime(state.ticks) +
        ". " +
        (state.newBest ? "That is your best time." : "Best " + bestText() + ".") +
        " Press Play again for another round.",
    );
    saveBest();
    updateText(true);
    if (session) session.saveNow();
  }

  function addPuffs(count) {
    if (reduced) return;
    // The most recently released dandelions are the ones nearest the seed.
    var b = state.ball;
    for (var i = 0; i < 5 * count && puffs.length < 60; i++) {
      var a = (i / 5) * Math.PI * 2 + state.ticks;
      puffs.push({ x: b.x, y: b.y, vx: Math.cos(a) * 24, vy: Math.sin(a) * 24 - 10, life: 40 });
    }
  }

  function update() {
    var wasOver = state.over;
    var driftsBefore = state.drifts;
    var got = L.step(state, dir());
    if (got > 0) addPuffs(got);
    for (var i = puffs.length - 1; i >= 0; i--) {
      var p = puffs[i];
      p.x += p.vx * L.STEP;
      p.y += p.vy * L.STEP;
      p.life--;
      if (p.life <= 0) puffs.splice(i, 1);
    }
    if (state.drifts !== driftsBefore) say("A new seed drifts in. Drifts " + state.drifts + ".");
    if (state.over && !wasOver) endRound();
    updateText(false);
  }

  // ---- drawing -----------------------------------------------------------------------------
  function drawBackground() {
    var g = ctx.createLinearGradient(0, 0, 0, L.H);
    g.addColorStop(0, "#bfe3ef");
    g.addColorStop(0.7, "#e4f2e4");
    g.addColorStop(1, "#b9d9a4");
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, L.W, L.H);
    ctx.fillStyle = "#9ec98a";
    ctx.beginPath();
    ctx.moveTo(0, L.H);
    ctx.lineTo(0, 280);
    ctx.quadraticCurveTo(120, 262, 220, 282);
    ctx.quadraticCurveTo(320, 296, L.W, 272);
    ctx.lineTo(L.W, L.H);
    ctx.closePath();
    ctx.fill();
  }

  function drawDandelion(d) {
    if (d.released) {
      // Released clocks leave only a small stem.
      ctx.strokeStyle = "rgba(110,150,90,0.5)";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(d.x, d.y + 4);
      ctx.lineTo(d.x, d.y + 14);
      ctx.stroke();
      return;
    }
    ctx.strokeStyle = "rgba(110,150,90,0.9)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(d.x, d.y + 4);
    ctx.lineTo(d.x, d.y + 16);
    ctx.stroke();
    ctx.strokeStyle = "rgba(255,255,255,0.95)";
    ctx.lineWidth = 1;
    for (var i = 0; i < 10; i++) {
      var a = (i / 10) * Math.PI * 2;
      ctx.beginPath();
      ctx.moveTo(d.x, d.y);
      ctx.lineTo(d.x + Math.cos(a) * 11, d.y + Math.sin(a) * 11);
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      ctx.beginPath();
      ctx.arc(d.x + Math.cos(a) * 11, d.y + Math.sin(a) * 11, 1.8, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.fillStyle = "#c9b86a";
    ctx.beginPath();
    ctx.arc(d.x, d.y, 2.5, 0, Math.PI * 2);
    ctx.fill();
  }

  function drawSeed() {
    var b = state.ball;
    var waiting = b.wait > 0;
    ctx.globalAlpha = waiting ? 0.6 : 1;
    ctx.strokeStyle = "rgba(255,255,255,0.9)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(b.x, b.y - L.BALL_R);
    ctx.lineTo(b.x, b.y - L.BALL_R - 7);
    ctx.stroke();
    ctx.fillStyle = "#8a6a3a";
    ctx.beginPath();
    ctx.arc(b.x, b.y, L.BALL_R, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  function drawLeaf() {
    var x = state.paddleX - L.PADDLE_W / 2;
    var y = L.PADDLE_Y;
    ctx.fillStyle = "#4f9a5a";
    ctx.beginPath();
    ctx.moveTo(x, y + L.PADDLE_H / 2);
    ctx.quadraticCurveTo(state.paddleX, y - 8, x + L.PADDLE_W, y + L.PADDLE_H / 2);
    ctx.quadraticCurveTo(state.paddleX, y + L.PADDLE_H + 6, x, y + L.PADDLE_H / 2);
    ctx.fill();
  }

  function render() {
    drawBackground();
    for (var i = 0; i < state.dands.length; i++) drawDandelion(state.dands[i]);
    for (var j = 0; j < puffs.length; j++) {
      ctx.globalAlpha = Math.max(0, puffs[j].life / 40);
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(puffs[j].x, puffs[j].y, 2, 2);
    }
    ctx.globalAlpha = 1;
    drawSeed();
    drawLeaf();
    if (state.over) {
      ctx.fillStyle = "rgba(40,70,50,0.55)";
      ctx.fillRect(0, 110, L.W, 80);
      ctx.fillStyle = "#ffffff";
      ctx.font = "600 22px system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText("All released in " + L.formatTime(state.ticks), L.W / 2, 158);
    }
  }

  // ---- input -------------------------------------------------------------------------------
  function keyDir(e) {
    if (e.code === "ArrowLeft" || e.code === "KeyA") return "left";
    if (e.code === "ArrowRight" || e.code === "KeyD") return "right";
    return null;
  }

  function clearHeld() {
    held.left = false;
    held.right = false;
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
    // Keyboard users move the leaf with the arrow keys; Enter on the button nudges it a little.
    b.addEventListener("click", function (e) {
      if (e.detail === 0) {
        var half = L.PADDLE_W / 2;
        state.paddleX = Math.min(
          L.W - half,
          Math.max(half, state.paddleX + (side === "left" ? -30 : 30)),
        );
        render();
      }
    });
  }

  function newRound() {
    state = L.restart(state, A.randomSeed());
    puffs = [];
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
    state = restored || L.create(A.randomSeed(), reduced, 0);
    if (!restored) puffs = [];
    state.reduced = reduced || state.reduced;
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
