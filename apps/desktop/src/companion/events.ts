// Tells an open Companion view that providers or models changed in Settings (decision 36), so it
// reads them again. Carries no data.

const listeners = new Set<() => void>();

export function onProvidersChanged(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function providersChanged(): void {
  for (const fn of [...listeners]) fn();
}

// A notice's "Open" action asks the app to show one conversation. The app switches to Companion
// and the Companion view selects the thread. Carries the thread id only.

const openListeners = new Set<(threadId: number) => void>();

export function onOpenThread(fn: (threadId: number) => void): () => void {
  openListeners.add(fn);
  return () => {
    openListeners.delete(fn);
  };
}

export function openThread(threadId: number): void {
  for (const fn of [...openListeners]) fn(threadId);
}
