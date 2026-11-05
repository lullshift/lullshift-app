// Away Prompts, engine lullshift.away@1 (ADR-016). The prompts arrive as `content` in onInit.
// All rules live in logic.js; this file only draws and listens.
(function () {
  "use strict";
  var L = globalThis.AwayLogic;
  var data = L.SAMPLE;
  var isSample = true;
  var state = L.newState(data.prompts.length, false, Math.random);
  var hostRunning = true;
  var lastTick = 0;
  var lastSave = 0;
  var lastSummary = "";
  var timer = null;

  function $(id) {
    return document.getElementById(id);
  }
  var el = {
    title: $("title"),
    intro: $("intro"),
    clock: $("clock"),
    card: $("card"),
    category: $("category"),
    prompt: $("prompt"),
    progress: $("progress"),
    status: $("status"),
    prev: $("prev"),
    next: $("next"),
    back: $("back"),
  };

  function render() {
    el.title.textContent = data.title;
    el.intro.hidden = !data.intro;
    el.intro.textContent = data.intro || "";
    renderClock();
    var p = data.prompts[L.current(state)];
    var many = data.prompts.length > 1;
    el.prev.hidden = !many;
    el.next.hidden = !many;
    el.prev.disabled = state.back;
    el.next.disabled = state.back;
    el.back.textContent = state.back ? "Away again" : "Back at desk";
    if (state.back) {
      el.category.hidden = true;
      el.category.textContent = "";
      el.prompt.textContent = data.backAtDesk || "Welcome back.";
    } else {
      var label = p.category ? L.CATEGORY_LABELS[p.category] : "";
      el.category.hidden = !label;
      el.category.textContent = label;
      el.prompt.textContent = p.text;
    }
    el.progress.textContent = state.back
      ? ""
      : "Idea " + (state.pos + 1) + " of " + data.prompts.length;
  }

  function renderClock() {
    el.clock.textContent = L.clock(state.elapsed);
  }

  function summaryNow() {
    var items = [
      { label: "Time away", value: L.awayText(state.elapsed) },
      { label: "Ideas seen", value: L.seenCount(state) },
    ];
    lastSummary = items[0].value + "|" + items[1].value;
    LullshiftBridge.summary(items);
  }

  function save() {
    LullshiftBridge.autosave(L.snapshot(state));
  }

  function report() {
    render();
    save();
    summaryNow();
  }

  function tick() {
    var now = Date.now();
    var dt = (now - lastTick) / 1000;
    lastTick = now;
    if (!hostRunning || state.back) return;
    state = L.addTime(state, dt, true);
    renderClock();
    if (now - lastSave > 5000) {
      lastSave = now;
      save();
      if (L.awayText(state.elapsed) + "|" + L.seenCount(state) !== lastSummary) summaryNow();
    }
  }

  function go(dir) {
    if (state.back || data.prompts.length < 2) return;
    state = L.move(state, dir);
    report();
  }

  el.next.addEventListener("click", function () {
    go(1);
    el.card.focus();
  });
  el.prev.addEventListener("click", function () {
    go(-1);
    el.card.focus();
  });
  el.back.addEventListener("click", function () {
    state = L.setBack(state, !state.back);
    el.status.textContent = state.back ? "Clock stopped." : "Clock running again.";
    lastTick = Date.now();
    report();
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      go(-1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      go(1);
    }
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme === "dark" ? "dark" : "light");
      document.documentElement.classList.toggle("reduced", !!init.reducedMotion);
      if (L.validData(init.content)) {
        data = init.content;
        isSample = false;
      } else {
        data = L.SAMPLE;
        isSample = true;
      }
      state = L.restore(
        isSample ? null : init.restore,
        data.prompts.length,
        data.shuffle === true,
        Math.random,
      );
      hostRunning = true;
      lastTick = Date.now();
      lastSave = lastTick;
      if (!timer) timer = setInterval(tick, 500);
      report();
    },
    saveState: function () {
      return L.snapshot(state);
    },
    onPause: function () {
      hostRunning = false;
      save();
      summaryNow();
    },
    onResume: function () {
      hostRunning = true;
      lastTick = Date.now();
    },
  });
  render();
})();
