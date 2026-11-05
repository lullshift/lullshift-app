// Guided Pause, engine lullshift.pause@1 (ADR-016). The steps arrive as `content` in onInit.
// Time is counted from the page's own ticks, so a pause (host or button) stops it, and the saved
// state is the exact step and the milliseconds spent in it.
(function () {
  "use strict";
  var L = globalThis.PauseLogic;
  var SVG = "http://www.w3.org/2000/svg";
  var content = L.SAMPLE;
  var pos = L.start();
  var running = false;
  var wasRunning = false;
  var reduced = false;
  var timer = null;
  var last = 0;
  var lastKey = "";
  var shownLook = null;
  var segments = [];
  var builtFor = null;

  function $(id) {
    return document.getElementById(id);
  }
  var el = {
    title: $("title"),
    intro: $("intro"),
    look: $("look"),
    text: $("text"),
    status: $("status"),
    bar: $("bar"),
    prev: $("prev"),
    next: $("next"),
    toggle: $("toggle"),
    restart: $("restart"),
  };

  // Tiny illustrations from plain shapes. Each entry is [tag, class, attributes].
  var ART = {
    eyes: [
      ["ellipse", "line", { cx: 38, cy: 40, rx: 22, ry: 15 }],
      ["ellipse", "line", { cx: 82, cy: 40, rx: 22, ry: 15 }],
      ["circle", "fill", { cx: 38, cy: 40, r: 7 }],
      ["circle", "fill", { cx: 82, cy: 40, r: 7 }],
    ],
    hands: [
      ["rect", "line", { x: 18, y: 26, width: 34, height: 38, rx: 14 }],
      ["rect", "line", { x: 68, y: 26, width: 34, height: 38, rx: 14 }],
      ["circle", "fill", { cx: 35, cy: 20, r: 5 }],
      ["circle", "fill", { cx: 85, cy: 20, r: 5 }],
    ],
    window: [
      ["rect", "line", { x: 32, y: 8, width: 56, height: 64, rx: 4 }],
      ["line", "line", { x1: 60, y1: 8, x2: 60, y2: 72 }],
      ["line", "line", { x1: 32, y1: 40, x2: 88, y2: 40 }],
      ["circle", "fill", { cx: 74, cy: 24, r: 6 }],
    ],
    shoulders: [
      ["circle", "line", { cx: 60, cy: 22, r: 12 }],
      ["rect", "line", { x: 24, y: 40, width: 72, height: 34, rx: 17 }],
      ["circle", "fill", { cx: 28, cy: 44, r: 4 }],
      ["circle", "fill", { cx: 92, cy: 44, r: 4 }],
    ],
    jaw: [
      ["circle", "line", { cx: 60, cy: 40, r: 30 }],
      ["circle", "fill", { cx: 49, cy: 33, r: 3 }],
      ["circle", "fill", { cx: 71, cy: 33, r: 3 }],
      ["ellipse", "line", { cx: 60, cy: 52, rx: 8, ry: 4 }],
    ],
    breath: [
      ["circle", "line", { cx: 60, cy: 40, r: 34 }],
      ["circle", "line", { cx: 60, cy: 40, r: 22 }],
      ["circle", "fill", { cx: 60, cy: 40, r: 9 }],
    ],
  };

  function drawLook(name) {
    if (name === shownLook) return;
    shownLook = name;
    el.look.textContent = "";
    el.look.hidden = !ART[name];
    if (!ART[name]) return;
    var svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 120 80");
    svg.setAttribute("focusable", "false");
    ART[name].forEach(function (s) {
      var node = document.createElementNS(SVG, s[0]);
      node.setAttribute("class", s[1]);
      for (var k in s[2]) node.setAttribute(k, String(s[2][k]));
      svg.appendChild(node);
    });
    el.look.appendChild(svg);
  }

  function setText(node, text) {
    if (node.textContent !== text) node.textContent = text;
  }

  function buildBar() {
    if (builtFor === content) return;
    builtFor = content;
    el.bar.textContent = "";
    segments = [];
    for (var i = 0; i < content.steps.length; i++) {
      var seg = document.createElement("span");
      seg.className = "seg";
      var fill = document.createElement("span");
      fill.className = "fill";
      seg.appendChild(fill);
      el.bar.appendChild(seg);
      segments.push(fill);
    }
  }

  // How full a step's segment is. In reduced motion it steps once a second instead of sliding.
  function segmentFill(i, done) {
    if (i < pos.index) return 1;
    if (i !== pos.index || done) return 0;
    if (!reduced) return L.fraction(content, pos);
    var secs = L.stepMs(content, i) / 1000;
    return Math.floor(pos.elapsed / 1000) / secs;
  }

  function render() {
    buildBar();
    var done = L.isDone(content, pos);
    var fresh = pos.index === 0 && pos.elapsed === 0;
    el.title.textContent = content.title;
    el.intro.hidden = !content.intro || !fresh;
    el.intro.textContent = content.intro || "";

    if (done) {
      drawLook("");
      setText(el.text, content.closing || "That is the pause. Carry on whenever you are ready.");
      setText(el.status, "Finished");
    } else {
      var step = content.steps[pos.index];
      drawLook(step.look || "");
      setText(el.text, step.text);
      var where = "Step " + (pos.index + 1) + " of " + content.steps.length;
      setText(
        el.status,
        fresh && !running
          ? where + ". Press Begin when you are ready."
          : where + ", " + L.secondsLeftInStep(content, pos) + " s left",
      );
    }

    var total = L.totalMs(content);
    var doneMs = total - L.timeLeft(content, pos);
    el.bar.setAttribute("aria-valuenow", String(Math.round((doneMs / total) * 100)));
    for (var i = 0; i < segments.length; i++) {
      segments[i].style.transform = "scaleX(" + segmentFill(i, done) + ")";
    }

    var had = document.activeElement;
    el.prev.disabled = fresh && !done;
    el.next.hidden = done;
    el.toggle.hidden = done;
    el.toggle.textContent = running ? "Pause" : fresh ? "Begin" : "Continue";
    el.restart.disabled = fresh && !done;
    // Never leave focus on a control that just went away: hand it to the main button.
    if (done && (had === el.toggle || had === el.next)) el.restart.focus();
    else if ((had === el.prev && el.prev.disabled) || (had === el.restart && el.restart.disabled))
      el.toggle.focus();
  }

  function report() {
    LullshiftBridge.autosave(L.makeState(pos));
    var done = L.isDone(content, pos);
    LullshiftBridge.summary([
      {
        label: "Step",
        value: done ? "finished" : pos.index + 1 + " of " + content.steps.length,
      },
      { label: "Time left", value: L.formatTime(L.timeLeft(content, pos)) },
    ]);
  }

  function stop() {
    running = false;
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  function key() {
    return pos.index + ":" + Math.floor(pos.elapsed / 1000);
  }

  function tick() {
    var now = performance.now();
    pos = L.advance(content, pos, now - last);
    last = now;
    if (L.isDone(content, pos)) stop();
    render();
    var k = key();
    if (k !== lastKey || !running) {
      lastKey = k;
      report();
    }
  }

  function start() {
    if (running || L.isDone(content, pos)) return;
    running = true;
    last = performance.now();
    timer = setInterval(tick, reduced ? 250 : 100);
    render();
    report();
  }

  function moveTo(p) {
    pos = p;
    if (L.isDone(content, pos)) stop();
    last = performance.now();
    lastKey = key();
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
  el.prev.addEventListener("click", function () {
    moveTo(L.previous(content, pos));
  });
  el.next.addEventListener("click", function () {
    moveTo(L.next(content, pos));
    if (L.isDone(content, pos)) el.restart.focus();
  });
  el.restart.addEventListener("click", function () {
    stop();
    moveTo(L.start());
    el.toggle.focus();
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      reduced = init.reducedMotion === true;
      stop();
      content = L.validContent(init.content) ? init.content : L.SAMPLE;
      pos = L.validState(init.restore, content)
        ? { index: init.restore.index, elapsed: init.restore.elapsed }
        : L.start();
      lastKey = key();
      render();
      report();
    },
    saveState: function () {
      return L.makeState(pos);
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
