import { type Component, Show } from "solid-js";
import { appStore } from "../state/store";
import { formatBytes } from "../lib/format";

const SECTION: Record<string, string> = {
  dashboard: "Dashboard",
  clean: "Clean",
  settings: "Settings",
};

const PHASE_LABEL: Record<string, string> = {
  welcome: "Setup",
  scanning: "Scanning",
  results: "Review",
  cleaning: "Cleaning",
  done: "Done",
};

/**
 * Slim content header. Left: a breadcrumb of the current section (and, during
 * the clean flow, the live phase). Right: a compact free-space readout. The
 * theme picker deliberately lives only in Settings, not here.
 */
const TopBar: Component = () => {
  const section = () => SECTION[appStore.state.view] ?? "";
  const drive = () => appStore.state.driveBefore;

  return (
    <header class="topbar">
      <div class="crumb">
        Safai <span class="sep">/</span>
        <b>{section()}</b>
        <Show when={appStore.state.view === "clean"}>
          <span class="sep">/</span>
          <b>{PHASE_LABEL[appStore.state.phase]}</b>
        </Show>
      </div>

      <Show when={drive()} keyed>
        {(d) => (
          <span class="drive">
            <span class="dot" aria-hidden="true" />
            {formatBytes(d.freeBytes)} free on {d.mount}
          </span>
        )}
      </Show>
    </header>
  );
};

export default TopBar;
