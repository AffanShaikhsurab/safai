import { Switch, Match, Show, onMount, type Component, type JSX } from "solid-js";
import { appStore } from "./state/store";
import {
  loadDevInactiveDays,
  loadScanPrefs,
  loadSkyPrefs,
  loadTheme,
} from "./lib/prefs";
import { ensureNotifyPermission } from "./lib/notify";
import { layoutFamily } from "./lib/layout";
import PixelSky from "./components/PixelSky";
import SkyShell from "./shell/SkyShell";
import DenseShell from "./shell/DenseShell";
import Dashboard from "./screens/Dashboard";
import Automation from "./screens/Automation";
import Settings from "./screens/Settings";
import Welcome from "./screens/Welcome";
import Scanning from "./screens/Scanning";
import Results from "./screens/Results";
import Cleaning from "./screens/Cleaning";
import Done from "./screens/Done";

/**
 * The screen for the current view/phase. Shared by both shells, so the routing
 * table exists once — only the chrome around it differs per layout family.
 */
const Screen: Component = () => (
  <Switch fallback={<Dashboard />}>
    <Match when={appStore.state.view === "dashboard"}>
      <Dashboard />
    </Match>
    <Match when={appStore.state.view === "automation"}>
      <Automation />
    </Match>
    <Match when={appStore.state.view === "settings"}>
      <Settings />
    </Match>
    <Match when={appStore.state.view === "clean"}>
      <Switch fallback={<Welcome />}>
        <Match when={appStore.state.phase === "welcome"}>
          <Welcome />
        </Match>
        <Match when={appStore.state.phase === "scanning"}>
          <Scanning />
        </Match>
        <Match when={appStore.state.phase === "results"}>
          <Results />
        </Match>
        <Match when={appStore.state.phase === "cleaning"}>
          <Cleaning />
        </Match>
        <Match when={appStore.state.phase === "done"}>
          <Done />
        </Match>
      </Switch>
    </Match>
  </Switch>
);

const App: Component = () => {
  // Restore saved preferences on launch.
  onMount(async () => {
    // Sky settings before the theme: PixelSky reacts to both, and setting the
    // theme last means the canvas builds once with the right palette, not twice.
    appStore.setSkyPrefs(await loadSkyPrefs());
    appStore.setTheme(await loadTheme());
    appStore.setScanPrefs(await loadScanPrefs());
    appStore.setDevInactiveDays(await loadDevInactiveDays(), false);
    // Automation: subscribe before hydrating the rest, so a run starting
    // during startup can't slip through the gap between the two.
    void appStore.initAutomationListeners();
    // Enumerate drives before the first screen paints so Welcome and the
    // shell header have the real volume list. On a first run with more than
    // one drive, default to Quick: a Deep data-drive scan is the only new
    // multi-minute cost, and it is one click away.
    const firstMultiDriveRun = await appStore.hydrateDrives();
    if (firstMultiDriveRun) appStore.setDeepScan(false);
    // Restore the last scan report so the Overview breakdown persists across
    // restarts (KPI stats are hydrated in Dashboard.onMount).
    void appStore.hydrateLastReport();
    // Ask for notification permission up front so completion pings work later.
    void ensureNotifyPermission();
    void appStore.hydrateAutomation();
  });

  const screen = (): JSX.Element => <Screen />;

  return (
    <>
      <PixelSky />
      <div class="sky-content">
        {/* One dispatch point for the whole app's structure. See lib/layout.ts
            for why this keys on family rather than on theme. */}
        <Show
          when={layoutFamily(appStore.state.theme) === "dense"}
          fallback={<SkyShell>{screen()}</SkyShell>}
        >
          <DenseShell>{screen()}</DenseShell>
        </Show>
      </div>
    </>
  );
};

export default App;
