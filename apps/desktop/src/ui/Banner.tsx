import type { ReactNode } from "react";
import { Icon } from "./Icon";

type Tone = "danger" | "attention" | "success" | "info";

const ICON = { danger: "alert", attention: "alert", success: "check", info: "info" } as const;

/**
 * Problems are alerts (announced at once); confirmations and notes are polite status. Pass
 * `live={false}` when the banner sits inside a live region that is always present.
 */
export function Banner({
  tone,
  children,
  live = true,
}: {
  tone: Tone;
  children: ReactNode;
  live?: boolean;
}) {
  return (
    <div
      className={`banner banner-${tone}`}
      role={live ? (tone === "danger" ? "alert" : "status") : undefined}
    >
      <Icon name={ICON[tone]} />
      <div>{children}</div>
    </div>
  );
}
