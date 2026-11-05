// Clock Patience rules. No DOM. Classic script: attaches ClockLogic to globalThis.
//
// Thirteen piles of four face-down cards sit like a clock face: Ace at 1 o'clock up to Queen at
// 12 o'clock, Kings in the centre. Each turn takes the top face-down card of the current pile,
// puts it face up under the pile for its rank, and moves on to that pile. The game ends when the
// fourth King is turned. It is won when every card is face up by then.
//
// Pure chance: the whole game is decided by the deal, so a saved game is just { seed, turned } and
// restoring it replays the turns.
//
// Cards are integers 0..51: rank = id % 13 + 1, suit = floor(id / 13). Pile index = rank - 1, so
// pile 12 (Kings) is the centre.
(function (root) {
  "use strict";

  var PILES = 13;
  var CENTRE = 12;
  var CARDS = 52;
  var SUITS = ["Leaves", "Acorns", "Berries", "Shells"];
  var SUIT_ONE = ["Leaf", "Acorn", "Berry", "Shell"];
  var RANK_NAMES = [
    "",
    "Ace",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "Jack",
    "Queen",
    "King",
  ];

  function kit() {
    return root.LullshiftKits && root.LullshiftKits.card;
  }

  function rank(id) {
    return (id % 13) + 1;
  }
  function suit(id) {
    return Math.floor(id / 13);
  }
  function rankText(id) {
    var r = rank(id);
    return r === 1 ? "A" : r === 11 ? "J" : r === 12 ? "Q" : r === 13 ? "K" : String(r);
  }
  function cardName(id) {
    return RANK_NAMES[rank(id)] + " of " + SUITS[suit(id)];
  }
  function pileName(p) {
    if (p === CENTRE) return "King pile, centre";
    return RANK_NAMES[p + 1] + " pile, " + (p + 1) + " o'clock";
  }
  function dateSeed(d) {
    return kit().dailySeed(d, "clock-patience");
  }

  // A fresh deal: four face-down cards per pile, standing at the centre, nothing turned yet.
  function deal(seed) {
    var rng = kit().createRng(seed >>> 0);
    var ids = [];
    for (var i = 0; i < CARDS; i++) ids.push(i);
    ids = rng.shuffle(ids);
    var down = [];
    var up = [];
    for (var p = 0; p < PILES; p++) {
      down.push(ids.slice(p * 4, p * 4 + 4));
      up.push([]);
    }
    return {
      seed: seed >>> 0,
      down: down,
      up: up,
      cur: CENTRE,
      turned: 0,
      last: null,
      over: false,
    };
  }

  // One turn. Returns { card, to } or null when the game is already over.
  function step(st) {
    if (st.over) return null;
    var pile = st.down[st.cur];
    if (pile.length === 0) {
      st.over = true;
      return null;
    }
    var card = pile.pop();
    var to = rank(card) - 1;
    st.up[to].push(card);
    st.last = { card: card, from: st.cur, to: to };
    st.cur = to;
    st.turned += 1;
    if (st.down[st.cur].length === 0) st.over = true;
    return { card: card, to: to };
  }

  // The deal for a seed after `turned` turns (stops early if the game ends sooner).
  function play(seed, turned) {
    var st = deal(seed);
    for (var i = 0; i < turned && !st.over; i++) step(st);
    return st;
  }

  function finish(seed) {
    var st = deal(seed);
    while (!st.over) step(st);
    return st;
  }

  function faceDown(st) {
    var n = 0;
    for (var p = 0; p < PILES; p++) n += st.down[p].length;
    return n;
  }
  function pilesLeft(st) {
    var n = 0;
    for (var p = 0; p < PILES; p++) if (st.down[p].length > 0) n += 1;
    return n;
  }
  function won(st) {
    return st.over && faceDown(st) === 0;
  }

  // Plain-words result of a finished game.
  function resultText(st) {
    if (!st.over) return "";
    if (won(st)) return "Every pile is complete. A rare, lucky deal.";
    var left = pilesLeft(st);
    return (
      left +
      (left === 1 ? " pile" : " piles") +
      " left this time, with " +
      faceDown(st) +
      " cards still face down."
    );
  }

  // Checks saved data and returns { seed, turned, mode } or null.
  function validate(raw) {
    if (!raw || typeof raw !== "object") return null;
    if (!Number.isInteger(raw.seed) || raw.seed < 0 || raw.seed > 4294967295) return null;
    if (!Number.isInteger(raw.turned) || raw.turned < 0 || raw.turned > CARDS) return null;
    return { seed: raw.seed, turned: raw.turned, mode: raw.mode === "random" ? "random" : "daily" };
  }

  root.ClockLogic = {
    PILES: PILES,
    CENTRE: CENTRE,
    CARDS: CARDS,
    SUITS: SUITS,
    SUIT_ONE: SUIT_ONE,
    rank: rank,
    suit: suit,
    rankText: rankText,
    cardName: cardName,
    pileName: pileName,
    dateSeed: dateSeed,
    deal: deal,
    step: step,
    play: play,
    finish: finish,
    faceDown: faceDown,
    pilesLeft: pilesLeft,
    won: won,
    resultText: resultText,
    validate: validate,
  };
})(globalThis);
