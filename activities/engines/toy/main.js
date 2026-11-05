// Creative Toy, engine lullshift.toy@1 (ADR-016). The pack arrives as `content` in onInit.
(function () {
  "use strict";
  var L = globalThis.ToyLogic;

  var SAMPLE = {
    title: "Creative Toy",
    intro: "Move with the arrow keys and press Space to leave a mark. There is nothing to finish.",
    palette: [
      { name: "Soft violet", color: "#6b4fd8" },
      { name: "Warm coral", color: "#e07a5f" },
      { name: "Sea green", color: "#3d9a8b" },
      { name: "Sand gold", color: "#d9a441" },
    ],
    mode: "ripples",
    symmetry: "none",
    brushSize: { min: 4, max: 16 },
    background: "#fffdf9",
  };

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    canvas: document.getElementById("canvas"),
    palette: document.getElementById("palette"),
    status: document.getElementById("status"),
    summary: document.getElementById("canvasSummary"),
    place: document.getElementById("place"),
    undo: document.getElementById("undo"),
    clear: document.getElementById("clear"),
    size: document.getElementById("size"),
  };
  var ctx = el.canvas.getContext("2d");

  var content = null;
  var marks = [];
  var cursor = { x: L.W / 2, y: L.H / 2 };
  var color = 0;
  var size = 4;
  var showCursor = false;
  var reduced = false;
  var cleared = null; // marks removed by the last Clear, so Undo can bring them back
  var swatches = [];
  var raf = 0;
  var grow = null; // { start } while the newest mark is easing in
  var dragging = false;
  var lastDrag = null;

  function sizeSteps() {
    var out = [];
    for (var i = 0; i < 4; i++) {
      var v = Math.round(content.min + ((content.max - content.min) * i) / 3);
      if (out.indexOf(v) < 0) out.push(v);
    }
    return out;
  }

  function state() {
    return { marks: marks, cursor: cursor, color: color, size: size };
  }

  var moveTimer = null;

  function say(text) {
    if (moveTimer !== null) {
      clearTimeout(moveTimer);
      moveTimer = null;
    }
    el.status.textContent = text;
  }

  // Cursor position, announced politely once the arrow keys pause.
  function sayCursorSoon() {
    if (moveTimer !== null) clearTimeout(moveTimer);
    moveTimer = setTimeout(function () {
      moveTimer = null;
      el.status.textContent =
        "Cursor " +
        Math.round((cursor.x / L.W) * 100) +
        " percent across, " +
        Math.round((cursor.y / L.H) * 100) +
        " percent down.";
    }, 300);
  }

  // ---- drawing ----

  function drawOne(c, prev, t) {
    var hex = content.palette[c[2]].color;
    var s = c[3] * t;
    var sym = content.symmetry;
    var pts = L.reflect({ x: c[0], y: c[1] }, sym);
    var prevPts = prev ? L.reflect({ x: prev[0], y: prev[1] }, sym) : null;
    ctx.fillStyle = hex;
    ctx.strokeStyle = hex;
    for (var k = 0; k < pts.length; k++) {
      var x = pts[k].x;
      var y = pts[k].y;
      if (content.mode === "ripples") {
        ctx.lineWidth = Math.max(1, s / 6);
        [0.5, 1, 1.6].forEach(function (f, j) {
          ctx.globalAlpha = 0.85 - j * 0.25;
          ctx.beginPath();
          ctx.arc(x, y, Math.max(0.5, s * f), 0, Math.PI * 2);
          ctx.stroke();
        });
      } else if (content.mode === "mosaic") {
        var side = Math.max(4, Math.min(14, s));
        ctx.globalAlpha = 0.9;
        ctx.fillRect(x - side / 2, y - side / 2, side, side);
      } else if (content.mode === "stars") {
        ctx.globalAlpha = 0.95;
        ctx.beginPath();
        for (var p = 0; p < 8; p++) {
          var a = (p * Math.PI) / 4 - Math.PI / 2;
          var r = p % 2 === 0 ? s : s * 0.35;
          var px = x + Math.cos(a) * r;
          var py = y + Math.sin(a) * r;
          if (p === 0) ctx.moveTo(px, py);
          else ctx.lineTo(px, py);
        }
        ctx.closePath();
        ctx.fill();
      } else {
        ctx.lineWidth = Math.max(1, s / 5);
        ctx.lineCap = "round";
        ctx.globalAlpha = 0.85;
        if (prevPts) {
          ctx.beginPath();
          ctx.moveTo(prevPts[k].x, prevPts[k].y);
          ctx.lineTo(x, y);
          ctx.stroke();
        }
        ctx.beginPath();
        ctx.arc(x, y, Math.max(1, s / 5), 0, Math.PI * 2);
        ctx.fill();
      }
    }
    ctx.globalAlpha = 1;
  }

  function draw(t) {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.fillStyle = content.background;
    ctx.fillRect(0, 0, el.canvas.width, el.canvas.height);
    for (var i = 0; i < marks.length; i++) {
      drawOne(marks[i], i > 0 ? marks[i - 1] : null, i === marks.length - 1 ? t : 1);
    }
    if (showCursor) {
      var ink = L.cursorInk(content.background);
      ctx.strokeStyle = ink;
      ctx.lineWidth = 1.5;
      ctx.globalAlpha = 1;
      var r = Math.max(4, size / 2 + 3);
      ctx.beginPath();
      ctx.arc(cursor.x, cursor.y, r, 0, Math.PI * 2);
      ctx.moveTo(cursor.x - r - 4, cursor.y);
      ctx.lineTo(cursor.x - r + 2, cursor.y);
      ctx.moveTo(cursor.x + r - 2, cursor.y);
      ctx.lineTo(cursor.x + r + 4, cursor.y);
      ctx.moveTo(cursor.x, cursor.y - r - 4);
      ctx.lineTo(cursor.x, cursor.y - r + 2);
      ctx.moveTo(cursor.x, cursor.y + r - 2);
      ctx.lineTo(cursor.x, cursor.y + r + 4);
      ctx.stroke();
    }
  }

  function stopAnim() {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    grow = null;
  }

  function tick(now) {
    raf = 0;
    if (!grow) return;
    var t = Math.min(1, (now - grow.start) / 350);
    draw(0.3 + 0.7 * t);
    if (t < 1) raf = requestAnimationFrame(tick);
    else grow = null;
  }

  function render(animateNewest) {
    stopAnim();
    if (animateNewest && !reduced && marks.length > 0) {
      grow = { start: performance.now() };
      draw(0.3);
      raf = requestAnimationFrame(tick);
    } else {
      draw(1);
    }
    // The summary is a description, not the name, so a placed mark is spoken once (via status).
    el.summary.textContent = L.describe(marks);
    el.undo.disabled = marks.length === 0 && cleared === null;
    el.clear.disabled = marks.length === 0;
    el.size.textContent = "Size " + size;
    for (var i = 0; i < swatches.length; i++) {
      swatches[i].setAttribute("aria-pressed", i === color ? "true" : "false");
    }
  }

  function report(animateNewest) {
    render(animateNewest);
    LullshiftBridge.autosave(state());
    LullshiftBridge.summary([
      { label: "Marks", value: marks.length },
      { label: "Colour", value: content.palette[color].name.slice(0, 24) },
    ]);
  }

  // ---- actions ----

  function placeAt(x, y, quiet) {
    var m = L.makeMark(x, y, color, size, content.mode);
    var last = marks.length ? marks[marks.length - 1] : null;
    if (last && last[0] === m[0] && last[1] === m[1] && last[2] === m[2] && last[3] === m[3]) {
      return false;
    }
    marks = L.addMark(marks, m);
    cleared = null;
    report(true);
    if (!quiet) say(L.describe(marks));
    return true;
  }

  function doUndo() {
    if (marks.length > 0) {
      marks = L.undo(marks);
      report(false);
      say("Removed the last mark. " + L.describe(marks));
    } else if (cleared) {
      marks = cleared;
      cleared = null;
      report(false);
      say("Brought the marks back. " + L.describe(marks));
    }
  }

  function doClear() {
    if (marks.length === 0) return;
    cleared = marks;
    marks = [];
    report(false);
    say("Canvas cleared. Undo brings it back.");
  }

  function setColor(i) {
    if (i < 0 || i >= content.palette.length || i === color) return;
    color = i;
    report(false);
    say("Colour " + (i + 1) + " of " + content.palette.length + ": " + content.palette[i].name);
  }

  function cycleSize(dir) {
    var steps = sizeSteps();
    var at = steps.indexOf(size);
    if (at < 0) at = 0;
    var next = (at + dir + steps.length) % steps.length;
    size = steps[next];
    report(false);
    say("Brush size " + size);
  }

  function moveBy(dx, dy, big) {
    cursor = L.moveCursor(cursor, dx, dy, big);
    showCursor = true;
    report(false);
    sayCursorSoon();
  }

  // ---- building the page ----

  function buildPalette() {
    el.palette.textContent = "";
    swatches = [];
    content.palette.forEach(function (p, i) {
      var b = document.createElement("button");
      b.type = "button";
      b.style.backgroundColor = p.color;
      b.setAttribute("aria-label", "Colour " + (i + 1) + ", " + p.name);
      b.title = i + 1 + ": " + p.name;
      b.addEventListener("click", function () {
        setColor(i);
      });
      el.palette.appendChild(b);
      swatches.push(b);
    });
  }

  el.place.addEventListener("click", function () {
    if (content) placeAt(cursor.x, cursor.y);
  });
  el.undo.addEventListener("click", function () {
    if (content) doUndo();
  });
  el.clear.addEventListener("click", function () {
    if (content) doClear();
  });
  el.size.addEventListener("click", function () {
    if (content) cycleSize(1);
  });

  el.canvas.addEventListener("focus", function () {
    if (!content) return;
    showCursor = true;
    render(false);
  });
  el.canvas.addEventListener("blur", function () {
    if (!content) return;
    showCursor = false;
    render(false);
  });
  el.canvas.addEventListener("keydown", function (e) {
    if (!content || e.altKey || e.metaKey) return;
    var k = e.key;
    if (e.ctrlKey) {
      if (k === "z" || k === "Z") {
        e.preventDefault();
        doUndo();
      }
      return;
    }
    var handled = true;
    if (k === "ArrowLeft") moveBy(-1, 0, e.shiftKey);
    else if (k === "ArrowRight") moveBy(1, 0, e.shiftKey);
    else if (k === "ArrowUp") moveBy(0, -1, e.shiftKey);
    else if (k === "ArrowDown") moveBy(0, 1, e.shiftKey);
    else if (k === " " || k === "Enter") placeAt(cursor.x, cursor.y);
    else if (k === "c" || k === "C") doClear();
    else if (k === "u" || k === "U" || k === "Backspace") doUndo();
    else if (k === "[" || k === "-") cycleSize(-1);
    else if (k === "]" || k === "+" || k === "=") cycleSize(1);
    else if (k.length === 1 && k >= "1" && k <= "8") setColor(Number(k) - 1);
    else handled = false;
    if (handled) e.preventDefault();
  });

  function pointFromEvent(e) {
    var r = el.canvas.getBoundingClientRect();
    if (!r.width || !r.height) return { x: cursor.x, y: cursor.y };
    return {
      x: Math.max(0, Math.min(L.W, ((e.clientX - r.left) / r.width) * L.W)),
      y: Math.max(0, Math.min(L.H, ((e.clientY - r.top) / r.height) * L.H)),
    };
  }
  el.canvas.addEventListener("pointerdown", function (e) {
    if (!content || e.button !== 0) return;
    e.preventDefault();
    el.canvas.focus();
    var p = pointFromEvent(e);
    cursor = { x: Math.round(p.x), y: Math.round(p.y) };
    dragging = true;
    lastDrag = p;
    try {
      el.canvas.setPointerCapture(e.pointerId);
    } catch (err) {
      // Capture is a nicety; drawing still works without it.
    }
    placeAt(p.x, p.y);
  });
  el.canvas.addEventListener("pointermove", function (e) {
    if (!content || !dragging) return;
    var p = pointFromEvent(e);
    var d = Math.hypot(p.x - lastDrag.x, p.y - lastDrag.y);
    if (d < Math.max(6, size)) return;
    lastDrag = p;
    cursor = { x: Math.round(p.x), y: Math.round(p.y) };
    placeAt(p.x, p.y, true);
  });
  function endDrag() {
    if (!dragging) return;
    dragging = false;
    if (content) say(L.describe(marks));
  }
  el.canvas.addEventListener("pointerup", endDrag);
  el.canvas.addEventListener("pointercancel", endDrag);

  function load(init) {
    document.documentElement.setAttribute("data-theme", init.theme);
    reduced = init.reducedMotion === true;
    document.documentElement.classList.toggle("reduced", reduced);
    content = L.normalizeContent(init.content) || L.normalizeContent(SAMPLE);
    el.title.textContent = content.title;
    el.intro.hidden = !content.intro;
    el.intro.textContent = content.intro || "";
    buildPalette();
    var s = L.restoreState(init.restore, content);
    marks = s ? s.marks : [];
    cursor = s ? s.cursor : { x: L.W / 2, y: L.H / 2 };
    color = s ? s.color : 0;
    size = s ? s.size : content.min;
    cleared = null;
    showCursor = false;
    say(
      marks.length
        ? "Welcome back. " + L.describe(marks)
        : "Arrow keys move, Space places a mark. Number keys pick a colour.",
    );
    report(false);
  }

  LullshiftBridge.start({
    onInit: load,
    saveState: function () {
      return content ? state() : undefined;
    },
    onPause: stopAnim,
    onResume: function () {
      if (content) render(false);
    },
  });
  // Until the host sends the pack, show the friendly sample so the page is never blank.
  content = L.normalizeContent(SAMPLE);
  el.title.textContent = content.title;
  buildPalette();
  render(false);
})();
