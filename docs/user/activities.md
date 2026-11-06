# Activities and pausing

Open **Activities** in the sidebar. It has three tabs:

- **Library:** games and breaks you can start.
- **Manage:** every activity, including ones you have turned off. Turn activities on or off, or uninstall ones you added from a marketplace.
- **Marketplaces:** Git repositories you have added. See [Marketplaces](marketplaces.md).

## What is bundled

Lullshift ships with original games (puzzles, card patience and gentle arcade games) and content packs for breathing, movement, guided pauses, quizzes, brain teasers, reflection prompts, creative toys, typing drills and away prompts. They are part of the app, work offline and update with the app. You can turn any of them off, but not remove them.

Each activity has **How to play** help: what it is, the goal, the steps and the keyboard controls.

## Pausing

Every activity can be paused at any moment without losing progress. Lullshift, not the activity, controls this.

An activity pauses when:

- an agent needs you (a "needs you" event arrives). The paused card says which agent needs you and offers **Go to Sessions** or **Keep playing**;
- you press **Pause**;
- you switch to another view or the window is hidden;
- you open **How to play**;
- the activity stops responding. Lullshift closes it so it can't slow anything down.

Activities save as you play and again when they pause. The paused card says "Your progress is saved." or, if the last save didn't arrive, "Your last saved progress is kept." Press **Resume** to carry on from there. **Close** saves and returns to the list.

An agent finishing a turn, failing or ending does not pause an activity. Only "needs you" does.

While paused, the card shows the time played and a few stats the activity reports, such as moves or progress.

## Keyboard, motion and theme

- Every activity can be played with the keyboard. **How to play** lists the keys.
- Activities follow your system's reduced-motion setting: animations and transitions are turned off when it is on.
- Activities follow your system's light or dark theme.

## Saved progress

Progress is stored in Lullshift's local database on your computer. It is never uploaded. When you uninstall an activity you added from a marketplace, you choose whether to keep its saved progress.

## Activities can't reach your computer

Each activity runs in an isolated frame with no network, no file access, no clipboard and no access to the rest of the app. An activity can only save its own state, and keep small values if it declares the `activity.storage` permission. Details for the curious: [the sandbox rules](../creators/sandbox.md).
