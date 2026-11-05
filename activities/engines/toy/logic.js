// Pure rules for Creative Toy (lullshift.toy@1). No DOM. Attaches its API to globalThis.ToyLogic.
(function (root) {
  "use strict";

  var W = 320;
  var H = 240;
  var STEP = 8;
  var BIG_STEP = 32;
  var MAX_MARKS = 2000;
  var MODES = ["ripples", "mosaic", "stars", "threads"];
  var SYMMETRIES = ["none", "mirror", "quad", "radial6"];
  var CELL = 16;
  var HEX = /^#[0-9a-f]{6}$/;

  function clamp(v, lo, hi) {
    return Math.max(lo, Math.min(hi, v));
  }

  function isInt(v) {
    return typeof v === "number" && isFinite(v) && Math.floor(v) === v;
  }

  // A cursor is {x, y} in canvas units, always inside 0..W and 0..H.
  function moveCursor(cur, dx, dy, big) {
    var s = big ? BIG_STEP : STEP;
    return {
      x: clamp(cur.x + dx * s, 0, W),
      y: clamp(cur.y + dy * s, 0, H),
    };
  }

  function snapToCell(p) {
    return {
      x: clamp(Math.floor(p.x / CELL) * CELL + CELL / 2, CELL / 2, W - CELL / 2),
      y: clamp(Math.floor(p.y / CELL) * CELL + CELL / 2, CELL / 2, H - CELL / 2),
    };
  }

  // Marks are [x, y, colourIndex, size], all integers. Order is the order they were placed.
  function makeMark(x, y, colorIndex, size, mode) {
    var p = { x: Math.round(x), y: Math.round(y) };
    if (mode === "mosaic") p = snapToCell(p);
    return [clamp(Math.round(p.x), 0, W), clamp(Math.round(p.y), 0, H), colorIndex, size];
  }

  // Returns a new list with the mark added; the oldest marks fall away past the cap.
  function addMark(marks, mark) {
    var next = marks.concat([mark]);
    return next.length > MAX_MARKS ? next.slice(next.length - MAX_MARKS) : next;
  }

  function undo(marks) {
    return marks.slice(0, Math.max(0, marks.length - 1));
  }

  function r1(n) {
    return Math.round(n * 10) / 10;
  }

  // The points a single placed point stands for under a symmetry. The first is the point itself.
  function reflect(p, symmetry) {
    var cx = W / 2;
    var cy = H / 2;
    if (symmetry === "mirror") return [p, { x: r1(W - p.x), y: p.y }];
    if (symmetry === "quad") {
      return [
        p,
        { x: r1(W - p.x), y: p.y },
        { x: p.x, y: r1(H - p.y) },
        { x: r1(W - p.x), y: r1(H - p.y) },
      ];
    }
    if (symmetry === "radial6") {
      var out = [];
      for (var k = 0; k < 6; k++) {
        var a = (k * Math.PI) / 3;
        var dx = p.x - cx;
        var dy = p.y - cy;
        out.push({
          x: r1(cx + dx * Math.cos(a) - dy * Math.sin(a)),
          y: r1(cy + dx * Math.sin(a) + dy * Math.cos(a)),
        });
      }
      return out;
    }
    return [p];
  }

  function symmetryCount(symmetry) {
    return reflect({ x: 0, y: 0 }, symmetry).length;
  }

  function distinctColors(marks) {
    var seen = {};
    var n = 0;
    for (var i = 0; i < marks.length; i++) {
      if (!seen[marks[i][2]]) {
        seen[marks[i][2]] = true;
        n++;
      }
    }
    return n;
  }

  // The text a screen reader gets for the canvas.
  function describe(marks) {
    if (marks.length === 0) return "Empty canvas";
    var c = distinctColors(marks);
    return (
      marks.length +
      (marks.length === 1 ? " mark" : " marks") +
      " in " +
      c +
      (c === 1 ? " colour" : " colours")
    );
  }

  function luminance(hex) {
    if (!HEX.test(hex)) return 1;
    var r = parseInt(hex.slice(1, 3), 16);
    var g = parseInt(hex.slice(3, 5), 16);
    var b = parseInt(hex.slice(5, 7), 16);
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  }

  // Dark ink on light backgrounds, light ink on dark ones, for the cursor ring.
  function cursorInk(background) {
    return luminance(background) > 0.5 ? "#2a2633" : "#f6f2ec";
  }

  function validMark(m, colors) {
    return (
      Array.isArray(m) &&
      m.length === 4 &&
      isInt(m[0]) &&
      m[0] >= 0 &&
      m[0] <= W &&
      isInt(m[1]) &&
      m[1] >= 0 &&
      m[1] <= H &&
      isInt(m[2]) &&
      m[2] >= 0 &&
      m[2] < colors &&
      isInt(m[3]) &&
      m[3] >= 1 &&
      m[3] <= 40
    );
  }

  // Cheap second look at pack data (the host has already validated it against the schema).
  function normalizeContent(c) {
    if (!c || typeof c !== "object") return null;
    if (typeof c.title !== "string" || c.title.length < 1 || c.title.length > 80) return null;
    if (c.intro !== undefined && (typeof c.intro !== "string" || c.intro.length > 300)) return null;
    if (!Array.isArray(c.palette) || c.palette.length < 2 || c.palette.length > 8) return null;
    var palette = [];
    for (var i = 0; i < c.palette.length; i++) {
      var p = c.palette[i];
      if (!p || typeof p !== "object") return null;
      if (typeof p.name !== "string" || p.name.length < 1 || p.name.length > 32) return null;
      if (typeof p.color !== "string" || !HEX.test(p.color)) return null;
      palette.push({ name: p.name, color: p.color });
    }
    if (MODES.indexOf(c.mode) < 0) return null;
    var symmetry = c.symmetry === undefined ? "none" : c.symmetry;
    if (SYMMETRIES.indexOf(symmetry) < 0) return null;
    var min = 4;
    var max = 16;
    if (c.brushSize !== undefined) {
      var b = c.brushSize;
      if (!b || typeof b !== "object") return null;
      if (b.min !== undefined) min = b.min;
      if (b.max !== undefined) max = b.max;
      if (!isInt(min) || !isInt(max) || min < 2 || max > 40 || min > max) return null;
    }
    var background = c.background === undefined ? "#fffdf9" : c.background;
    if (typeof background !== "string" || !HEX.test(background)) return null;
    return {
      title: c.title,
      intro: c.intro,
      palette: palette,
      mode: c.mode,
      symmetry: symmetry,
      min: min,
      max: max,
      background: background,
    };
  }

  // Returns a safe state from whatever was saved, or null when nothing usable was saved.
  function restoreState(raw, content) {
    if (!raw || typeof raw !== "object" || !Array.isArray(raw.marks)) return null;
    var n = content.palette.length;
    var marks = [];
    var start = Math.max(0, raw.marks.length - MAX_MARKS);
    for (var i = start; i < raw.marks.length; i++) {
      var m = raw.marks[i];
      if (!validMark(m, n)) continue;
      marks.push([m[0], m[1], m[2], clamp(m[3], content.min, content.max)]);
    }
    var cur = raw.cursor && typeof raw.cursor === "object" ? raw.cursor : {};
    return {
      marks: marks,
      cursor: {
        x: isInt(cur.x) ? clamp(cur.x, 0, W) : W / 2,
        y: isInt(cur.y) ? clamp(cur.y, 0, H) : H / 2,
      },
      color: isInt(raw.color) && raw.color >= 0 && raw.color < n ? raw.color : 0,
      size: isInt(raw.size) ? clamp(raw.size, content.min, content.max) : content.min,
    };
  }

  function clampSize(size, content) {
    return clamp(size, content.min, content.max);
  }

  root.ToyLogic = {
    W: W,
    H: H,
    STEP: STEP,
    MAX_MARKS: MAX_MARKS,
    MODES: MODES,
    SYMMETRIES: SYMMETRIES,
    moveCursor: moveCursor,
    snapToCell: snapToCell,
    makeMark: makeMark,
    addMark: addMark,
    undo: undo,
    reflect: reflect,
    symmetryCount: symmetryCount,
    describe: describe,
    cursorInk: cursorInk,
    validMark: validMark,
    normalizeContent: normalizeContent,
    restoreState: restoreState,
    clampSize: clampSize,
  };
})(globalThis);
