// The Lullshift mascot (docs/brand): a folded-paper card that rests while agents work and
// wakes when one needs you. Bundled with the app; never fetched from the network.
import asleep from "../../../../docs/brand/mascot-asleep.svg";
import awake from "../../../../docs/brand/mascot-awake.svg";
import happy from "../../../../docs/brand/mascot-happy.svg";
import watching from "../../../../docs/brand/mascot-watching.svg";

const FACES = { asleep, awake, happy, watching } as const;

export type Face = keyof typeof FACES;

/** Decorative unless `label` is given; the meaning is always also said in text nearby. */
export function Mascot({ face, size, label }: { face: Face; size: number; label?: string }) {
  return (
    <img
      className={`mascot mascot-${face}`}
      src={FACES[face]}
      width={size}
      height={size}
      alt={label ?? ""}
      draggable={false}
    />
  );
}
