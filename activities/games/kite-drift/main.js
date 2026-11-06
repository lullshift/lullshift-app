// Kite Drift: UI, canvas drawing and wiring. The rules live in logic.js.
(function () {
  "use strict";
  var A = globalThis.LullshiftKits.arcade;
  var L = globalThis.KiteLogic;
  var state = L.create(A.randomSeed(), false);
  var keysHeld = { space: false, up: false };
  var pointerHeld = false;
  var lastSummary = "";
  var canvas = document.getElementById("field");
  var ctx = canvas.getContext("2d");
  var session = null;
  var reduced = false;

  function $(id) {
    return document.getElementById(id);
  }

  function holding() {
    return keysHeld.space || keysHeld.up || pointerHeld;
  }

  function say(text) {
    $("message").textContent = text;
  }

  function cap(word) {
    return word.charAt(0).toUpperCase() + word.slice(1);
  }

  function updateText(force) {
    $("status").textContent = "Gaps " + state.gaps + ". Best streak " + state.bestStreak + ".";
    $("pace").textContent = "Pace: " + cap(state.pace);
    $("pace").setAttribute(
      "aria-label",
      "Pace: " + state.pace + ". Press to switch to " + L.nextPace(state.pace),
    );
    var key = state.gaps + "|" + state.bestStreak;
    if (force || key !== lastSummary) {
      lastSummary = key;
      LullshiftBridge.summary([
        { label: "Gaps", value: state.gaps },
        { label: "Best streak", value: state.bestStreak },
      ]);
    }
  }

  function update(dt) {
    var before = state.streak;
    var r = L.step(state, dt, holding());
    if (r.passed > 0) {
      updateText(false);
      if (state.gaps % 10 === 0) say(state.gaps + " gaps passed.");
    }
    if (r.bumped > 0) {
      if (before >= 5) say("Bumped. Your streak was " + before + ". A new one starts now.");
      updateText(false);
    }
  }

  // ---- drawing -----------------------------------------------------------------------------
  function drawSky() {
    var g = ctx.createLinearGradient(0, 0, 0, L.H);
    g.addColorStop(0, "#9cc9f0");
    g.addColorStop(1, "#f6e4d0");
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, L.W, L.H);
    if (reduced) return;
    // Slow parallax puffs far behind the playfield.
    ctx.fillStyle = "rgba(255,255,255,0.35)";
    for (var i = 0; i < 6; i++) {
      var span = L.W + 120;
      var px = ((((i * 97 - state.scroll * 0.25) % span) + span) % span) - 60;
      ctx.beginPath();
      ctx.ellipse(px, 40 + ((i * 53) % 200), 40, 12, 0, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function puff(x, y, r) {
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
  }

  function drawBank(c) {
    var sx = c.x - state.scroll;
    if (sx > L.W + 20 || sx + L.BANK_W < -20) return;
    var top = c.gapY - L.GAP_H / 2;
    var bottom = c.gapY + L.GAP_H / 2;
    ctx.fillStyle = c.done ? "rgba(255,255,255,0.7)" : "#ffffff";
    var cx = sx + L.BANK_W / 2;
    for (var y = top; y > -20; y -= 26) puff(cx, y - 8, 26);
    for (var y2 = bottom; y2 < L.H + 20; y2 += 26) puff(cx, y2 + 8, 26);
  }

  function drawKite() {
    var x = L.KITE_X;
    var y = state.y;
    var tilt = reduced ? 0 : state.vy / 400;
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(tilt);
    ctx.fillStyle = state.bump > 0 ? "#f4a39a" : "#e8604c";
    ctx.beginPath();
    ctx.moveTo(0, -14);
    ctx.lineTo(11, 0);
    ctx.lineTo(0, 14);
    ctx.lineTo(-11, 0);
    ctx.closePath();
    ctx.fill();
    ctx.strokeStyle = "#7a2c20";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(0, -14);
    ctx.lineTo(0, 14);
    ctx.moveTo(-11, 0);
    ctx.lineTo(11, 0);
    ctx.stroke();
    ctx.strokeStyle = "#7a2c20";
    ctx.beginPath();
    ctx.moveTo(0, 14);
    ctx.quadraticCurveTo(-10, 24, -20, 22);
    ctx.quadraticCurveTo(-30, 20, -38, 28);
    ctx.stroke();
    ctx.restore();
  }

  function render() {
    drawSky();
    for (var i = 0; i < state.clouds.length; i++) drawBank(state.clouds[i]);
    drawKite();
  }

  // ---- input -------------------------------------------------------------------------------
  document.addEventListener("keydown", function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if ((e.key === "Enter" || e.key === " ") && session && session.confirm()) {
      e.preventDefault();
      return;
    }
    if (e.code === "Space") {
      keysHeld.space = true;
      e.preventDefault();
    } else if (e.code === "ArrowUp") {
      keysHeld.up = true;
      e.preventDefault();
    } else if (e.key === "Escape" && session) {
      session.pause();
    }
  });
  document.addEventListener("keyup", function (e) {
    if (e.code === "Space") {
      keysHeld.space = false;
      e.preventDefault();
    } else if (e.code === "ArrowUp") {
      keysHeld.up = false;
    }
  });
  function clearHeld() {
    keysHeld.space = false;
    keysHeld.up = false;
    pointerHeld = false;
    $("hold").classList.remove("active");
  }
  window.addEventListener("blur", clearHeld);

  function bindHold() {
    var b = $("hold");
    function on(e) {
      pointerHeld = true;
      b.classList.add("active");
      e.preventDefault();
    }
    function off() {
      pointerHeld = false;
      b.classList.remove("active");
    }
    b.addEventListener("pointerdown", on);
    b.addEventListener("pointerup", off);
    b.addEventListener("pointercancel", off);
    b.addEventListener("pointerleave", off);
    canvas.addEventListener("pointerdown", on);
    canvas.addEventListener("pointerup", off);
    canvas.addEventListener("pointercancel", off);
    canvas.addEventListener("pointerleave", off);
  }

  function newFlight() {
    state = L.restart(state, A.randomSeed());
    say("New flight.");
    updateText(true);
    render();
    if (session) session.saveNow();
  }

  function changePace() {
    state.pace = L.nextPace(state.pace);
    say("Pace: " + state.pace + ".");
    updateText(true);
    if (session) session.saveNow();
  }

  function setup(info) {
    reduced = !!info.reducedMotion;
    document.documentElement.setAttribute("data-theme", info.theme === "dark" ? "dark" : "light");
    document.documentElement.classList.toggle("reduced", reduced);
    state = L.validate(info.restore) || L.create(A.randomSeed(), reduced);
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

  bindHold();
  $("again").addEventListener("click", newFlight);
  $("pace").addEventListener("click", changePace);
  canvas.focus();
})();
