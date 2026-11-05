// Harvest Pairs rules. No DOM. Classic script: attaches HarvestLogic to globalThis.
//
// Cards are integers 0..51: rank = id % 13 + 1 (1 to 13), suit = floor(id / 13) (0 to 3).
// The field is a stacked triangle of seven rows, 28 slots, row r holding r + 1 cards. Slot (r, c)
// is index r * (r + 1) / 2 + c and is covered by slots (r + 1, c) and (r + 1, c + 1).
// An "entry" names a card you can act on: 0..27 is a field slot, 28 is the top of the waste pile.
// Moves are small number arrays so the history stays compact in saved state:
//   [0]               draw: top of the draw pile goes to the waste pile
//   [1]               turn over: the waste pile becomes the draw pile again, in the same order
//   [2, a, b, ca, cb] remove entry a with entry b (or alone when b is -1); ca and cb are the cards
(function (root) {
  "use strict";

  var ROWS = 7;
  var SLOTS = 28;
  var WASTE = 28;
  var STOCK_N = 24;
  var LOG_LIMIT = 20000;
  var SEARCH_BUDGET = 60000;
  var ATTEMPTS = 8;
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

  var ROW_OF = [];
  var COL_OF = [];
  var KIDS = [];
  (function () {
    for (var r = 0; r < ROWS; r++) {
      for (var c = 0; c <= r; c++) {
        var s = (r * (r + 1)) / 2 + c;
        ROW_OF[s] = r;
        COL_OF[s] = c;
        KIDS[s] =
          r < ROWS - 1 ? [((r + 1) * (r + 2)) / 2 + c, ((r + 1) * (r + 2)) / 2 + c + 1] : [];
      }
    }
  })();

  function kit() {
    return root.LullshiftKits && root.LullshiftKits.card;
  }

  function rank(id) {
    return (id % 13) + 1;
  }
  function suit(id) {
    return Math.floor(id / 13);
  }
  function isCard(id) {
    return Number.isInteger(id) && id >= 0 && id < 52;
  }
  function cardName(id) {
    return RANK_NAMES[rank(id)] + " of " + SUITS[suit(id)];
  }
  function rankText(id) {
    var r = rank(id);
    return r === 1 ? "A" : r === 11 ? "J" : r === 12 ? "Q" : r === 13 ? "K" : String(r);
  }

  function dateSeed(d) {
    return kit().dailySeed(d, "harvest-pairs");
  }

  // ---- State ----------------------------------------------------------------------------------

  function isUncovered(pyr, s) {
    if (pyr[s] < 0) return false;
    var k = KIDS[s];
    return k.length === 0 || (pyr[k[0]] < 0 && pyr[k[1]] < 0);
  }

  function copy(st) {
    return {
      seed: st.seed,
      attempt: st.attempt,
      verified: st.verified,
      pyr: st.pyr.slice(),
      stock: st.stock.slice(),
      waste: st.waste.slice(),
      log: st.log.map(function (m) {
        return m.slice();
      }),
    };
  }

  function cardAt(st, entry) {
    if (entry === WASTE) return st.waste.length ? st.waste[st.waste.length - 1] : -1;
    if (!Number.isInteger(entry) || entry < 0 || entry >= SLOTS) return -1;
    return isUncovered(st.pyr, entry) ? st.pyr[entry] : -1;
  }

  function cleared(st) {
    for (var i = 0; i < SLOTS; i++) if (st.pyr[i] >= 0) return false;
    return true;
  }

  function left(st) {
    var n = 0;
    for (var i = 0; i < SLOTS; i++) if (st.pyr[i] >= 0) n += 1;
    return n;
  }

  // Why two entries cannot be removed together; null when they can.
  function whyNot(st, a, b) {
    var ca = cardAt(st, a);
    if (ca < 0) return "That card is not free.";
    if (b === -1) return rank(ca) === 13 ? null : "Only a King can go alone.";
    if (a === b) return "Pick a different card.";
    var cb = cardAt(st, b);
    if (cb < 0) return "That card is not free.";
    var sum = rank(ca) + rank(cb);
    return sum === 13 ? null : "Those add up to " + sum + ", not 13.";
  }

  // Applies a removal; returns the move to record, or null when it is not allowed.
  function remove(st, a, b) {
    if (whyNot(st, a, b) !== null) return null;
    var ca = cardAt(st, a);
    var cb = b === -1 ? -1 : cardAt(st, b);
    var move = [2, a, b, ca, cb];
    run(st, move);
    pushLog(st, move);
    return move;
  }

  function draw(st) {
    if (st.stock.length === 0) return null;
    var move = [0];
    run(st, move);
    pushLog(st, move);
    return move;
  }

  function turnOver(st) {
    if (st.stock.length !== 0 || st.waste.length === 0) return null;
    var move = [1];
    run(st, move);
    pushLog(st, move);
    return move;
  }

  function pushLog(st, move) {
    st.log.push(move);
    if (st.log.length > LOG_LIMIT) st.log.splice(0, st.log.length - LOG_LIMIT);
  }

  // Performs a move that has already been checked.
  function run(st, m) {
    if (m[0] === 0) {
      st.waste.push(st.stock.pop());
    } else if (m[0] === 1) {
      st.stock = st.waste.slice().reverse();
      st.waste = [];
    } else {
      takeOut(st, m[1]);
      if (m[2] !== -1) takeOut(st, m[2]);
    }
  }

  function takeOut(st, entry) {
    if (entry === WASTE) st.waste.pop();
    else st.pyr[entry] = -1;
  }

  // Takes back the last logged move. Returns false (changing nothing) when the log does not fit the
  // state, which only happens with corrupted saved data.
  function undo(st) {
    if (st.log.length === 0) return false;
    var m = st.log[st.log.length - 1];
    if (m[0] === 0) {
      if (st.waste.length === 0) return false;
      st.stock.push(st.waste.pop());
    } else if (m[0] === 1) {
      if (st.waste.length !== 0) return false;
      st.waste = st.stock.slice().reverse();
      st.stock = [];
    } else {
      var entries =
        m[2] === -1
          ? [[m[1], m[3]]]
          : [
              [m[1], m[3]],
              [m[2], m[4]],
            ];
      var i;
      for (i = 0; i < entries.length; i++) {
        var e = entries[i][0];
        if (e !== WASTE && st.pyr[e] !== -1) return false;
      }
      for (i = 0; i < entries.length; i++) {
        if (entries[i][0] === WASTE) st.waste.push(entries[i][1]);
        else st.pyr[entries[i][0]] = entries[i][1];
      }
    }
    st.log.pop();
    return true;
  }

  // ---- Solver ---------------------------------------------------------------------------------
  // The draw pile and waste pile form one fixed cycle of cards: the waste is always the first k
  // cards still in the cycle and the draw pile the rest, turning over restarts the cycle. So a
  // position is (cards left on the field, cards left in the cycle, k) and a depth-first search with
  // a visited set is exact. Returns { result: "solved" | "dead" | "unknown", moves }.

  function search(st, budget) {
    var cycle = st.waste.concat(st.stock.slice().reverse());
    var n = cycle.length;
    var mask0 = 0;
    var i;
    for (i = 0; i < SLOTS; i++) if (st.pyr[i] >= 0) mask0 |= 1 << i;
    var rem0 = n === 0 ? 0 : n >= 31 ? -1 : (1 << n) - 1;
    var seen = new Set();
    var nodes = 0;
    var cut = false;
    var path = [];
    var val = [];
    for (i = 0; i < SLOTS; i++) val[i] = st.pyr[i] >= 0 ? rank(st.pyr[i]) : 0;

    function nthBit(rem, k) {
      // Index of the k-th (1-based) set bit.
      var seenBits = 0;
      for (var b = 0; b < n; b++) {
        if ((rem >> b) & 1) {
          seenBits += 1;
          if (seenBits === k) return b;
        }
      }
      return -1;
    }

    function dfs(mask, rem, k) {
      if (mask === 0) return true;
      if (cut) return false;
      var key = mask + "," + rem + "," + k;
      if (seen.has(key)) return false;
      seen.add(key);
      nodes += 1;
      if (nodes > budget) {
        cut = true;
        return false;
      }
      var open = [];
      var s;
      for (s = 0; s < SLOTS; s++) {
        if (((mask >> s) & 1) === 0) continue;
        var kids = KIDS[s];
        if (kids.length === 0 || (((mask >> kids[0]) & 1) === 0 && ((mask >> kids[1]) & 1) === 0)) {
          open.push(s);
        }
      }
      var wIdx = k > 0 ? nthBit(rem, k) : -1;
      var wRank = wIdx >= 0 ? rank(cycle[wIdx]) : 0;
      var a;
      var b;
      for (a = 0; a < open.length; a++) {
        if (val[open[a]] === 13) {
          path.push([2, open[a], -1]);
          if (dfs(mask & ~(1 << open[a]), rem, k)) return true;
          path.pop();
        }
      }
      for (a = 0; a < open.length; a++) {
        for (b = a + 1; b < open.length; b++) {
          if (val[open[a]] + val[open[b]] === 13) {
            path.push([2, open[a], open[b]]);
            if (dfs(mask & ~(1 << open[a]) & ~(1 << open[b]), rem, k)) return true;
            path.pop();
          }
        }
      }
      if (wIdx >= 0) {
        if (wRank === 13) {
          path.push([2, WASTE, -1]);
          if (dfs(mask, rem & ~(1 << wIdx), k - 1)) return true;
          path.pop();
        } else {
          for (a = 0; a < open.length; a++) {
            if (val[open[a]] + wRank === 13) {
              path.push([2, open[a], WASTE]);
              if (dfs(mask & ~(1 << open[a]), rem & ~(1 << wIdx), k - 1)) return true;
              path.pop();
            }
          }
        }
      }
      var total = 0;
      for (var q = 0; q < n; q++) total += (rem >> q) & 1;
      if (k < total) {
        path.push([0]);
        if (dfs(mask, rem, k + 1)) return true;
        path.pop();
      } else if (k > 0) {
        path.push([1]);
        if (dfs(mask, rem, 0)) return true;
        path.pop();
      }
      return false;
    }

    var ok = dfs(mask0, rem0, st.waste.length);
    if (ok) return { result: "solved", moves: path.map(copyMove) };
    return { result: cut ? "unknown" : "dead", moves: [] };
  }

  function copyMove(m) {
    return m.slice();
  }

  // Plays the moves of a search result on a copy, with full rule checks, as proof of a solution.
  function replay(st, moves) {
    var c = copy(st);
    for (var i = 0; i < moves.length; i++) {
      var m = moves[i];
      var ok = m[0] === 0 ? draw(c) : m[0] === 1 ? turnOver(c) : remove(c, m[1], m[2]);
      if (!ok) return null;
    }
    return c;
  }

  // ---- Deals ----------------------------------------------------------------------------------

  function plainDeal(seed, attempt) {
    var rng = kit().createRng(((seed >>> 0) + Math.imul(attempt, 0x9e3779b1)) >>> 0);
    var ids = [];
    for (var i = 0; i < 52; i++) ids.push(i);
    ids = rng.shuffle(ids);
    return {
      seed: seed >>> 0,
      attempt: attempt,
      verified: false,
      pyr: ids.slice(0, SLOTS),
      stock: ids.slice(SLOTS),
      waste: [],
      log: [],
    };
  }

  // The deal for a seed. It tries up to ATTEMPTS shuffles and keeps the first one the solver can
  // clear; "verified" says whether that happened. Same seed, same deal, always.
  function newGame(seed, budget) {
    var limit = budget || SEARCH_BUDGET;
    var first = null;
    for (var a = 0; a < ATTEMPTS; a++) {
      var st = plainDeal(seed, a);
      if (first === null) first = st;
      if (search(st, limit).result === "solved") {
        st.verified = true;
        return st;
      }
    }
    return first;
  }

  // Clearable from here? "unknown" when the search budget ran out.
  function outlook(st, budget) {
    if (cleared(st)) return { result: "solved", moves: [] };
    return search(st, budget || 20000);
  }

  // Checks saved data and returns a state built from it, or null.
  function validate(raw) {
    if (!raw || typeof raw !== "object") return null;
    if (!Array.isArray(raw.pyr) || raw.pyr.length !== SLOTS) return null;
    if (!Array.isArray(raw.stock) || !Array.isArray(raw.waste) || !Array.isArray(raw.log)) {
      return null;
    }
    if (raw.stock.length + raw.waste.length > STOCK_N) return null;
    var seen = {};
    var all = raw.pyr.concat(raw.stock, raw.waste);
    for (var i = 0; i < all.length; i++) {
      var id = all[i];
      if (i < SLOTS && id === -1) continue;
      if (!isCard(id) || seen[id]) return null;
      seen[id] = true;
    }
    if (raw.log.length > LOG_LIMIT) return null;
    var log = [];
    for (var j = 0; j < raw.log.length; j++) {
      var m = raw.log[j];
      if (!Array.isArray(m)) return null;
      if (m[0] === 0 || m[0] === 1) {
        if (m.length !== 1) return null;
      } else if (m[0] === 2) {
        if (m.length !== 5) return null;
        var okEntry = function (e) {
          return Number.isInteger(e) && e >= 0 && e <= WASTE;
        };
        if (!okEntry(m[1]) || !(m[2] === -1 || okEntry(m[2]))) return null;
        if (!isCard(m[3]) || !(m[4] === -1 || isCard(m[4]))) return null;
        if ((m[2] === -1) !== (m[4] === -1)) return null;
      } else return null;
      log.push(m.slice());
    }
    if (!Number.isInteger(raw.seed) || !Number.isInteger(raw.attempt)) return null;
    return {
      seed: raw.seed >>> 0,
      attempt: raw.attempt,
      verified: raw.verified === true,
      pyr: raw.pyr.slice(),
      stock: raw.stock.slice(),
      waste: raw.waste.slice(),
      log: log,
    };
  }

  root.HarvestLogic = {
    ROWS: ROWS,
    SLOTS: SLOTS,
    WASTE: WASTE,
    STOCK_N: STOCK_N,
    SUITS: SUITS,
    SUIT_ONE: SUIT_ONE,
    ROW_OF: ROW_OF,
    COL_OF: COL_OF,
    KIDS: KIDS,
    rank: rank,
    suit: suit,
    isCard: isCard,
    cardName: cardName,
    rankText: rankText,
    dateSeed: dateSeed,
    isUncovered: isUncovered,
    cardAt: cardAt,
    cleared: cleared,
    left: left,
    whyNot: whyNot,
    remove: remove,
    draw: draw,
    turnOver: turnOver,
    undo: undo,
    copy: copy,
    search: search,
    replay: replay,
    plainDeal: plainDeal,
    newGame: newGame,
    outlook: outlook,
    validate: validate,
  };
})(globalThis);
