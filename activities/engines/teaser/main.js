// Brain Teasers, engine lullshift.teaser@1 (ADR-016). The pack arrives as `content` in onInit.
(function () {
  "use strict";
  var L = globalThis.TeaserLogic;
  var pack = L.SAMPLE;
  var state = L.initialState(pack.teasers.length);

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    progress: document.getElementById("progress"),
    card: document.getElementById("card"),
    kind: document.getElementById("kind"),
    question: document.getElementById("question"),
    hint: document.getElementById("hint"),
    answerBox: document.getElementById("answerBox"),
    answer: document.getElementById("answer"),
    explanation: document.getElementById("explanation"),
    status: document.getElementById("status"),
    hintBtn: document.getElementById("hintBtn"),
    reveal: document.getElementById("reveal"),
    got: document.getElementById("got"),
    missed: document.getElementById("missed"),
    next: document.getElementById("next"),
    again: document.getElementById("again"),
  };

  function total() {
    return pack.teasers.length;
  }

  function render() {
    el.title.textContent = pack.title;
    el.intro.hidden = !pack.intro;
    el.intro.textContent = pack.intro || "";
    if (L.atEnd(state, total())) {
      var t = L.tally(state);
      el.progress.textContent = "All " + total() + " teasers seen";
      el.kind.hidden = true;
      el.question.textContent = "That's the last one";
      el.hint.hidden = true;
      el.answerBox.hidden = true;
      el.status.textContent =
        t.got + t.missed > 0
          ? "You marked " +
            t.got +
            " as got it and " +
            t.missed +
            " as not this time. Only you can see this, and it is not kept."
          : "Start again whenever you like.";
      el.hintBtn.hidden = true;
      el.reveal.hidden = true;
      el.got.hidden = true;
      el.missed.hidden = true;
      el.next.hidden = true;
      el.again.hidden = false;
      return;
    }
    var teaser = pack.teasers[state.i];
    el.progress.textContent = "Teaser " + (state.i + 1) + " of " + total();
    el.kind.hidden = teaser.kind === undefined;
    el.kind.textContent = teaser.kind ? L.KIND_LABELS[teaser.kind] : "";
    el.question.textContent = teaser.question;
    el.hint.hidden = !state.hint;
    el.hint.textContent = state.hint ? "Hint: " + teaser.hint : "";
    el.answerBox.hidden = !state.revealed;
    el.answer.textContent = state.revealed ? "Answer: " + teaser.answer : "";
    el.explanation.textContent = state.revealed ? teaser.explanation : "";
    el.status.textContent = "";
    el.hintBtn.hidden = state.revealed || teaser.hint === undefined;
    el.hintBtn.disabled = state.hint;
    el.reveal.hidden = state.revealed;
    el.got.hidden = !state.revealed;
    el.missed.hidden = !state.revealed;
    el.got.setAttribute("aria-pressed", String(state.marks[state.i] === true));
    el.missed.setAttribute("aria-pressed", String(state.marks[state.i] === false));
    el.next.hidden = false;
    el.next.textContent = state.i + 1 >= total() ? "Finish" : "Next";
    el.again.hidden = true;
  }

  function report() {
    render();
    LullshiftBridge.autosave(state);
    var end = L.atEnd(state, total());
    LullshiftBridge.summary([
      {
        label: end ? "Teasers" : "Teaser",
        value: end ? "finished" : state.i + 1 + " of " + total(),
      },
    ]);
  }

  function update(next) {
    if (next === state) return false;
    state = next;
    report();
    return true;
  }

  el.hintBtn.addEventListener("click", function () {
    if (update(L.showHint(state, total(), pack.teasers))) {
      el.status.textContent = "Hint: " + pack.teasers[state.i].hint;
      el.card.focus();
    }
  });
  el.reveal.addEventListener("click", function () {
    if (update(L.reveal(state, total()))) {
      var t = pack.teasers[state.i];
      // Written after render(), which blanks the region, so the message is not erased.
      el.status.textContent = "Answer: " + t.answer + ". " + t.explanation;
      el.got.focus();
    }
  });
  el.got.addEventListener("click", function () {
    update(L.mark(state, total(), true));
  });
  el.missed.addEventListener("click", function () {
    update(L.mark(state, total(), false));
  });
  el.next.addEventListener("click", function () {
    update(L.go(state, total(), state.i + 1));
    (L.atEnd(state, total()) ? el.again : el.card).focus();
  });
  el.again.addEventListener("click", function () {
    state = L.again(total());
    report();
    el.card.focus();
  });
  el.card.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowRight") {
      e.preventDefault();
      update(L.go(state, total(), state.i + 1));
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      update(L.go(state, total(), state.i - 1));
    }
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      pack = L.validTeasers(init.content) ? init.content : L.SAMPLE;
      state = L.restoreState(pack.teasers, init.restore) || L.initialState(total());
      report();
    },
    saveState: function () {
      return state;
    },
  });
  render();
})();
