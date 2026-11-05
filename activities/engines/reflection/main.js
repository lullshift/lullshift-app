// Reflection Prompts, engine lullshift.reflection@1 (ADR-016). The pack arrives as `content`.
// Notes live only in the saved state of this activity, on this device. They are never put in
// the paused-view summary or the live region.
(function () {
  "use strict";
  var L = globalThis.ReflectionLogic;

  var SAMPLE = {
    title: "Reflection Prompts",
    intro: "A sample prompt. Install a prompt pack to fill this in.",
    allowNotes: true,
    prompts: [
      { text: "What is one small thing that went well today?", followUp: "Take a breath with it." },
      { text: "What would make the next hour a little easier?" },
    ],
  };

  var pack = SAMPLE;
  var index = 0;
  var notes = L.clearNotes(SAMPLE.prompts.length);
  var saveTimer = null;

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    card: document.getElementById("card"),
    prompt: document.getElementById("prompt"),
    followup: document.getElementById("followup"),
    area: document.getElementById("notes-area"),
    notes: document.getElementById("notes"),
    progress: document.getElementById("progress"),
    prev: document.getElementById("prev"),
    next: document.getElementById("next"),
    clear: document.getElementById("clear"),
  };

  function state() {
    return { index: index, notes: pack.allowNotes ? notes : [] };
  }

  function render() {
    var total = pack.prompts.length;
    var p = pack.prompts[index];
    el.title.textContent = pack.title;
    el.intro.hidden = !pack.intro;
    el.intro.textContent = pack.intro || "";
    el.prompt.textContent = p.text;
    el.followup.hidden = !p.followUp;
    el.followup.textContent = p.followUp || "";
    el.progress.textContent = "Prompt " + (index + 1) + " of " + total;
    el.prev.disabled = index === 0;
    el.next.disabled = index === total - 1;
    el.area.hidden = !pack.allowNotes;
    el.clear.hidden = !pack.allowNotes;
    el.clear.disabled = !L.hasNotes(notes);
    if (pack.allowNotes && el.notes.value !== notes[index]) el.notes.value = notes[index];
  }

  function save() {
    if (saveTimer !== null) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    LullshiftBridge.autosave(state());
  }

  function report() {
    render();
    save();
    LullshiftBridge.summary([{ label: "Prompt", value: index + 1 + " of " + pack.prompts.length }]);
  }

  function go(to) {
    var target = L.clampIndex(to, pack.prompts.length);
    if (target === index) return;
    index = target;
    report();
    el.card.focus();
  }

  el.prev.addEventListener("click", function () {
    go(index - 1);
  });
  el.next.addEventListener("click", function () {
    go(index + 1);
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      go(index - 1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      go(index + 1);
    }
  });
  el.notes.addEventListener("input", function () {
    notes = L.setNote(notes, index, el.notes.value);
    el.clear.disabled = !L.hasNotes(notes);
    if (saveTimer !== null) clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 400);
  });
  el.notes.addEventListener("blur", save);
  el.clear.addEventListener("click", function () {
    notes = L.clearNotes(pack.prompts.length);
    el.notes.value = "";
    report();
    el.notes.focus();
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      pack = L.validPack(init.content) ? init.content : SAMPLE;
      index = 0;
      notes = L.clearNotes(pack.prompts.length);
      var r = init.restore;
      if (r && typeof r === "object") {
        index = L.clampIndex(r.index, pack.prompts.length);
        if (pack.allowNotes) notes = L.cleanNotes(r.notes, pack.prompts.length);
      }
      report();
    },
    saveState: function () {
      return state();
    },
  });
  render();
})();
