// {{name}}: a Lullshift activity. The host owns pausing: it asks for your state, then unloads
// this page. Keep all progress in `state`, report it with autosave after each change, and rebuild
// everything from `restore` in onInit.
(function () {
  "use strict";
  var GOAL = 10;
  var state = { pebbles: 0 };

  function $(id) {
    return document.getElementById(id);
  }

  function render() {
    var cairn = $("cairn");
    cairn.textContent = "";
    for (var i = 0; i < state.pebbles; i++) {
      var p = document.createElement("span");
      p.className = "pebble";
      cairn.appendChild(p);
    }
    $("count").textContent =
      state.pebbles === 0
        ? "No pebbles yet."
        : state.pebbles >= GOAL
          ? "The cairn is " + GOAL + " high. Done."
          : state.pebbles + (state.pebbles === 1 ? " pebble." : " pebbles.");
    $("add").disabled = state.pebbles >= GOAL;
    $("remove").disabled = state.pebbles === 0;
    // Shown by the host while paused: one to four short label/value pairs.
    LullshiftBridge.summary([{ label: "Pebbles", value: state.pebbles + " of " + GOAL }]);
  }

  function change(next) {
    state = next;
    render();
    LullshiftBridge.autosave(state);
  }

  // Restored state is untrusted too: check it before use.
  function fromRestore(r) {
    var n = r && typeof r.pebbles === "number" ? Math.floor(r.pebbles) : 0;
    return { pebbles: Math.max(0, Math.min(GOAL, n)) };
  }

  function setup(init) {
    document.documentElement.dataset.theme = init.theme;
    document.documentElement.classList.toggle("reduced", init.reducedMotion);
    state = fromRestore(init.restore);
    render();
    $("add").addEventListener("click", function () {
      if (state.pebbles < GOAL) change({ pebbles: state.pebbles + 1 });
    });
    $("remove").addEventListener("click", function () {
      if (state.pebbles > 0) change({ pebbles: state.pebbles - 1 });
    });
    $("restart").addEventListener("click", function () {
      change({ pebbles: 0 });
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "+" || e.key === "=") $("add").click();
      else if (e.key === "-") $("remove").click();
    });
    $("add").focus();
  }

  LullshiftBridge.start({
    onInit: setup,
    saveState: function () {
      return state;
    },
  });
})();
