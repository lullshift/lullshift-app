// Movement Routine, engine lullshift.movement@1 (ADR-016). The routine arrives as `content` in
// onInit. All rules live in logic.js; this file only draws and listens.
(function () {
  "use strict";
  var L = globalThis.MovementLogic;
  var data = L.SAMPLE;
  var isSample = true;
  var stages = L.buildStages(data);
  var state = L.newState(stages, data.moves.length);
  var running = true; // false while the host has paused us
  var lastTick = 0;
  var timer = null;
  var lastSave = 0;
  var wasUp = false;

  function $(id) {
    return document.getElementById(id);
  }
  var el = {
    title: $("title"),
    intro: $("intro"),
    card: $("card"),
    name: $("move-name"),
    meta: $("meta"),
    steps: $("steps"),
    timer: $("timer"),
    progress: $("progress"),
    status: $("status"),
    back: $("back"),
    toggle: $("toggle"),
    next: $("next"),
    skip: $("skip"),
    again: $("again"),
    wayout: $("wayout"),
  };
  var ART = ["seated", "standing", "either", "rest"];

  function showArt(which) {
    for (var i = 0; i < ART.length; i++) $("art-" + ART[i]).hidden = ART[i] !== which;
  }

  function setSteps(lines) {
    el.steps.textContent = "";
    for (var i = 0; i < lines.length; i++) {
      var li = document.createElement("li");
      li.textContent = lines[i];
      el.steps.appendChild(li);
    }
  }

  function render() {
    el.title.textContent = data.title;
    el.intro.hidden = !data.intro;
    el.intro.textContent = data.intro || "";
    el.wayout.textContent = data.wayOut;
    var tally = L.tally(state);
    if (L.finished(state, stages)) {
      showArt("rest");
      el.name.textContent = "That's the routine";
      el.meta.textContent = tally.done + " of " + tally.total + " moves done";
      setSteps([data.closing || "Thanks for moving a little."]);
      el.timer.hidden = true;
      el.progress.textContent = tally.skipped > 0 ? tally.skipped + " skipped, which is fine" : "";
      el.back.hidden = false;
      el.toggle.hidden = true;
      el.next.hidden = true;
      el.skip.hidden = true;
      el.again.hidden = false;
      return;
    }
    var st = stages[state.stage];
    var move = data.moves[st.move];
    el.again.hidden = true;
    el.back.hidden = false;
    el.back.disabled = state.stage === 0;
    el.next.hidden = false;
    el.skip.hidden = false;
    if (st.type === "rest") {
      showArt("rest");
      el.name.textContent = "Rest";
      el.meta.textContent = "Take a breath";
      setSteps(["Next up: " + move.name]);
      el.progress.textContent = "Move " + (st.move + 1) + " of " + data.moves.length + " is next";
    } else {
      showArt(move.position || "either");
      el.name.textContent = move.name;
      var bits = [L.amount(move, st), L.POSITION_LABELS[move.position || "either"]];
      var side = L.sideLabel(move, st);
      if (side) bits.unshift(side);
      el.meta.textContent = bits.join(". ");
      setSteps(move.steps);
      el.progress.textContent = "Move " + (st.move + 1) + " of " + data.moves.length;
    }
    renderTimer();
    el.skip.textContent = st.type === "rest" ? "Skip rest" : "Skip";
  }

  function renderTimer() {
    if (L.finished(state, stages)) return;
    var timed = typeof state.remaining === "number";
    el.timer.hidden = !timed;
    el.toggle.hidden = !timed;
    if (!timed) return;
    var up = L.timeUp(state);
    el.timer.textContent = up ? "Time is up" : L.clock(state.remaining);
    var hadFocus = document.activeElement === el.toggle;
    el.toggle.disabled = up;
    if (up && hadFocus) el.next.focus();
    el.toggle.textContent = state.running
      ? "Pause timer"
      : state.remaining < stages[state.stage].seconds
        ? "Continue"
        : "Start timer";
  }

  function announce(text) {
    el.status.textContent = text;
  }

  function summaryNow() {
    var t = L.tally(state);
    var items = [{ label: "Moves done", value: t.done + " of " + t.total }];
    if (t.skipped > 0) items.push({ label: "Skipped", value: t.skipped });
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

  function apply(fn) {
    state = fn(state, stages);
    wasUp = L.timeUp(state);
    announce("");
    report();
  }

  function tick() {
    var now = Date.now();
    var dt = Math.max(0, (now - lastTick) / 1000);
    lastTick = now;
    if (!running || !state.running) return;
    state = L.tick(state, dt);
    renderTimer();
    if (L.timeUp(state) && !wasUp) {
      wasUp = true;
      announce(
        stages[state.stage].type === "rest"
          ? "Rest is over. Press Next when you are ready."
          : "Time is up. Press Next when you are ready.",
      );
      report();
    } else if (now - lastSave > 5000) {
      lastSave = now;
      save();
    }
  }

  el.next.addEventListener("click", function () {
    apply(L.next);
    el.card.focus();
  });
  el.skip.addEventListener("click", function () {
    apply(L.skip);
    el.card.focus();
  });
  el.back.addEventListener("click", function () {
    apply(L.back);
    el.card.focus();
  });
  el.again.addEventListener("click", function () {
    state = L.restart(stages, data.moves.length);
    wasUp = false;
    announce("");
    report();
    el.card.focus();
  });
  el.toggle.addEventListener("click", function () {
    state = L.toggle(state);
    lastTick = Date.now();
    announce(state.running ? "Timer running." : "Timer paused.");
    renderTimer();
    save();
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      if (!el.back.disabled) apply(L.back);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      if (!el.next.hidden) apply(L.next);
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
      stages = L.buildStages(data);
      state = L.restore(isSample ? null : init.restore, stages, data.moves.length);
      wasUp = L.timeUp(state);
      running = true;
      lastTick = Date.now();
      lastSave = lastTick;
      if (!timer) timer = setInterval(tick, 250);
      report();
    },
    saveState: function () {
      return L.snapshot(state);
    },
    onPause: function () {
      running = false;
      renderTimer();
      save();
      summaryNow();
    },
    onResume: function () {
      running = true;
      lastTick = Date.now();
    },
  });
  render();
})();
