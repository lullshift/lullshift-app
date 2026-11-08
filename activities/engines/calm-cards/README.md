# Calm Cards

The engine `lullshift.cards@1`: a quiet deck of short cards read one at a time, each with an optional "Show more" back. A content pack supplies the deck (see `data.schema.json`); with no pack it says it needs one.

- `logic.js` holds the pure rules (`globalThis.CardsLogic`); `main.js` is the page.
- Optional `roundSize` (3 to 60, at most the number of cards): each round shows that many cards in the deck's written order, drawn with a seeded generator and preferring cards not shown in recent rounds; when too few are left unshown, the least recently shown fill the round. The saved state then also holds `seed` and `seen` (card indexes from earlier rounds, each at most once, so never more than the card count), which rebuild the same round after a pause or restart. A `roundSize` above the card count plays the whole deck, and `lullshift validate` warns about it. Without `roundSize` the deck plays exactly as before. The bundled `wonder-facts` deck (12 cards) sets `roundSize: 6`.
- Arrow keys move between cards when the card is focused; the place is saved through `saveState`.
- `lullshift validate .` and `lullshift simulate .` check it. License: Apache-2.0. Original text, no third-party assets.
