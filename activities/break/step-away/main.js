(function () {
  "use strict";
  var IDEAS = [
    "Stand up and stretch your arms overhead.",
    "Look at something far away for a little while.",
    "Get yourself a glass of water.",
    "Roll your shoulders back, slowly.",
  ];
  var PHASE_SECONDS = 4;
  // elapsed counts only time the activity was running, so time paused is not counted.
  var state = { elapsed: 0, pacerOn: false, idea: 0, pacerSecs: 0 };
  var reduced = false;
  var running = false;
  var timer = null;
  var lastTick = 0;
  var lastSave = 0;

  function $(id) {
    return document.getElementById(id);
  }

  function pad(n) {
    return n < 10 ? "0" + n : String(n);
  }
  function fmt(total) {
    var s = Math.floor(total);
    var h = Math.floor(s / 3600);
    var m = Math.floor((s % 3600) / 60);
    var sec = s % 60;
    return (h > 0 ? h + ":" + pad(m) : pad(m)) + ":" + pad(sec);
  }
  function inhaling() {
    return Math.floor(state.pacerSecs / PHASE_SECONDS) % 2 === 0;
  }

  function renderPacer() {
    var on = state.pacerOn;
    $("pacer-area").hidden = !on;
    $("pacer-toggle").setAttribute("aria-pressed", on ? "true" : "false");
    $("pacer-toggle").textContent = on ? "Stop breathing cue" : "Start breathing cue";
    if (!on) return;
    var inh = inhaling();
    $("cue").textContent = inh ? "Breathe in" : "Breathe out";
    if (!reduced) $("circle").classList.toggle("big", inh);
  }
  function renderClock() {
    $("clock").textContent = fmt(state.elapsed);
  }
  function renderIdea() {
    $("idea").textContent = IDEAS[state.idea % IDEAS.length];
  }
  function snapshot() {
    return {
      elapsed: state.elapsed,
      pacerOn: state.pacerOn,
      idea: state.idea,
      pacerSecs: state.pacerSecs,
    };
  }
  function save() {
    LullshiftBridge.autosave(snapshot());
  }

  // What the host shows while paused (S05-T09).
  function report() {
    var minutes = Math.floor(state.elapsed / 60);
    LullshiftBridge.summary([
      { label: "Time away", value: minutes < 1 ? "Under a minute" : minutes + " min" },
    ]);
  }

  function tick() {
    var now = Date.now();
    var dt = Math.min(2, Math.max(0, (now - lastTick) / 1000));
    lastTick = now;
    if (!running) return;
    var prevPhase = Math.floor(state.pacerSecs / PHASE_SECONDS);
    state.elapsed += dt;
    if (state.pacerOn) state.pacerSecs += dt;
    renderClock();
    if (state.pacerOn && Math.floor(state.pacerSecs / PHASE_SECONDS) !== prevPhase) renderPacer();
    if (now - lastSave > 5000) {
      lastSave = now;
      save();
    }
  }

  function setup(init) {
    var r = init.restore;
    reduced = !!init.reducedMotion;
    document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
    if (reduced) document.documentElement.classList.add("reduced");
    if (r && typeof r === "object") {
      if (typeof r.elapsed === "number" && r.elapsed >= 0) state.elapsed = r.elapsed;
      state.pacerOn = r.pacerOn === true;
      if (typeof r.idea === "number" && r.idea >= 0) state.idea = Math.floor(r.idea) % IDEAS.length;
      if (typeof r.pacerSecs === "number" && r.pacerSecs >= 0) state.pacerSecs = r.pacerSecs;
      var w = $("welcome");
      w.hidden = false;
      setTimeout(function () {
        w.hidden = true;
      }, 6000);
    }
    renderClock();
    renderIdea();
    renderPacer();
    $("pacer-toggle").addEventListener("click", function () {
      state.pacerOn = !state.pacerOn;
      state.pacerSecs = 0;
      renderPacer();
      save();
    });
    $("idea-next").addEventListener("click", function () {
      state.idea = (state.idea + 1) % IDEAS.length;
      renderIdea();
      save();
    });
    running = true;
    lastTick = Date.now();
    lastSave = lastTick;
    if (!timer) timer = setInterval(tick, 500);
    save();
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: snapshot,
    onPause: function () {
      running = false;
      report();
    },
    onResume: function () {
      lastTick = Date.now();
      running = true;
    },
  });
})();
