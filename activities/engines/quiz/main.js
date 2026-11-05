// Quick Quiz, engine lullshift.quiz@1 (ADR-016). The quiz arrives as `content` in onInit.
(function () {
  "use strict";
  var L = globalThis.QuizLogic;
  var quiz = L.SAMPLE;
  var steps = [];
  var state = null;
  var choiceButtons = [];

  var el = {
    title: document.getElementById("title"),
    intro: document.getElementById("intro"),
    progress: document.getElementById("progress"),
    card: document.getElementById("card"),
    prompt: document.getElementById("prompt"),
    choices: document.getElementById("choices"),
    feedback: document.getElementById("feedback"),
    next: document.getElementById("next"),
    again: document.getElementById("again"),
  };

  function newSeed() {
    return Math.floor(Math.random() * 4294967296);
  }

  function finished() {
    return state.i >= steps.length;
  }

  function begin(seed) {
    steps = L.plan(quiz, seed);
    state = L.initialState(seed);
  }

  function tag(button, text) {
    var span = document.createElement("span");
    span.className = "tag";
    span.textContent = text;
    button.appendChild(span);
  }

  function renderChoices() {
    el.choices.textContent = "";
    choiceButtons = [];
    if (finished()) return;
    var step = steps[state.i];
    var q = quiz.questions[step.q];
    var right = L.correctPosition(quiz, steps, state.i);
    var answered = state.picked !== null;
    for (var s = 0; s < step.choices.length; s++) {
      (function (shown) {
        var b = document.createElement("button");
        b.type = "button";
        b.className = "choice";
        var key = document.createElement("span");
        key.className = "key";
        key.textContent = String(shown + 1) + ".";
        b.appendChild(key);
        var label = document.createElement("span");
        label.className = "label";
        label.textContent = q.choices[step.choices[shown]];
        b.appendChild(label);
        if (answered) {
          b.setAttribute("aria-disabled", "true");
          if (shown === right) {
            b.classList.add("right");
            tag(b, "Right answer");
          } else if (shown === state.picked) {
            b.classList.add("picked");
            tag(b, "Your pick");
          }
        }
        b.addEventListener("click", function () {
          choose(shown);
        });
        el.choices.appendChild(b);
        choiceButtons.push(b);
      })(s);
    }
  }

  function render() {
    el.title.textContent = quiz.title;
    el.intro.hidden = !quiz.intro;
    el.intro.textContent = quiz.intro || "";
    var total = steps.length;
    if (finished()) {
      var got = L.score(state);
      el.progress.textContent = "All " + total + " questions done";
      el.prompt.textContent = "You got " + got + " of " + total + ".";
      el.feedback.textContent =
        L.closing(got, total) + " Only you can see this score, and it is not kept.";
      el.choices.textContent = "";
      choiceButtons = [];
      el.next.hidden = true;
      el.again.hidden = false;
      return;
    }
    var step = steps[state.i];
    var q = quiz.questions[step.q];
    el.progress.textContent = "Question " + (state.i + 1) + " of " + total;
    el.prompt.textContent = q.prompt;
    renderChoices();
    if (state.picked !== null) {
      var ok = state.results[state.results.length - 1] === true;
      el.feedback.textContent = (ok ? "That's right. " : "Not this time. ") + q.explanation;
      el.next.hidden = false;
      el.next.textContent = state.i + 1 >= total ? "See my score" : "Next question";
    } else {
      el.feedback.textContent = "";
      el.next.hidden = true;
    }
    el.again.hidden = true;
  }

  function report() {
    render();
    LullshiftBridge.autosave(state);
    LullshiftBridge.summary([
      {
        label: finished() ? "Quiz" : "Question",
        value: finished() ? "finished" : state.i + 1 + " of " + steps.length,
      },
    ]);
  }

  function choose(shown) {
    var next = L.pick(quiz, steps, state, shown);
    if (!next) return;
    state = next;
    report();
    el.next.focus();
  }

  function goNext() {
    var next = L.advance(steps, state);
    if (!next) return;
    state = next;
    report();
    (finished() ? el.again : choiceButtons[0] || el.prompt).focus();
  }

  el.next.addEventListener("click", goNext);
  el.again.addEventListener("click", function () {
    begin(newSeed());
    report();
    if (choiceButtons[0]) choiceButtons[0].focus();
  });

  document.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (!state || finished()) return;
    if (!el.card.contains(document.activeElement)) return;
    if (/^[1-5]$/.test(e.key)) {
      var n = Number(e.key) - 1;
      if (n < choiceButtons.length) {
        e.preventDefault();
        choose(n);
      }
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      var at = choiceButtons.indexOf(document.activeElement);
      if (at < 0) return;
      e.preventDefault();
      var to = (at + (e.key === "ArrowDown" ? 1 : choiceButtons.length - 1)) % choiceButtons.length;
      choiceButtons[to].focus();
    }
  });

  LullshiftBridge.start({
    onInit: function (init) {
      document.documentElement.setAttribute("data-theme", init.theme);
      document.documentElement.classList.toggle("reduced", init.reducedMotion);
      quiz = L.validQuiz(init.content) ? init.content : L.SAMPLE;
      var restored = L.restoreState(quiz, init.restore);
      if (restored) {
        steps = L.plan(quiz, restored.seed);
        state = restored;
      } else {
        begin(newSeed());
      }
      report();
    },
    saveState: function () {
      return state;
    },
  });
  begin(newSeed());
  render();
})();
