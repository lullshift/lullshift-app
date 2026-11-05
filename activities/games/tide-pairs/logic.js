// Pure rules for Tide Pairs, a memory pairs game. No DOM access. Attaches its API to
// globalThis.TidePairsLogic. Needs the card kit (globalThis.LullshiftKits.card) loaded first,
// for the seeded generator, the custom deck and the daily seed.
//
// A game state is plain JSON:
//   { seed, mode: "daily"|"random", pairs: 8|12, order: number[], matched: boolean[],
//     open: number[], turns }
// `order[i]` is the index into SYMBOLS of the card at slot i. `open` holds the slots turned up
// and not yet matched (0, 1 or 2 of them). A turn is two cards turned.
(function (root) {
  "use strict";
  var SYMBOLS = [
    "starfish",
    "conch",
    "crab",
    "jellyfish",
    "seahorse",
    "clam",
    "urchin",
    "octopus",
    "turtle",
    "dolphin",
    "angelfish",
    "coral",
  ];
  var SIZES = [8, 12];
  var COLUMNS = { 8: 4, 12: 6 };
  var SALT = "tide-pairs";

  function kit() {
    return root.LullshiftKits.card;
  }

  // The 2 * pairs cards, shuffled: each symbol twice. Uses the kit's custom deck.
  function shuffledOrder(seed, pairs) {
    var K = kit();
    var specs = [];
    for (var i = 0; i < pairs; i++) {
      specs.push({ suit: SYMBOLS[i], rank: 1, name: SYMBOLS[i] });
      specs.push({ suit: SYMBOLS[i], rank: 2, name: SYMBOLS[i] });
    }
    var deck = K.shuffleDeck(K.customDeck(specs), K.createRng(seed));
    return deck.map(function (card) {
      return SYMBOLS.indexOf(card.suit);
    });
  }

  function deal(seed, pairs, mode) {
    var n = SIZES.indexOf(pairs) >= 0 ? pairs : 8;
    var matched = [];
    for (var i = 0; i < n * 2; i++) matched.push(false);
    return {
      seed: seed,
      mode: mode === "daily" ? "daily" : "random",
      pairs: n,
      order: shuffledOrder(seed, n),
      matched: matched,
      open: [],
      turns: 0,
    };
  }

  function dailySeed(date) {
    return kit().dailySeed(date, SALT);
  }

  function copyState(s) {
    return {
      seed: s.seed,
      mode: s.mode,
      pairs: s.pairs,
      order: s.order.slice(),
      matched: s.matched.slice(),
      open: s.open.slice(),
      turns: s.turns,
    };
  }

  function symbolAt(s, i) {
    return SYMBOLS[s.order[i]];
  }

  function isUp(s, i) {
    return s.matched[i] === true || s.open.indexOf(i) >= 0;
  }

  function pairsFound(s) {
    var n = 0;
    for (var i = 0; i < s.matched.length; i++) if (s.matched[i]) n += 1;
    return n / 2;
  }

  function isWon(s) {
    return pairsFound(s) === s.pairs;
  }

  // True when two cards are up and do not match: they wait to be turned back.
  function needsSettle(s) {
    return s.open.length === 2;
  }

  // Turns a card up. Returns { state, result } where result is one of:
  //   "first"    the first card of a turn is up
  //   "match"    the second card matched the first: both stay up
  //   "mismatch" the second card did not match: both stay up until `settle`
  //   "ignored"  the slot is already up, out of range, or two unmatched cards are still showing
  function flip(s, i) {
    if (!Number.isInteger(i) || i < 0 || i >= s.order.length || isUp(s, i) || s.open.length >= 2) {
      return { state: s, result: "ignored" };
    }
    var n = copyState(s);
    if (n.open.length === 0) {
      n.open.push(i);
      return { state: n, result: "first" };
    }
    var other = n.open[0];
    n.turns += 1;
    if (n.order[other] === n.order[i]) {
      n.matched[other] = true;
      n.matched[i] = true;
      n.open = [];
      return { state: n, result: "match" };
    }
    n.open.push(i);
    return { state: n, result: "mismatch" };
  }

  // Turns back a pair that did not match. A single face-up card (mid-turn) is left alone.
  function settle(s) {
    if (s.open.length < 2) return s;
    var n = copyState(s);
    n.open = [];
    return n;
  }

  // Validates saved state: returns a clean state, or null when it cannot be trusted.
  function restore(raw) {
    if (!raw || typeof raw !== "object") return null;
    if (SIZES.indexOf(raw.pairs) < 0) return null;
    var pairs = raw.pairs;
    var size = pairs * 2;
    if (!Number.isInteger(raw.seed) || raw.seed < 0 || raw.seed > 4294967295) return null;
    if (!Array.isArray(raw.order) || raw.order.length !== size) return null;
    var counts = {};
    for (var i = 0; i < size; i++) {
      var v = raw.order[i];
      if (!Number.isInteger(v) || v < 0 || v >= pairs) return null;
      counts[v] = (counts[v] || 0) + 1;
    }
    for (var k = 0; k < pairs; k++) if (counts[k] !== 2) return null;
    if (!Array.isArray(raw.matched) || raw.matched.length !== size) return null;
    var matched = raw.matched.map(function (m) {
      return m === true;
    });
    // Matched cards must come in complete pairs.
    for (var j = 0; j < size; j++) {
      if (!matched[j]) continue;
      var twin = -1;
      for (var t = 0; t < size; t++) if (t !== j && raw.order[t] === raw.order[j]) twin = t;
      if (!matched[twin]) return null;
    }
    if (!Array.isArray(raw.open) || raw.open.length > 2) return null;
    for (var o = 0; o < raw.open.length; o++) {
      var idx = raw.open[o];
      if (!Number.isInteger(idx) || idx < 0 || idx >= size || matched[idx]) return null;
      if (raw.open.indexOf(idx) !== o) return null;
    }
    if (!Number.isInteger(raw.turns) || raw.turns < 0 || raw.turns > 100000) return null;
    return {
      seed: raw.seed,
      mode: raw.mode === "daily" ? "daily" : "random",
      pairs: pairs,
      order: raw.order.slice(),
      matched: matched,
      open: raw.open.slice(),
      turns: raw.turns,
    };
  }

  function snapshot(s) {
    return copyState(s);
  }

  root.TidePairsLogic = {
    SYMBOLS: SYMBOLS,
    SIZES: SIZES,
    COLUMNS: COLUMNS,
    deal: deal,
    dailySeed: dailySeed,
    shuffledOrder: shuffledOrder,
    symbolAt: symbolAt,
    isUp: isUp,
    pairsFound: pairsFound,
    isWon: isWon,
    needsSettle: needsSettle,
    flip: flip,
    settle: settle,
    restore: restore,
    snapshot: snapshot,
  };
})(globalThis);
