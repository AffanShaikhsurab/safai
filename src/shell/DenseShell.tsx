import { type Component, For, type JSX, Show } from "solid-js";
import { appStore, type View } from "../state/store";
import { formatBytes, relativeTime } from "../lib/format";
import { usedPercent } from "../lib/sky";
import { NAV_ITEMS } from "./nav";

/**
 * Shell for the `dense` layout family (Pulsar).
 *
 * A different philosophy, not a reskin: a 208px labelled rail with live counts
 * and a pinned disk readout, and a 46px status bar carrying the primary actions.
 * Content runs the full remaining width — the per-item detail each row needs is
 * already in the table columns, so there is no side panel competing for it.
 */
const DenseShell: Component<{ children: JSX.Element; footer?: JSX.Element }> = (
  props,
) => {
  const drive = () => appStore.state.driveBefore;
  const report = () => appStore.state.report;
  const stats = () => appStore.state.stats;

  const go = (id: View) => appStore.setView(id);

  /** Live count badge per destination. Empty string renders no badge. */
  const badge = (id: View): string => {
    if (id === "clean") {
      const n = report()?.groups.reduce((a, g) => a + g.items.length, 0) ?? 0;
      return n > 0 ? String(n) : "";
    }
    if (id === "automation") {
      const a = appStore.state.automation;
      if (!a) return "";
      return a.running ? "run" : a.config.enabled ? "on" : "off";
    }
    return "";
  };

  return (
    <div class="dense-shell">
      <nav class="dense-side" aria-label="Sections">
        <div class="dense-brand">
          <span class="dot" aria-hidden="true" />
          SAFAI
          <span class="ver">0.1.2</span>
        </div>

        <div class="dense-grp">Workspace</div>
        <For each={NAV_ITEMS}>
          {(item) => (
            <button
              type="button"
              class="dense-nav"
              data-on={appStore.state.view === item.id ? "true" : "false"}
              aria-current={appStore.state.view === item.id ? "page" : undefined}
              onClick={() => go(item.id)}
            >
              {item.label}
              <Show when={badge(item.id)}>
                <span class="tag">{badge(item.id)}</span>
              </Show>
            </button>
          )}
        </For>

        <div class="dense-grp">Scope</div>
        <div class="dense-stat">
          <span>Roots</span>
          <span class="v">{appStore.state.roots.length || "—"}</span>
        </div>
        <div class="dense-stat">
          <span>Lifetime</span>
          <span class="v">{formatBytes(stats().lifetimeReclaimedBytes)}</span>
        </div>
        <div class="dense-stat">
          <span>Cleanups</span>
          <span class="v">{stats().cleanupCount}</span>
        </div>

        <div class="grow" />

        <Show when={drive()} keyed>
          {(d) => {
            const used = usedPercent(d.freeBytes, d.totalBytes);
            return (
              <div class="dense-disk">
                {d.mount} &nbsp;{used}% used
                <div class="bar" aria-hidden="true">
                  <i style={{ width: `${used}%` }} />
                </div>
                {formatBytes(d.freeBytes)} free / {formatBytes(d.totalBytes)}
              </div>
            );
          }}
        </Show>
      </nav>

      <div class="dense-main">
        <header class="dense-top">
          <span class="title">
            {NAV_ITEMS.find((n) => n.id === appStore.state.view)?.label ?? "Safai"}
          </span>
          <span class="vsep" aria-hidden="true" />
          {/* Only pills backed by real data. The mockup also showed "5.3M files
              indexed" and "12 threads", neither of which the backend exposes —
              inventing them would be a lie on the instrument panel. */}
          <Show when={report()} keyed>
            {(r) => (
              <span class="dense-pill">
                {r.groups.reduce((a, g) => a + g.items.length, 0)} items
              </span>
            )}
          </Show>
          <Show when={stats().lastScanAt} keyed>
            {(at) => <span class="dense-pill">scanned {relativeTime(at)}</span>}
          </Show>
          <Show when={appStore.state.report}>
            <span class="dense-pill">
              {formatBytes(appStore.state.report!.totalReclaimableBytes)} reclaimable
            </span>
          </Show>
          <div class="grow" />
          <button type="button" class="dense-btn" onClick={() => go("clean")}>
            Scan
          </button>
          <Show when={appStore.selectedCount() > 0}>
            <button
              type="button"
              class="dense-btn pri"
              onClick={() => {
                go("clean");
                appStore.setPhase("results");
              }}
            >
              Clean {formatBytes(appStore.reclaimableSelectedBytes())}
            </button>
          </Show>
        </header>

        <div class="dense-content scroll-region">{props.children}</div>

        <Show when={props.footer}>{props.footer}</Show>
      </div>
    </div>
  );
};

export default DenseShell;
