// Types for the global `LullshiftBridge` that `lullshift-bridge-v1.js` defines. Reference this
// file from a TypeScript activity: /// <reference path="./lullshift-bridge-v1.d.ts" />

declare namespace LullshiftBridge {
  type Theme = "light" | "dark";

  interface InitInfo<State = unknown, Content = unknown> {
    /** The last saved state, or null on a fresh start. */
    restore: State | null;
    reducedMotion: boolean;
    theme: Theme;
    /** Pack data when this activity is an engine running a content pack; otherwise null. */
    content: Content | null;
  }

  interface Handlers<State = unknown, Content = unknown> {
    onInit?(info: InitInfo<State, Content>): void;
    /** Any JSON value except null. Returning nothing keeps the last known state. */
    saveState?(): State | undefined;
    onPause?(): void;
    onResume?(): void;
  }

  interface SummaryItem {
    /** Plain text, 1 to 24 characters. */
    label: string;
    /** Plain text of 1 to 24 characters, or a finite number. */
    value: string | number;
  }

  /** Call once your page is ready to receive `onInit`. */
  function start<State = unknown, Content = unknown>(handlers: Handlers<State, Content>): void;
  /** Reports state now. Call it after every meaningful change. */
  function autosave(state: unknown): void;
  /** One to four stats shown while the activity is paused. */
  function summary(items: SummaryItem[]): void;

  /** Needs the "activity.storage" permission. Values are strings of at most 64 KiB. */
  namespace storage {
    function get(key: string): Promise<string | null>;
    function set(key: string, value: string | number | boolean): Promise<null>;
    function remove(key: string): Promise<null>;
  }
}

interface Window {
  readonly LullshiftBridge: typeof LullshiftBridge;
}
