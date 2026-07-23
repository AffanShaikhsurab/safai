import { type Component, For, type JSX } from "solid-js";
import { appStore, type View } from "../state/store";
import { formatBytes } from "../lib/format";
import logoUrl from "../assets/logo.png";

/** Small inline icons (stroke = currentColor). */
const icon = (children: JSX.Element): Component<{ class?: string }> => (p) =>
  (
    <svg
      class={p.class}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );

const DashboardIcon = icon(
  <>
    <rect x="3" y="3" width="7" height="9" rx="1.5" />
    <rect x="14" y="3" width="7" height="5" rx="1.5" />
    <rect x="14" y="12" width="7" height="9" rx="1.5" />
    <rect x="3" y="16" width="7" height="5" rx="1.5" />
  </>,
);
const CleanIcon = icon(
  <path d="M12 3l1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8z" />,
);
const SettingsIcon = icon(
  <>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3" />
  </>,
);

interface NavItem {
  id: View;
  label: string;
  Icon: Component<{ class?: string }>;
}

const NAV: NavItem[] = [
  { id: "dashboard", label: "Dashboard", Icon: DashboardIcon },
  { id: "clean", label: "Clean", Icon: CleanIcon },
  { id: "settings", label: "Settings", Icon: SettingsIcon },
];

const Sidebar: Component = () => {
  const go = (id: View) => {
    // Navigation preserves the Clean flow's state (scanning stays live,
    // results/done are kept). Starting a fresh run is an explicit action:
    // "Scan again" on the Done screen or "Scan now" on the Dashboard.
    appStore.setView(id);
  };

  return (
    <aside class="rail">
      {/* Brand → dashboard */}
      <button
        type="button"
        class="brand"
        onClick={() => go("dashboard")}
        title="Safai — go to dashboard"
      >
        <img class="mark-img" src={logoUrl} alt="" width={36} height={36} />
        <span>
          <span class="name">SAFAI</span>
          <span class="sub">Disk cleanup</span>
        </span>
      </button>

      {/* Nav */}
      <nav class="nav">
        <For each={NAV}>
          {(item) => (
            <button
              type="button"
              class="nav-item"
              data-active={appStore.state.view === item.id ? "true" : "false"}
              aria-current={appStore.state.view === item.id ? "page" : undefined}
              onClick={() => go(item.id)}
            >
              <item.Icon />
              {item.label}
            </button>
          )}
        </For>
      </nav>

      <div class="spacer" />

      {/* Footer: understated lifetime reclaimed + version */}
      <div class="foot">
        <div class="l">Lifetime reclaimed</div>
        <div class="v">
          {formatBytes(appStore.state.stats.lifetimeReclaimedBytes)}
        </div>
        <div class="ver">Safai v0.1.0</div>
      </div>
    </aside>
  );
};

export default Sidebar;
