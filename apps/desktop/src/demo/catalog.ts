// The bundled activities, read from the repository's manifests at dev time, shaped like the
// Rust `activities_list` answer. The files themselves are served by the dev-only
// `/demo-activities/` route in vite.config.ts.

import type { ActivityInfo } from "../activity/ActivityHost";

interface Manifest {
  id: string;
  kind: "app" | "content";
  name: string;
  description: string;
  category: string;
  version: string;
  entrypoint?: string;
  engine?: string;
  data?: string;
  preview?: string;
  permissions?: string[];
  help?: ActivityInfo["help"];
}

const manifests = import.meta.glob<Manifest>(
  [
    "../../../../activities/games/*/manifest.json",
    "../../../../activities/break/*/manifest.json",
    "../../../../activities/engines/*/manifest.json",
    "../../../../activities/content/*/*/manifest.json",
  ],
  { eager: true, import: "default" },
);

const dataFiles = import.meta.glob<unknown>("../../../../activities/content/*/*/*.json", {
  eager: true,
  import: "default",
});

const PREFIX = "../../../../activities/";

/** "../../../../activities/games/fog-garden/manifest.json" -> "games/fog-garden" */
const folderOf = (key: string) => key.slice(PREFIX.length, key.lastIndexOf("/"));

export interface DemoActivity {
  info: ActivityInfo;
  folder: string;
  previewUrl: string | null;
  content: unknown;
}

function build(): DemoActivity[] {
  const engineFolders = new Map<string, string>();
  for (const [key, m] of Object.entries(manifests)) {
    if (folderOf(key).startsWith("engines/")) {
      // "app.lullshift.engines.breathing" serves packs that name "lullshift.breathing@1".
      engineFolders.set(`lullshift.${m.id.split(".").pop()}`, folderOf(key));
    }
  }

  const out: DemoActivity[] = [];
  for (const [key, m] of Object.entries(manifests)) {
    const folder = folderOf(key);
    if (folder.startsWith("engines/")) continue;
    const engineFolder = m.engine ? engineFolders.get(m.engine.split("@")[0] ?? "") : undefined;
    if (m.engine && !engineFolder) continue;
    const runFolder = engineFolder ?? folder;
    const runEntry = engineFolder
      ? (Object.entries(manifests).find(([k]) => folderOf(k) === engineFolder)?.[1].entrypoint ??
        "index.html")
      : (m.entrypoint ?? "index.html");
    out.push({
      folder,
      previewUrl: m.preview ? `/demo-activities/${folder}/${m.preview}` : null,
      content: m.data ? (dataFiles[`${PREFIX}${folder}/${m.data}`] ?? null) : null,
      info: {
        id: m.id,
        name: m.name,
        description: m.description,
        category: m.category,
        version: m.version,
        url: `/demo-activities/${runFolder}/${runEntry}`,
        storage: (m.permissions ?? []).includes("activity.storage"),
        savedUtc: null,
        help: m.help ?? null,
        preview: Boolean(m.preview),
        playedSeconds: 0,
        engine: m.engine ?? null,
        source: { marketplaceId: "lullshift", builtin: true, commit: null },
      },
    });
  }
  // Games first, then breaks and packs: the library's first screen then shows some of each.
  const rank = (a: DemoActivity) => (a.folder.startsWith("games/") ? 0 : 1);
  return out.sort((a, b) => rank(a) - rank(b) || a.info.name.localeCompare(b.info.name));
}

export const CATALOG: DemoActivity[] = build();

export const byId = (id: unknown) => CATALOG.find((a) => a.info.id === id);
