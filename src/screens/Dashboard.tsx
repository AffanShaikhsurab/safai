import { type Component, For, Show, onMount } from "solid-js";
import { appStore } from "../state/store";
import { loadStats } from "../lib/stats";
import { defaultRoots, detectTools, driveInfo } from "../lib/tauri";
import { formatBytes, relativeTime } from "../lib/format";
import { categoryMeta } from "../lib/categories";

// Deep sky blue ramp for breakdown bars (Nebula).
const BAR_COLORS = ["#2f5fad", "#3d74c4", "#5b8fd4", "#7eacdf", "#a8c8ec"];

/**
 * Dashboard — the overview. Reads straight off `appStore.state` (stats, drive,
 * report, tools). Keeps a calm first-run empty state; "Scan now" jumps into the
 * Clean flow at the welcome/setup phase.
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
        const probe = roots[0] ?? "C:/";
        appStore.setDriveBefore(await driveInfo(probe));
      }
    } catch {
      // Non-fatal: the gauge simply won't render.
    }
    try {
      if (appStore.state.tools.length === 0) {
        appStore.setTools(await detectTools());
      }
    } catch {
      // Non-fatal.
    }
  });

  const stats = () => appStore.state.stats;
  const drive = () => appStore.state.driveBefore;
  const report = () => appStore.state.report;
  const detectedTools = () => appStore.state.tools.filter((t) => t.detected);

  const usedPct = () => {
    const d = drive();
    if (!d || d.totalBytes <= 0) return 0;
    return Math.min(
      100,
      Math.round(((d.totalBytes - d.freeBytes) / d.totalBytes) * 100),
    );
  };

  const groupShare = (bytes: number) => {
    const total = report()?.totalReclaimableBytes ?? 0;
    if (total <= 0) return 0;
    return Math.max(2, Math.min(100, Math.round((bytes / total) * 100)));
  };

  const startScan = () => {
    appStore.setView("clean");
    // Don't disrupt an in-flight scan/clean — just switch to it. Otherwise
    // land on the setup (welcome) screen to configure a fresh run.
    const p = appStore.state.phase;
    if (p !== "scanning" && p !== "cleaning") {
      appStore.setPhase("welcome");
    }
  };

  return (
    <div class="dash animate-rise">
      {/* Header + primary CTA */}
      <div class="dash-head">
        <div>
          <div class="t">Overview</div>
          <div class="s">Your space at a glance.</div>
        </div>
        <button type="button" class="btn btn-primary" onClick={startScan}>
          Scan now
        </button>
      </div>

      {/* KPI row */}
      <div class="kpis">
        <div class="card kpi">
          <div class="k">Lifetime reclaimed</div>
          <div class="v mint">
            {formatBytes(stats().lifetimeReclaimedBytes)}
          </div>
          <div class="note">
            Across {stats().cleanupCount} cleanup
            {stats().cleanupCount === 1 ? "" : "s"}
          </div>
        </div>

        <div class="card kpi">
          <div class="k">Cleanups</div>
          <div class="v">{stats().cleanupCount}</div>
          <Show
            when={stats().lastCleanupAt}
            fallback={<div class="note">None yet</div>}
            keyed
          >
            {(at) => <div class="note">Last {relativeTime(at)}</div>}
          </Show>
        </div>

        <div class="card kpi">
          <div class="k">Last scan</div>
          <div class="v">{formatBytes(stats().lastScanReclaimable ?? 0)}</div>
          <Show
            when={stats().lastScanAt}
            fallback={<div class="note">No scan recorded yet</div>}
            keyed
          >
            {(at) => (
              <div class="note">
                {stats().lastScanItems ?? 0} items · {relativeTime(at)}
              </div>
            )}
          </Show>
        </div>
      </div>

      {/* Two-column: drive usage + last-scan breakdown */}
      <div class="dash-2col">
        <div class="card usage panel-pad">
          <div class="ch">Drive usage</div>
          <Show
            when={drive()}
            keyed
            fallback={
              <p class="note" style={{ "margin-top": "16px" }}>
                Drive information unavailable.
              </p>
            }
          >
            {(d) => (
              <>
                <div class="mount">
                  <span class="m">{d.mount}</span>
                  <span class="f">
                    {formatBytes(d.freeBytes)} free of{" "}
                    {formatBytes(d.totalBytes)}
                  </span>
                </div>
                <div class="bar-track" style={{ "margin-top": "12px" }}>
                  <i class="bar-fill" style={{ width: `${usedPct()}%` }} />
                </div>
                <div class="pct">{usedPct()}% in use</div>
              </>
            )}
          </Show>
        </div>

        <div class="card panel-pad">
          <div class="ch">Last scan breakdown</div>
          <Show
            when={report()}
            keyed
            fallback={
              <p class="note" style={{ "margin-top": "16px" }}>
                No scan yet — run one to see where your space is going.
              </p>
            }
          >
            {(r) => (
              <div class="brk">
                <For each={r.groups}>
                  {(group, i) => {
                    const meta = categoryMeta(group.category);
                    return (
                      <div class="brk-row">
                        <div class="top">
                          <span class="nm">
                            <meta.Icon />
                            {meta.label}
                          </span>
                          <span class="vv">{formatBytes(group.totalBytes)}</span>
                        </div>
                        <div class="b">
                          <i
                            style={{
                              width: `${groupShare(group.totalBytes)}%`,
                              background:
                                BAR_COLORS[i() % BAR_COLORS.length],
                            }}
                          />
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            )}
          </Show>
        </div>
      </div>

      {/* Detected tools */}
      <Show when={detectedTools().length > 0}>
        <div class="card panel-pad">
          <div class="ch">Detected tools</div>
          <div class="chips">
            <For each={detectedTools()}>
              {(tool) => (
                <span class="chip">
                  <span class="dot" aria-hidden="true" />
                  {tool.label}
                </span>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Dashboard;
