// Pure rules for Calm Cards (lullshift.cards@1). No DOM. Attaches its API to globalThis.CardsLogic.
(function (root) {
  "use strict";

  var LIMITS = {
    title: 80,
    intro: 300,
    front: 200,
    back: 400,
    maxCards: 60,
    minRound: 3,
  };

  function isText(v, max) {
    return typeof v === "string" && v.length > 0 && v.length <= max;
  }

  // The host validates packs; this is a cheap second look so a bad deck never breaks the page.
  function validDeck(c) {
    if (!c || typeof c !== "object" || !isText(c.title, LIMITS.title)) return false;
    if (c.intro !== undefined && !isText(c.intro, LIMITS.intro)) return false;
    if (
      c.roundSize !== undefined &&
      (!Number.isInteger(c.roundSize) ||
        c.roundSize < LIMITS.minRound ||
        c.roundSize > LIMITS.maxCards)
    )
      return false;
    if (!Array.isArray(c.cards) || c.cards.length < 1 || c.cards.length > LIMITS.maxCards)
      return false;
    for (var i = 0; i < c.cards.length; i++) {
      var k = c.cards[i];
      if (!k || typeof k !== "object" || !isText(k.front, LIMITS.front)) return false;
      if (k.back !== undefined && !isText(k.back, LIMITS.back)) return false;
    }
    return true;
  }

  // --- Rounds (ADR-028 section 10). With `roundSize`, each round shows that many cards drawn
  // with a seeded generator, preferring ones not shown in recent rounds, in the deck's written
  // order. The state then also carries `seed` and `seen` (card indexes from earlier rounds, least
  // recently shown first, each at most once, so never more than the card count). The round is
  // not saved: the seed and `seen` rebuild it exactly, so a resume shows the same round at the
  // same card. `index` counts within the round.

  function isSeed(s) {
    return Number.isInteger(s) && s >= 0 && s <= 4294967295;
  }

  // Small seeded generator (mulberry32), the same one Quick Quiz uses.
  function rng(seed) {
    var a = seed >>> 0;
    return function () {
      a = (a + 0x6d2b79f5) >>> 0;
      var t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // Cards per round, or 0 when a round is the whole deck. The schema can't say "at most the card
  // count", so a size at or above it is clamped to the whole deck, which plays exactly as a deck
  // without roundSize.
  function roundSizeOf(deck) {
    var k = deck.roundSize;
    return Number.isInteger(k) && k >= LIMITS.minRound && k < deck.cards.length ? k : 0;
  }

  // A saved `seen` list fits a deck of n cards: distinct whole indexes below n.
  function validSeen(raw, n) {
    if (!Array.isArray(raw) || raw.length > n) return false;
    var at = {};
    for (var i = 0; i < raw.length; i++) {
      var v = raw[i];
      if (!Number.isInteger(v) || v < 0 || v >= n || at[v]) return false;
      at[v] = true;
    }
    return true;
  }

  // k distinct indexes below n: first a seeded pick among those not in `seen`, then, if too few
  // are left, the least recently seen (the front of `seen`). Entries that don't fit are ignored.
  function draw(n, k, seen, rnd) {
    var at = {};
    var old = [];
    for (var s = 0; s < seen.length; s++) {
      var v = seen[s];
      if (Number.isInteger(v) && v >= 0 && v < n && !at[v]) {
        at[v] = true;
        old.push(v);
      }
    }
    var fresh = [];
    for (var i = 0; i < n; i++) if (!at[i]) fresh.push(i);
    var out = [];
    for (var j = 0; j < fresh.length && out.length < k; j++) {
      var r = j + Math.floor(rnd() * (fresh.length - j));
      var t = fresh[j];
      fresh[j] = fresh[r];
      fresh[r] = t;
      out.push(fresh[j]);
    }
    for (var o = 0; o < old.length && out.length < k; o++) out.push(old[o]);
    return out;
  }

  // `seen` after a round: the round's indexes move to the end (most recent), each kept once,
  // and the list is cut from the front to at most n entries.
  function remember(seen, round, n) {
    var inRound = {};
    for (var r = 0; r < round.length; r++) inRound[round[r]] = true;
    var out = [];
    for (var s = 0; s < seen.length; s++) if (!inRound[seen[s]]) out.push(seen[s]);
    for (var k = 0; k < round.length; k++) out.push(round[k]);
    return out.slice(Math.max(0, out.length - n));
  }

  // The card indexes of the state's round, in play order. Without roundSize, the whole deck.
  function roundOf(deck, s) {
    var n = deck.cards.length;
    var k = roundSizeOf(deck);
    if (!k) {
      var all = [];
      for (var i = 0; i < n; i++) all.push(i);
      return all;
    }
    return draw(n, k, s.seen || [], rng(s.seed)).sort(function (a, b) {
      return a - b;
    });
  }

  // A round's card objects.
  function roundCards(deck, s) {
    return roundOf(deck, s).map(function (i) {
      return deck.cards[i];
    });
  }

  function withRound(seed, seen, s) {
    s.seed = seed;
    s.seen = seen.slice();
    return s;
  }

  // A first round: nothing seen yet. Without roundSize, the plain state.
  function freshState(deck, seed) {
    var s = { index: 0, revealed: false };
    return roundSizeOf(deck) ? withRound(seed, [], s) : s;
  }

  // The next round after `s`: its cards now count as seen. Without roundSize, start again.
  function nextRound(deck, s, seed) {
    if (!roundSizeOf(deck)) return { index: 0, revealed: false };
    var seen = remember(s.seen || [], roundOf(deck, s), deck.cards.length);
    return withRound(seed, seen, { index: 0, revealed: false });
  }

  // A saved state for this deck. Without roundSize, any saved place is clamped into the deck, as
  // the engine always did. With it, the state must carry a seed and a fitting `seen` list, or
  // this returns null and a new round starts (an older save without them, for example).
  function restore(deck, r) {
    var round = null;
    if (roundSizeOf(deck)) {
      if (!r || typeof r !== "object" || !isSeed(r.seed) || !validSeen(r.seen, deck.cards.length))
        return null;
      round = withRound(r.seed, r.seen, {});
    }
    var cards = round ? roundCards(deck, round) : deck.cards;
    var index = 0;
    var revealed = false;
    if (r && typeof r === "object") {
      var i = typeof r.index === "number" && isFinite(r.index) ? Math.floor(r.index) : 0;
      index = Math.max(0, Math.min(cards.length, i));
      revealed =
        r.revealed === true && index < cards.length && typeof cards[index].back === "string";
    }
    var s = { index: index, revealed: revealed };
    return round ? withRound(round.seed, round.seen, s) : s;
  }

  root.CardsLogic = {
    LIMITS: LIMITS,
    validDeck: validDeck,
    roundSizeOf: roundSizeOf,
    draw: draw,
    remember: remember,
    roundOf: roundOf,
    roundCards: roundCards,
    freshState: freshState,
    nextRound: nextRound,
    restore: restore,
  };
})(globalThis);
