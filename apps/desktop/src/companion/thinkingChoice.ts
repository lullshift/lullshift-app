// Remembers whether the user opened or closed a Thinking section, in memory only, so the choice
// made on the live block survives the swap to the stored reply (a different component, a
// different key) and leaving and returning to the conversation. Nothing is persisted.

const choices = new Map<string, boolean>();
const MAX = 200;

export const liveKey = (requestId: string) => `req:${requestId}`;
export const storedKey = (messageId: number) => `msg:${messageId}`;

export function getChoice(key: string): boolean | null {
  return choices.get(key) ?? null;
}

export function setChoice(key: string, open: boolean): void {
  choices.delete(key);
  choices.set(key, open);
  while (choices.size > MAX) {
    const oldest = choices.keys().next().value;
    if (oldest === undefined) break;
    choices.delete(oldest);
  }
}

/** The request ended with this stored reply: its live choice now belongs to the reply. */
export function carryChoice(requestId: string, messageId: number): void {
  const c = choices.get(liveKey(requestId));
  if (c !== undefined) setChoice(storedKey(messageId), c);
  choices.delete(liveKey(requestId));
}

/** For tests. */
export function resetChoices(): void {
  choices.clear();
}
