// Breathing Space, engine lullshift.breathing@1 (ADR-016). The pattern arrives as `content` in
// onInit. Time is counted from the page's own ticks, so a pause (host or button) stops it, and the
// saved state is the exact position in milliseconds.
(function () {
  "use strict";
  var L = globalThis.BreathingLogic;
  var content = L.SAMPLE;
  var elapsed = 0;
  var running = false;
  var wasRunning = false;
  var reduced = false;
  var timer = null;
  var last = 0;
  var lastKey = "";

  function $(id) {
    return document.getElementById(id);
  }
  var el = {
    title: $("title"),
    intro: $("intro"),
    orb: $("orb"),
    count: $("count"),
    cue: $("cue"),
    round: $("round"),
    note: $("note"),
    toggle: $("toggle"),
    restart: $("restart"),
  };

  function setText(node, text) {
    if (node.textContent !== text) node.textContent = text;
  }

  function isDone() {
    return L.locate(content, elapsed).done;
  }

  function render() {
    var pos = L.locate(content, elapsed);
    var phase = content.phases[pos.phaseIndex];
    var started = elapsed > 0;
    el.title.textContent = content.title;
    el.intro.hidden = !content.intro;
    el.intro.textContent = content.intro || "";

    if (pos.done) {
      el.orb.setAttribute("data-kind", "rest");
      el.orb.style.transform = "scale(" + L.MIN_SCALE + ")";
      setText(el.cue, "Done");
      setText(el.count, "");
      setText(el.round, content.rounds + (content.rounds === 1 ? " round" : " rounds") + " done");
      el.note.hidden = false;
      setText(el.note, content.closing || "That is the set. Carry on whenever you are ready.");
    } else {
      el.orb.setAttribute("data-kind", phase.kind);
      // Reduced motion: the shape stays still at a middle size; the words and numbers do the work.
      el.orb.style.transform = "scale(" + (reduced ? 0.7 : L.scaleAt(content, elapsed)) + ")";
      setText(el.cue, started ? L.cueFor(phase) : "Ready");
      setText(el.count, started ? String(L.secondsLeft(pos)) : "");
      setText(el.round, "Round " + pos.round + " of " + content.rounds);
      el.note.hidden = started;
      setText(el.note, started ? "" : "Press Begin when you are ready. You can pause at any time.");
    }

    var had = document.activeElement;
    el.toggle.hidden = pos.done;
    el.toggle.textContent = running ? "Pause" : started ? "Continue" : "Begin";
    el.restart.disabled = !started;
    // Never leave focus on a control that just went away: hand it to the other button.
    if (pos.done && had === el.toggle) el.restart.focus();
    else if (!started && had === el.restart) el.toggle.focus();
  }

  function report() {
    LullshiftBridge.autosave(L.makeState(elapsed));
    var pos = L.locate(content, elapsed);
    LullshiftBridge.summary([
      { label: "Round", value: pos.round + " of " + content.rounds },
      { label: "Time left", value: L.formatTime(L.totalMs(content) - elapsed) },
    ]);
  }

  function key() {
    var pos = L.locate(content, elapsed);
    return pos.round + ":" + pos.phaseIndex + ":" + Math.floor(elapsed / 1000);
  }

  function stop() {
    running = false;
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  function tick() {
    var now = performance.now();
    elapsed = L.advance(content, elapsed, now - last);
    last = now;
    if (isDone()) stop();
    render();
    var k = key();
    if (k !== lastKey || !running) {
      lastKey = k;
      report();
    }
  }

  function start() {
    if (running || isDone()) return;
    running = true;
    last = performance.now();
    timer = setInterval(tick, reduced ? 250 : 50);
    render();
    report();
  }

  el.toggle.addEventListener("click", function () {
    if (running) {
      stop();
      render();
      report();
    } else {
      start();
    }
  });
  el.restart.addEventListener("click", function () {
    stop();
    elapsed = 0;
    lastKey = "";
    render();
    report();
    el.toggle.focus();
  });

  function load(c, restore) {
    stop();
    content = L.validContent(c) ? c : L.SAMPLE;
    elapsed = L.validState(restore, content) ? restore.elapsed : 0;
    lastKey = "";
    render();
    report();
  }

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      reduced = init.reducedMotion === true;
      load(init.content, init.restore);
    },
    saveState: function () {
      return L.makeState(elapsed);
    },
    onPause: function () {
      wasRunning = running;
      stop();
      render();
    },
    onResume: function () {
      if (wasRunning) start();
      wasRunning = false;
    },
  });
  render();
})();
