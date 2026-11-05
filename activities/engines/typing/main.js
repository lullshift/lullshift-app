// Typing Drills, engine lullshift.typing@1 (ADR-016). The pack arrives as `content`.
// Keystrokes never leave this page and the typed text is never saved: only counts are kept.
// The clock runs only while someone is typing a line and the activity is not paused.
(function () {
  "use strict";
  var L = globalThis.TypingLogic;

  var SAMPLE = {
    title: "Typing Drills",
    intro: "A sample drill. Install a drill pack to fill this in.",
    mode: "accuracy",
    lines: ["a calm hand finds the keys", "slow and steady is fine"],
  };

  var pack = SAMPLE;
  var index = 0;
  var stats = L.emptyStats();
  var bankedMs = 0;
  var runningSince = null;
  var paused = false;

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    target: document.getElementById("target"),
    targetText: document.getElementById("targetText"),
    typed: document.getElementById("typed"),
    now: document.getElementById("now"),
    stats: document.getElementById("stats"),
    progress: document.getElementById("progress"),
    restart: document.getElementById("restart"),
    skip: document.getElementById("skip"),
    again: document.getElementById("again"),
  };

  function finished() {
    return index >= pack.lines.length;
  }

  function state() {
    return {
      index: index,
      done: stats.done,
      chars: stats.chars,
      correct: stats.correct,
      activeMs: stats.activeMs,
    };
  }

  function statsText() {
    var acc = L.accuracy(stats);
    if (acc === null) return "Your accuracy shows up after your first line.";
    var text =
      "Accuracy " + acc + " percent over " + stats.done + (stats.done === 1 ? " line" : " lines");
    if (pack.mode === "flow") {
      var w = L.wpm(stats);
      if (w !== null)
        text += ". " + w + " words per minute, counting only the time you spend typing";
    }
    return text + ".";
  }

  // The line is exposed as plain text once per line, never per keystroke.
  function setTargetText(text) {
    if (el.targetText.textContent !== text) el.targetText.textContent = text;
  }

  function renderTarget(target, typed) {
    var marks = L.compare(target, typed);
    setTargetText("Line to type: " + target);
    el.target.textContent = "";
    var cur = Math.min(typed.length, target.length - 1);
    for (var i = 0; i < target.length; i++) {
      var s = document.createElement("span");
      s.className = "ch " + marks[i] + (i === cur ? " cur" : "");
      s.textContent = target.charAt(i);
      el.target.appendChild(s);
    }
    el.now.textContent = "Mistakes in this line so far: " + L.countOf(marks, "wrong") + ".";
  }

  function render() {
    el.title.textContent = pack.title;
    el.intro.hidden = !pack.intro;
    el.intro.textContent = pack.intro || "";
    el.stats.textContent = statsText();
    var over = finished();
    el.typed.hidden = over;
    el.now.hidden = over;
    el.restart.hidden = over;
    el.skip.hidden = over;
    el.again.hidden = !over;
    if (over) {
      el.target.textContent = "That's all the lines. Nicely done.";
      setTargetText("That's all the lines.");
      return;
    }
    var target = pack.lines[index];
    el.typed.maxLength = target.length;
    el.progress.textContent = "Line " + (index + 1) + " of " + pack.lines.length;
    renderTarget(target, el.typed.value);
  }

  function summary() {
    var items = [
      { label: "Lines", value: Math.min(index, pack.lines.length) + " of " + pack.lines.length },
    ];
    var acc = L.accuracy(stats);
    if (acc !== null) items.push({ label: "Accuracy", value: acc + "%" });
    var w = pack.mode === "flow" ? L.wpm(stats) : null;
    if (w !== null) items.push({ label: "Speed", value: w + " wpm" });
    LullshiftBridge.summary(items);
  }

  function resetLine() {
    el.typed.value = "";
    bankedMs = 0;
    runningSince = null;
  }

  function nextLine(announce) {
    index = Math.min(pack.lines.length, index + 1);
    resetLine();
    render();
    if (finished()) {
      el.progress.textContent = announce + " That's all the lines.";
      el.again.focus();
    } else {
      el.progress.textContent =
        announce + " Line " + (index + 1) + " of " + pack.lines.length + ".";
      el.typed.focus();
    }
    LullshiftBridge.autosave(state());
    summary();
  }

  function onType() {
    if (paused || finished()) return;
    var target = pack.lines[index];
    var typed = el.typed.value;
    if (runningSince === null && typed.length > 0) runningSince = Date.now();
    renderTarget(target, typed);
    if (!L.isComplete(target, typed)) return;
    var marks = L.compare(target, typed);
    var wrong = L.countOf(marks, "wrong");
    var ms = L.activeMs(bankedMs, runningSince, Date.now());
    stats = L.addLine(stats, target.length, target.length - wrong, ms);
    nextLine("Line done with " + L.mistakeText(wrong) + ".");
  }

  el.typed.addEventListener("input", onType);
  el.restart.addEventListener("click", function () {
    if (finished()) return;
    resetLine();
    render();
    el.progress.textContent =
      "Line " + (index + 1) + " of " + pack.lines.length + ", started over.";
    el.typed.focus();
  });
  el.skip.addEventListener("click", function () {
    if (finished()) return;
    nextLine("Line skipped.");
  });
  el.again.addEventListener("click", function () {
    index = 0;
    stats = L.emptyStats();
    resetLine();
    render();
    LullshiftBridge.autosave(state());
    summary();
    el.typed.focus();
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      pack = L.validPack(init.content) ? init.content : SAMPLE;
      var r = init.restore;
      index = 0;
      stats = L.emptyStats();
      if (r && typeof r === "object") {
        stats = L.cleanStats(r, pack.lines.length);
        var i = typeof r.index === "number" && isFinite(r.index) ? Math.floor(r.index) : 0;
        index = Math.max(0, Math.min(pack.lines.length, i));
      }
      paused = false;
      resetLine();
      render();
      LullshiftBridge.autosave(state());
      summary();
    },
    saveState: function () {
      return state();
    },
    onPause: function () {
      bankedMs = L.activeMs(bankedMs, runningSince, Date.now());
      runningSince = null;
      paused = true;
    },
    onResume: function () {
      paused = false;
      if (el.typed.value.length > 0 && !finished()) runningSince = Date.now();
    },
  });
  render();
})();
