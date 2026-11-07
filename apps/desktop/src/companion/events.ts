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
