import type { ReactNode } from "react";
import { Icon } from "./Icon";

type Tone = "danger" | "attention" | "success" | "info";

const ICON = { danger: "alert", attention: "alert", success: "check", info: "info" } as const;

/** Problems are alerts (announced at once); confirmations and notes are polite status. */
export function Banner({ tone, children }: { tone: Tone; children: ReactNode }) {
  return (
    <div className={`banner banner-${tone}`} role={tone === "danger" ? "alert" : "status"}>
      <Icon name={ICON[tone]} />
      <div>{children}</div>
    </div>
  );
}
