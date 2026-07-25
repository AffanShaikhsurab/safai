import { type Component, type JSX } from "solid-js";
import type { Phase, View } from "../state/store";

/**
 * Navigation model shared by both shells.
 *
 * The two shells render this very differently — icon-only buttons vs labelled
 * rows with counts — but the *set* of destinations and their order must not
 * diverge, so it lives here once.
 */

const icon = (children: JSX.Element): Component<{ class?: string }> => (p) =>
  (
    <svg
      class={p.class}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );

export const DashboardIcon = icon(
  <>
    <rect x="3" y="3" width="7" height="9" rx="1" />
    <rect x="14" y="3" width="7" height="5" rx="1" />
    <rect x="14" y="12" width="7" height="9" rx="1" />
    <rect x="3" y="16" width="7" height="5" rx="1" />
  </>,
);
export const CleanIcon = icon(
  <path d="M12 3l1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8z" />,
);
export const AutomationIcon = icon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v4l3 2" />
  </>,
);
export const SettingsIcon = icon(
  <>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3" />
  </>,
);

export interface NavItem {
  id: View;
  label: string;
  Icon: Component<{ class?: string }>;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "dashboard", label: "Overview", Icon: DashboardIcon },
  { id: "clean", label: "Clean", Icon: CleanIcon },
  { id: "automation", label: "Automation", Icon: AutomationIcon },
  { id: "settings", label: "Settings", Icon: SettingsIcon },
];

const PHASE_LABEL: Record<Phase, string> = {
  welcome: "Setup",
  scanning: "Scanning",
  results: "Review",
  cleaning: "Cleaning",
  done: "Done",
};

/**
 * Header title. Inside the Clean flow the phase is more useful than the section
 * name, since the section never changes while the phase does.
 */
export function SCREEN_TITLE(view: View, phase: Phase): string {
  if (view === "clean") return `Clean · ${PHASE_LABEL[phase]}`;
  const hit = NAV_ITEMS.find((n) => n.id === view);
  return hit?.label ?? "Safai";
}
