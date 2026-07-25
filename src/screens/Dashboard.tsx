import { type Component, Show, onMount } from "solid-js";
import { appStore } from "../state/store";
import { loadStats } from "../lib/stats";
import { defaultRoots, detectTools, driveInfo } from "../lib/tauri";
import { layoutFamily } from "../lib/layout";
import OverviewSky from "./overview/Sky";
import OverviewDense from "./overview/Dense";

/**
 * Overview — dispatches to one of two layout families.
 *
 * Nebula/Void get the centred night-sky column; Pulsar gets the dense
 * instrument panel. They are different DOM, not CSS variants of each other: a
 * treemap plus a sortable table cannot be restyled into a column of hairline
 * rows. All the arithmetic is shared via `overview/model.ts`, so only markup is
 * duplicated.
 *
 * Data loading stays here, above the split, so neither variant can drift on
 * which side-effects it triggers.
 */
const Dashboard: Component = () => {
  onMount(async () => {
    try {
      appStore.setStats(await loadStats());
    } catch {
      // Keep whatever stats are already in the store.
    }
    try {
      if (!appStore.state.driveBefore) {
        const roots = await defaultRoots();
        appStore.setDriveBefore(await driveInfo(roots[0] ?? "C:/"));
      }
    } catch {
      // Non-fatal: the drive stat renders a dash.
    }
    try {
      if (appStore.state.tools.length === 0) {
        appStore.setTools(await detectTools());
      }
    } catch {
      // Non-fatal.
    }
  });

  return (
    <Show
      when={layoutFamily(appStore.state.theme) === "dense"}
      fallback={<OverviewSky />}
    >
      <OverviewDense />
    </Show>
  );
};

export default Dashboard;
