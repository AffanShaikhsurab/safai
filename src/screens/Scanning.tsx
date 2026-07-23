import {
  type Component,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { appStore } from "../state/store";
import { cancelScan } from "../lib/tauri";
import { splitBytes } from "../lib/format";
import Dial from "../components/Dial";

/**
 * Clean · scanning — the dial becomes a live progress ring driven by
 * rulesChecked/rulesTotal, with the running reclaimable total in the center.
 *
 * Two ways out:
 *  • Stop   — flip the backend cancel flag but STAY in the scanning phase, so
 *             the in-flight `scan()` promise resolves with the partial report
 *             and the flow advances to Review with whatever was found so far.
 *  • Cancel — flip the flag AND return to Welcome, discarding the results.
 * (Both use the same backend cancel command; only the phase handling differs.)
 */
const Scanning: Component = () => {
  const progress = () => appStore.state.progress;
  const [stopping, setStopping] = createSignal(false);

  const pct = createMemo(() => {
    const p = progress();
    if (p.rulesTotal <= 0) return 0;
    return Math.min(100, Math.round((p.rulesChecked / p.rulesTotal) * 100));
  });

  const found = createMemo(() => splitBytes(progress().foundBytes));

  // Tick a "now" signal; elapsed is derived from the store's scanStartedAt so
  // the timer survives navigation (switching to Dashboard and back keeps
  // counting from the real start instead of resetting to 0).
  const [now, setNow] = createSignal(Date.now());
  onMount(() => {
    const id = window.setInterval(() => setNow(Date.now()), 500);
    onCleanup(() => window.clearInterval(id));
  });
  const elapsed = createMemo(() => {
    const start = appStore.state.scanStartedAt;
    if (!start) return 0;
    return Math.max(0, Math.floor((now() - start) / 1000));
  });
  const clock = createMemo(() => {
    const s = elapsed();
    const m = Math.floor(s / 60);
    const r = s % 60;
    return `${m}:${r.toString().padStart(2, "0")}`;
  });

  const currentName = () => {
    if (stopping()) return "Finishing up…";
    const p = progress().currentPath;
    if (!p) return "Starting…";
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts.length ? parts[parts.length - 1] : p;
  };

  // Stop & keep: flip the flag; the awaiting scan() resolves → Review.
  const stopAndKeep = async () => {
    if (stopping()) return;
    setStopping(true);
    await cancelScan();
  };

  // Cancel & discard: flip the flag and jump back to Welcome.
  const cancelDiscard = async () => {
    await cancelScan();
    appStore.setPhase("welcome");
  };

  // NOTE: we deliberately do NOT cancel the scan on unmount. The scan is owned
  // by the store (see `runScan`), so navigating to Dashboard/Settings and back
  // must not kill an in-flight scan. Cancellation is explicit only, via the
  // Stop/Cancel buttons above.

  return (
    <div class="stage animate-rise">
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Status</div>
            <div class="s">{stopping() ? "Stopping…" : "Working…"}</div>
          </div>
          <div class="val">{clock()}</div>
        </div>
        <div class="card topcard">
          <div>
            <div class="k">Found</div>
            <div class="s">items so far</div>
          </div>
          <div class="val">{progress().itemCount}</div>
        </div>
      </div>

      <div class="dialrow">
        {/* Cancel (discard) */}
        <div class="round-wrap">
          <button
            type="button"
            class="round"
            onClick={cancelDiscard}
            disabled={stopping()}
            title="Cancel and discard this scan"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M6 6l12 12M18 6l-12 12" />
            </svg>
          </button>
          <span class="cap">Cancel</span>
        </div>

        <Dial
          pct={pct()}
          big={found().value}
          unit={found().unit}
          cap="Reclaimable"
          sub={currentName()}
        />

        {/* Stop (keep results) — primary */}
        <div class="round-wrap">
          <button
            type="button"
            class="round mint"
            onClick={stopAndKeep}
            disabled={stopping()}
            title="Stop here and review what's found so far"
          >
            <svg viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true">
              <rect x="6" y="6" width="12" height="12" rx="3" />
            </svg>
          </button>
          <span class="cap">{stopping() ? "Stopping…" : "Stop"}</span>
        </div>
      </div>

      <div class="card infocard">
        <div class="l">Scanning</div>
        <div class="r" title={progress().currentPath}>
          {pct()}% · {progress().rulesChecked}/{progress().rulesTotal} checks ·{" "}
          {progress().currentPath || "starting…"}
        </div>
      </div>
    </div>
  );
};

export default Scanning;
