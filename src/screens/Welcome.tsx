import { type Component, Show, createMemo, createSignal, onMount } from "solid-js";
import { appStore } from "../state/store";
import {
  defaultRoots,
  detectTools,
  driveInfo,
  pickFolder,
} from "../lib/tauri";
import { formatBytes, splitBytes } from "../lib/format";
import Dial from "../components/Dial";

/**
 * Clean · setup — the dial control panel (ported from docs/mockup.html):
 * two top cards (free space + deep-scan toggle), the big drive-usage dial with
 * a round Scan button beside it, and two segmented cards (scan mode /
 * destination). "Add folder" is kept as a small secondary affordance so the
 * clean look is preserved without losing the ability to include custom roots.
 */
const Welcome: Component = () => {
  const [loadError, setLoadError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const [roots, tools] = await Promise.all([defaultRoots(), detectTools()]);
      if (appStore.state.roots.length === 0) appStore.setRoots(roots);
      appStore.setTools(tools);

      const probe = roots[0] ?? "C:/";
      try {
        appStore.setDriveBefore(await driveInfo(probe));
      } catch {
        // Non-fatal: dial falls back to placeholder text.
      }
    } catch (e) {
      setLoadError(String(e));
    }
  });

  const drive = () => appStore.state.driveBefore;
  const usedPct = createMemo(() => {
    const d = drive();
    if (!d || d.totalBytes <= 0) return 0;
    return Math.min(
      100,
      Math.round(((d.totalBytes - d.freeBytes) / d.totalBytes) * 100),
    );
  });
  const free = createMemo(() => splitBytes(drive()?.freeBytes ?? 0));

  const addFolder = async () => {
    const folder = await pickFolder();
    if (folder) appStore.addRoot(folder);
  };

  const startScan = () => {
    const roots = [...appStore.state.roots];
    if (roots.length === 0) return;
    // Store-owned orchestration: the scan survives navigation between views.
    void appStore.runScan(roots);
  };

  const canScan = () => appStore.state.roots.length > 0;

  return (
    <div class="stage animate-rise">
      {/* Top cards: free space + deep-scan toggle */}
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Free space</div>
            <div class="s">{drive()?.mount ?? "Drive C:"}</div>
          </div>
          <div class="val">{formatBytes(drive()?.freeBytes ?? 0)}</div>
        </div>
        <div class="card topcard">
          <div>
            <div class="k">Deep scan</div>
            <div class="s">Check large folders</div>
          </div>
          <span
            class="switch"
            data-on={appStore.state.deepScan ? "true" : "false"}
            role="switch"
            aria-checked={appStore.state.deepScan}
            aria-label="Deep scan"
          >
            <input
              type="checkbox"
              class="sr-check"
              checked={appStore.state.deepScan}
              onChange={(e) => appStore.setDeepScan(e.currentTarget.checked)}
            />
          </span>
        </div>
      </div>

      {/* Dial + Scan button */}
      <div class="dialrow">
        <div class="dial-spacer" aria-hidden="true" />

        <Dial
          pct={usedPct()}
          big={free().value}
          unit={free().unit}
          cap="Free"
          sub={
            drive()
              ? `of ${formatBytes(drive()!.totalBytes)} · ${usedPct()}% used`
              : "Detecting drive…"
          }
        />

        <div class="round-wrap">
          <button
            type="button"
            class="round mint"
            disabled={!canScan()}
            onClick={startScan}
            title="Scan for reclaimable space"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M8 5v14l11-7z" />
            </svg>
          </button>
          <span class="cap">Scan</span>
        </div>
      </div>

      {/* Bottom cards: scan mode + destination */}
      <div class="bottomrow">
        <div class="card segcard">
          <div>
            <div class="k">Scan mode</div>
            <div class="cur">{appStore.state.deepScan ? "Deep" : "Quick"}</div>
          </div>
          <div class="segmented" role="group" aria-label="Scan mode">
            <button
              type="button"
              data-on={!appStore.state.deepScan ? "true" : "false"}
              onClick={() => appStore.setDeepScan(false)}
            >
              Quick
            </button>
            <button
              type="button"
              data-on={appStore.state.deepScan ? "true" : "false"}
              onClick={() => appStore.setDeepScan(true)}
            >
              Deep
            </button>
          </div>
        </div>

        <div class="card segcard">
          <div>
            <div class="k">After cleaning</div>
            <div class="cur">
              {appStore.state.toRecycleBin ? "Recycle Bin" : "Permanent"}
            </div>
          </div>
          <div class="segmented" role="group" aria-label="After cleaning">
            <button
              type="button"
              data-on={appStore.state.toRecycleBin ? "true" : "false"}
              onClick={() => appStore.setDestination(true)}
            >
              Recycle
            </button>
            <button
              type="button"
              data-on={!appStore.state.toRecycleBin ? "true" : "false"}
              onClick={() => appStore.setDestination(false)}
            >
              Permanent
            </button>
          </div>
        </div>
      </div>

      {/* Secondary affordance: scan scope + add folder */}
      <div class="scope-row">
        <span>
          Scanning {appStore.state.roots.length} location
          {appStore.state.roots.length === 1 ? "" : "s"}
        </span>
        <span aria-hidden="true">·</span>
        <button
          type="button"
          class="link"
          onClick={addFolder}
          title="Add one of your own folders to the scan"
        >
          + Add folder
        </button>
      </div>

      <Show when={loadError()} keyed>
        {(err) => <p class="err-line">{err}</p>}
      </Show>
    </div>
  );
};

export default Welcome;
