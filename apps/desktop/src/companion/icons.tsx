// Small line icons for the Companion toolbar and rail. Decorative: the button carries the name.

import type { ReactNode } from "react";

function Svg({ children }: { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="18"
      height="18"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export const GearIcon = () => (
  <Svg>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7" />
  </Svg>
);

/** An arrow pointing down at a baseline: follow the newest text. */
export const FollowIcon = () => (
  <Svg>
    <path d="M12 4v11M7 11l5 5 5-5M5 20h14" />
  </Svg>
);

export const PlusIcon = () => (
  <Svg>
    <path d="M12 5v14M5 12h14" />
  </Svg>
);

/** A panel with its left rail; the chevron points the way the rail will go. */
export const PanelIcon = ({ collapse }: { collapse: boolean }) => (
  <Svg>
    <rect x="3.5" y="4.5" width="17" height="15" rx="3" />
    <path d="M9.5 4.5v15" />
    <path d={collapse ? "m16 10-2 2 2 2" : "m14 10 2 2-2 2"} />
  </Svg>
);
