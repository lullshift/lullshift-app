// Pure rules for Reflection Prompts (lullshift.reflection@1). No DOM, no timers.
(function (root) {
  "use strict";
  var MAX_NOTE = 1000;

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // The host validates packs; this is a cheap second look so a bad pack never breaks the page.
  function validPack(c) {
    if (!c || typeof c !== "object" || !isText(c.title, 80)) return false;
    if (c.intro !== undefined && !isText(c.intro, 300)) return false;
    if (c.allowNotes !== undefined && typeof c.allowNotes !== "boolean") return false;
    if (!Array.isArray(c.prompts) || c.prompts.length < 1 || c.prompts.length > 30) return false;
    for (var i = 0; i < c.prompts.length; i++) {
      var p = c.prompts[i];
      if (!p || typeof p !== "object" || !isText(p.text, 200)) return false;
      if (p.followUp !== undefined && !isText(p.followUp, 200)) return false;
    }
    return true;
  }

  function clampIndex(index, count) {
    var i = typeof index === "number" && isFinite(index) ? Math.floor(index) : 0;
    return Math.max(0, Math.min(count - 1, i));
  }

  /** One string per prompt, each at most 1000 characters. Anything else becomes "". */
  function cleanNotes(notes, count) {
    var out = [];
    for (var i = 0; i < count; i++) {
      var v = Array.isArray(notes) ? notes[i] : "";
      out.push(typeof v === "string" ? v.slice(0, MAX_NOTE) : "");
    }
    return out;
  }

  function setNote(notes, index, text) {
    var next = notes.slice();
    next[index] = String(text).slice(0, MAX_NOTE);
    return next;
  }

  function hasNotes(notes) {
    for (var i = 0; i < notes.length; i++) if (notes[i].length > 0) return true;
    return false;
  }

  function clearNotes(count) {
    return cleanNotes([], count);
  }

  root.ReflectionLogic = {
    MAX_NOTE: MAX_NOTE,
    validPack: validPack,
    clampIndex: clampIndex,
    cleanNotes: cleanNotes,
    setNote: setNote,
    hasNotes: hasNotes,
    clearNotes: clearNotes,
  };
})(typeof globalThis !== "undefined" ? globalThis : this);
