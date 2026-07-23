import { Switch, Match, onMount, type Component } from "solid-js";
import { appStore } from "./state/store";
import { loadScanPrefs, loadTheme } from "./lib/prefs";
import { ensureNotifyPermission } from "./lib/notify";
import SpaceBackground from "./components/SpaceBackground";
import Sidebar from "./components/Sidebar";
import TopBar from "./components/TopBar";
import Dashboard from "./screens/Dashboard";
import Settings from "./screens/Settings";
import Welcome from "./screens/Welcome";
import Scanning from "./screens/Scanning";
import Results from "./screens/Results";
import Cleaning from "./screens/Cleaning";
import Done from "./screens/Done";

const App: Component = () => {
  // Restore saved preferences on launch (theme applies the <html> class).
  onMount(async () => {
    appStore.setTheme(await loadTheme());
    appStore.setScanPrefs(await loadScanPrefs());
    // Restore the last scan report so the Dashboard breakdown persists across
    // restarts (the KPI stats are hydrated separately in Dashboard.onMount).
    void appStore.hydrateLastReport();
    // Ask for notification permission up front so completion pings work later.
    void ensureNotifyPermission();
  });

  return (
    <>
      <SpaceBackground />
      <div class="sky-content flex h-screen text-white">
        <Sidebar />
        <div class="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <main class="scroll-region flex min-w-0 flex-1 flex-col">
            <Switch fallback={<Dashboard />}>
              <Match when={appStore.state.view === "dashboard"}>
                <Dashboard />
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
          </main>
        </div>
      </div>
    </>
  );
};

export default App;
