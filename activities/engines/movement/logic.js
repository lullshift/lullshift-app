// Pure rules for Movement Routine (engine lullshift.movement@1). No DOM, no clock.
// Attaches its API to globalThis.MovementLogic.
(function (root) {
  "use strict";

  var POSITION_LABELS = { seated: "Seated", standing: "Standing", either: "Seated or standing" };
  var SIDE_LABELS = ["Left side", "Right side"];

  var SAMPLE = {
    title: "A small sample routine",
    intro: "Install a movement pack to fill this in. Here is a taste of how it works.",
    moves: [
      {
        name: "Shoulder roll",
        steps: ["Let your arms hang loose.", "Roll both shoulders back and down, slowly."],
        seconds: 20,
        position: "either",
      },
      {
        name: "Gentle neck turn",
        steps: [
          "Look over one shoulder as far as is comfortable.",
          "Come back, then try the other side.",
        ],
        reps: 4,
        side: "left-right",
        position: "seated",
      },
    ],
    restSeconds: 5,
    closing: "That is the sample. A pack will bring more moves.",
    wayOut: "Stop if anything feels uncomfortable, and skip any move that does not suit you.",
  };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }
  function isInt(v, min, max) {
    return typeof v === "number" && isFinite(v) && Math.floor(v) === v && v >= min && v <= max;
  }

  // The host validates packs; this is a cheap second look so a bad routine never breaks the page.
  function validData(c) {
    if (!c || typeof c !== "object" || Array.isArray(c)) return false;
    if (!isText(c.title, 80) || !isText(c.wayOut, 200)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (c.closing !== undefined && !isText(c.closing, 200)) return false;
    if (c.restSeconds !== undefined && !isInt(c.restSeconds, 0, 30)) return false;
    if (!Array.isArray(c.moves) || c.moves.length < 1 || c.moves.length > 20) return false;
    for (var i = 0; i < c.moves.length; i++) {
      var m = c.moves[i];
      if (!m || typeof m !== "object" || !isText(m.name, 60)) return false;
      if (!Array.isArray(m.steps) || m.steps.length < 1 || m.steps.length > 4) return false;
      for (var j = 0; j < m.steps.length; j++) if (!isText(m.steps[j], 160)) return false;
      var hasSec = m.seconds !== undefined;
      var hasReps = m.reps !== undefined;
      if (hasSec === hasReps) return false;
      if (hasSec && !isInt(m.seconds, 10, 120)) return false;
      if (hasReps && !isInt(m.reps, 1, 20)) return false;
      if (m.side !== undefined && ["none", "left-right", "both"].indexOf(m.side) < 0) return false;
      if (
        m.position !== undefined &&
        !Object.prototype.hasOwnProperty.call(POSITION_LABELS, m.position)
      )
        return false;
    }
    return true;
  }

  // Flat list of stages. A left-right move is two stages (left, then right). A rest stage sits
  // between moves when restSeconds is above zero.
  function buildStages(data) {
    var stages = [];
    var rest = data.restSeconds || 0;
    for (var i = 0; i < data.moves.length; i++) {
      var m = data.moves[i];
      if (i > 0 && rest > 0)
        stages.push({ type: "rest", move: i, part: 0, parts: 1, seconds: rest });
      var parts = m.side === "left-right" ? 2 : 1;
      for (var p = 0; p < parts; p++) {
        stages.push({
          type: "move",
          move: i,
          part: p,
          parts: parts,
          seconds: typeof m.seconds === "number" ? m.seconds : null,
          reps: typeof m.reps === "number" ? m.reps : null,
        });
      }
    }
    return stages;
  }

  // status per move: 0 not yet, 1 done, 2 skipped.
  function newState(stages, moveCount) {
    var status = [];
    for (var i = 0; i < moveCount; i++) status.push(0);
    return enter({ stage: 0, remaining: null, running: false, status: status }, stages, 0);
  }

  function enter(state, stages, index) {
    var st = stages[index];
    var next = {
      stage: index,
      remaining: st && st.seconds !== null ? st.seconds : null,
      // A rest starts on its own; a timed move waits until the person starts it.
      running: !!st && st.type === "rest",
      status: state.status,
    };
    return next;
  }

  function finished(state, stages) {
    return state.stage >= stages.length;
  }

  function withStatus(status, move, value) {
    var out = status.slice();
    out[move] = value;
    return out;
  }

  // Next: finish this stage and go on. Finishing the last part of a move marks it done.
  function next(state, stages) {
    if (finished(state, stages)) return state;
    var st = stages[state.stage];
    var status = state.status;
    if (st.type === "move" && st.part === st.parts - 1) status = withStatus(status, st.move, 1);
    return enter({ status: status }, stages, state.stage + 1);
  }

  // Skip: leave the whole move (or the rest) without counting it as done.
  function skip(state, stages) {
    if (finished(state, stages)) return state;
    var st = stages[state.stage];
    var status = state.status;
    var i = state.stage + 1;
    if (st.type === "move") {
      status = withStatus(status, st.move, 2);
      while (i < stages.length && stages[i].type === "move" && stages[i].move === st.move) i++;
    }
    // Landing on a rest after a skip would only make the person wait, so pass over it.
    if (i < stages.length && stages[i].type === "rest") i++;
    return enter({ status: status }, stages, i);
  }

  function back(state, stages) {
    if (state.stage <= 0) return state;
    return enter({ status: state.status }, stages, state.stage - 1);
  }

  function restart(stages, moveCount) {
    return newState(stages, moveCount);
  }

  function timeUp(state) {
    return typeof state.remaining === "number" && state.remaining <= 0;
  }

  function toggle(state) {
    if (typeof state.remaining !== "number" || state.remaining <= 0) return state;
    return {
      stage: state.stage,
      remaining: state.remaining,
      running: !state.running,
      status: state.status,
    };
  }

  // Count down by dt seconds (never negative, never huge). Stops itself at zero.
  function tick(state, dt) {
    if (!state.running || typeof state.remaining !== "number") return state;
    var step = Math.min(2, Math.max(0, dt));
    var rem = Math.max(0, state.remaining - step);
    return {
      stage: state.stage,
      remaining: rem,
      running: rem > 0,
      status: state.status,
    };
  }

  function pause(state) {
    if (!state.running) return state;
    return { stage: state.stage, remaining: state.remaining, running: false, status: state.status };
  }

  // The saved shape never says "running": after a restore the person starts the timer again.
  function snapshot(state) {
    return { v: 1, stage: state.stage, remaining: state.remaining, status: state.status.slice() };
  }

  function restore(raw, stages, moveCount) {
    var fresh = newState(stages, moveCount);
    if (!raw || typeof raw !== "object" || raw.v !== 1) return fresh;
    if (!isInt(raw.stage, 0, stages.length)) return fresh;
    if (!Array.isArray(raw.status) || raw.status.length !== moveCount) return fresh;
    var status = [];
    for (var i = 0; i < moveCount; i++) {
      var s = raw.status[i];
      if (s !== 0 && s !== 1 && s !== 2) return fresh;
      status.push(s);
    }
    var entered = enter({ status: status }, stages, raw.stage);
    var st = stages[raw.stage];
    if (st && st.seconds !== null) {
      if (typeof raw.remaining === "number" && isFinite(raw.remaining)) {
        entered.remaining = Math.max(0, Math.min(st.seconds, raw.remaining));
      }
    }
    entered.running = false;
    return entered;
  }

  function tally(state) {
    var done = 0;
    var skipped = 0;
    for (var i = 0; i < state.status.length; i++) {
      if (state.status[i] === 1) done++;
      else if (state.status[i] === 2) skipped++;
    }
    return { done: done, skipped: skipped, total: state.status.length };
  }

  function clock(seconds) {
    var s = Math.max(0, Math.ceil(seconds));
    var m = Math.floor(s / 60);
    var r = s % 60;
    return m + ":" + (r < 10 ? "0" : "") + r;
  }

  function amount(move, stage) {
    if (stage.seconds !== null) return stage.seconds + " seconds";
    return move.reps === 1 ? "1 time" : move.reps + " times";
  }

  function sideLabel(move, stage) {
    if (move.side === "left-right") return SIDE_LABELS[stage.part];
    if (move.side === "both") return "Both sides together";
    return "";
  }

  root.MovementLogic = {
    SAMPLE: SAMPLE,
    POSITION_LABELS: POSITION_LABELS,
    validData: validData,
    buildStages: buildStages,
    newState: newState,
    finished: finished,
    next: next,
    skip: skip,
    back: back,
    restart: restart,
    timeUp: timeUp,
    toggle: toggle,
    tick: tick,
    pause: pause,
    snapshot: snapshot,
    restore: restore,
    tally: tally,
    clock: clock,
    amount: amount,
    sideLabel: sideLabel,
  };
})(globalThis);
