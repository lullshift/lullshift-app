// Pure rules for Ridge Walk. No DOM access. Attaches its API to globalThis.RidgeWalkLogic.
// Needs the card kit (globalThis.LullshiftKits.card) loaded first, for the seeded generator
// and the 52-card deck.
//
// A card is an integer 0..51: suit = floor(n / 13) (0 spades, 1 hearts, 2 diamonds, 3 clubs),
// rank = n % 13 + 1 (1 is Ace, 13 is King).
//
// Rules: 35 cards are dealt face up into 7 ridges of 5. One more card starts the path and
// the other 16 are the stock. The top (last) card of a ridge can be played onto the path
// when its rank is exactly one higher or one lower than the path's top card. Ranks wrap: a
// King can be followed by an Ace and an Ace by a King (`wrap` is true unless a test passes false).
// The player may also flip the next stock card onto the path at any time.
// A deal is just the seed's shuffle, so it is instant. It is not checked to be clearable;
// `solve` is kept for tests and tools.
(function (root) {
  "use strict";
  var RIDGES = 7;
  var RIDGE_SIZE = 5;
  var FLIP = 7; // move code for "flip the next stock card"
  var SUIT_NAMES = ["spades", "hearts", "diamonds", "clubs"];
  var SUIT_SYMBOLS = ["♠", "♥", "♦", "♣"];
  var RANK_TEXT = ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"];
  var RANK_NAMES = ["Ace", "2", "3", "4", "5", "6", "7", "8", "9", "10", "Jack", "Queen", "King"];
  var SOLVER_NODE_LIMIT = 30000;

  function kit() {
    return root.LullshiftKits.card;
  }

  function suitOf(card) {
    return Math.floor(card / 13);
  }
  function rankOf(card) {
    return (card % 13) + 1;
  }
  function suitName(card) {
    return SUIT_NAMES[suitOf(card)];
  }
  function suitSymbol(card) {
    return SUIT_SYMBOLS[suitOf(card)];
  }
  function isRed(card) {
    var s = suitOf(card);
    return s === 1 || s === 2;
  }
  function cardText(card) {
    return RANK_TEXT[rankOf(card) - 1] + suitSymbol(card);
  }
  function cardName(card) {
    return RANK_NAMES[rankOf(card) - 1] + " of " + suitName(card);
  }

  // True when a card of rank `a` can be played on a card of rank `b`.
  function adjacent(a, b, wrap) {
    var d = Math.abs(a - b);
    return d === 1 || (wrap !== false && d === 12);
  }

  function copyState(s) {
    return {
      seed: s.seed,
      ridges: s.ridges.map(function (r) {
        return r.slice();
      }),
      stock: s.stock.slice(),
      path: s.path.slice(),
      moves: s.moves.slice(),
      wrap: s.wrap !== false,
    };
  }

  // Lays out a shuffled 52-card order as a game state.
  function layOut(order, seed, wrap) {
    var ridges = [];
    for (var r = 0; r < RIDGES; r++) {
      ridges.push(order.slice(r * RIDGE_SIZE, (r + 1) * RIDGE_SIZE));
    }
    var used = RIDGES * RIDGE_SIZE;
    return {
      seed: seed,
      ridges: ridges,
      path: [order[used]],
      stock: order.slice(used + 1),
      moves: [],
      wrap: wrap !== false,
    };
  }

  function shuffledOrder(rng) {
    var K = kit();
    var ids = K.buildDeck();
    var shuffled = K.shuffleDeck(ids, rng);
    var order = [];
    for (var i = 0; i < shuffled.length; i++) {
      var c = shuffled[i];
      order.push(Math.max(0, K.STANDARD_SUITS.indexOf(c.suit)) * 13 + (c.rank - 1));
    }
    return order;
  }

  function pathTop(s) {
    return s.path.length ? s.path[s.path.length - 1] : null;
  }

  function canPlay(s, ridge) {
    if (!(ridge >= 0 && ridge < RIDGES)) return false;
    var r = s.ridges[ridge];
    var top = pathTop(s);
    if (!r.length || top === null) return false;
    return adjacent(rankOf(r[r.length - 1]), rankOf(top), s.wrap);
  }

  function canFlip(s) {
    return s.stock.length > 0;
  }

  // Applies a move code (0..6 plays that ridge, 7 flips). Returns a new state, or null when
  // the move is not allowed. The input is never changed.
  function apply(s, move) {
    var n;
    if (move === FLIP) {
      if (!canFlip(s)) return null;
      n = copyState(s);
      n.path.push(n.stock.pop());
    } else {
      if (!canPlay(s, move)) return null;
      n = copyState(s);
      n.path.push(n.ridges[move].pop());
    }
    n.moves.push(move);
    return n;
  }

  // Takes back the last move. Returns a new state, or null when there is nothing to undo.
  function undo(s) {
    if (!s.moves.length) return null;
    var n = copyState(s);
    var move = n.moves.pop();
    var card = n.path.pop();
    if (move === FLIP) n.stock.push(card);
    else n.ridges[move].push(card);
    return n;
  }

  function cardsLeft(s) {
    var n = 0;
    for (var i = 0; i < s.ridges.length; i++) n += s.ridges[i].length;
    return n;
  }
  function isWon(s) {
    return cardsLeft(s) === 0;
  }
  function hasPlay(s) {
    for (var i = 0; i < RIDGES; i++) if (canPlay(s, i)) return true;
    return false;
  }
  // No play, no flip: the path ends here (and the ridges are not clear).
  function isStuck(s) {
    return !isWon(s) && !hasPlay(s) && !canFlip(s);
  }

  // Searches for a way to clear every ridge. Depth-first, trying plays before flips, with a
  // memo of dead states. Returns an array of move codes, or null if none was found within the
  // node limit (then the deal is simply not used).
  function solve(s, limit) {
    var budget = typeof limit === "number" ? limit : SOLVER_NODE_LIMIT;
    var heights = s.ridges.map(function (r) {
      return r.length;
    });
    var stockLeft = s.stock.length;
    var stockStart = s.stock.slice(); // top is the end
    var ridges = s.ridges;
    var wrap = s.wrap;
    var dead = Object.create(null);
    var moves = [];
    var path = pathTop(s);

    function key(top) {
      return heights.join("") + "," + stockLeft + "," + top;
    }

    function dfs(top, left) {
      if (left === 0) return true;
      if (budget-- <= 0) return false;
      var k = key(top);
      if (dead[k]) return false;
      for (var r = 0; r < RIDGES; r++) {
        var h = heights[r];
        if (h === 0) continue;
        var card = ridges[r][h - 1];
        if (!adjacent(rankOf(card), rankOf(top), wrap)) continue;
        heights[r] = h - 1;
        moves.push(r);
        if (dfs(card, left - 1)) return true;
        moves.pop();
        heights[r] = h;
      }
      if (stockLeft > 0) {
        var flipped = stockStart[stockLeft - 1];
        stockLeft -= 1;
        moves.push(FLIP);
        if (dfs(flipped, left)) return true;
        moves.pop();
        stockLeft += 1;
      }
      dead[k] = true;
      return false;
    }

    return path !== null && dfs(path, cardsLeft(s)) ? moves.slice() : null;
  }

  // The deal for a seed: always the same, and instant (no solver run). Returns the starting state.
  function deal(seed, wrap) {
    return layOut(shuffledOrder(kit().createRng(seed)), seed, wrap);
  }

  // Checks a saved state: well-formed, with every card of the deck exactly once, and a move
  // list that replays from the deal to exactly this position. Returns a clean state or null.
  function restore(raw) {
    if (!raw || typeof raw !== "object") return null;
    if (!Number.isInteger(raw.seed) || raw.seed < 0 || raw.seed > 4294967295) return null;
    if (!Array.isArray(raw.moves) || raw.moves.length > 200) return null;
    for (var i = 0; i < raw.moves.length; i++) {
      var m = raw.moves[i];
      if (!Number.isInteger(m) || m < 0 || m > FLIP) return null;
    }
    // Older saves (before the wrap rule) were dealt with a solver check and may not replay;
    // those fail the layout check below and the game starts a fresh deal.
    var s = deal(raw.seed);
    for (var j = 0; j < raw.moves.length; j++) {
      s = apply(s, raw.moves[j]);
      if (s === null) return null;
    }
    // The saved layout must match the replay exactly (guards against hand-edited state).
    if (
      JSON.stringify([s.ridges, s.stock, s.path]) !==
      JSON.stringify([raw.ridges, raw.stock, raw.path])
    ) {
      return null;
    }
    return s;
  }

  function snapshot(s) {
    return copyState(s);
  }

  root.RidgeWalkLogic = {
    RIDGES: RIDGES,
    RIDGE_SIZE: RIDGE_SIZE,
    FLIP: FLIP,
    suitOf: suitOf,
    rankOf: rankOf,
    suitName: suitName,
    suitSymbol: suitSymbol,
    isRed: isRed,
    cardText: cardText,
    cardName: cardName,
    adjacent: adjacent,
    deal: deal,
    layOut: layOut,
    shuffledOrder: shuffledOrder,
    pathTop: pathTop,
    canPlay: canPlay,
    canFlip: canFlip,
    apply: apply,
    undo: undo,
    cardsLeft: cardsLeft,
    isWon: isWon,
    hasPlay: hasPlay,
    isStuck: isStuck,
    solve: solve,
    restore: restore,
    snapshot: snapshot,
  };
})(globalThis);
