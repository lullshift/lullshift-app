import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

// Preview images, as data: URLs read by Rust (ADR-012 additions). Cached for the app's life:
// bundled files don't change while it runs.
const cache = new Map<string, Promise<string | null>>();

function load(id: string): Promise<string | null> {
  let p = cache.get(id);
  if (!p) {
    p = invoke<string | null>("activity_preview", { id }).catch(() => null);
    cache.set(id, p);
  }
  return p;
}

/** The activity's preview image URL, or null while loading, if it has none, or on error. */
export function usePreview(id: string, hasPreview: boolean): string | null {
  const [url, setUrl] = useState<{ id: string; url: string | null } | null>(null);
  useEffect(() => {
    if (!hasPreview) return;
    let live = true;
    void load(id).then((u) => {
      if (live) setUrl({ id, url: u });
    });
    return () => {
      live = false;
    };
  }, [id, hasPreview]);
  return hasPreview && url?.id === id ? url.url : null;
}
