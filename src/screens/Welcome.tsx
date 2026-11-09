import { type Component, For, Show, createMemo, createSignal, onMount } from "solid-js";
import { appStore } from "../state/store";
import { detectTools, pickFolders } from "../lib/tauri";
import { formatBytes, splitBytes } from "../lib/format";
import { aggregateDrives, perDriveTotals } from "../lib/drives";
import type { DriveEntry } from "../lib/types";
import Dial from "../components/Dial";

/**
 * Clean · setup — drive scope, scan depth and destination.
 *
 * Multi-drive is first-class here: the list is the scope. The dial is an
 * aggregate of the selected volumes (never one dial per drive), and the only
 * primary action is still one Scan button.
 */
const Welcome: Component = () => {
  const [loadError, setLoadError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const tools = await detectTools();
      appStore.setTools(tools);
      // Welcome can be the first screen a user lands on after navigation, so
      // make sure the scope is hydrated even if App's bootstrap is still busy.
      if (appStore.state.drives.length === 0) await appStore.hydrateDrives();
    } catch (e) {
      setLoadError(String(e));
    }
  });

  const drives = () => appStore.state.drives;
  const selectedDrives = createMemo(() =>
    drives().filter((d) => appStore.state.selectedDrives.includes(d.mount)),
  );
  const aggregate = createMemo(() => aggregateDrives(selectedDrives()));
  const free = createMemo(() => splitBytes(aggregate().freeBytes));

  const lastFound = createMemo(() => {
    const report = appStore.state.report;
    if (!report) return new Map<string, number>();
    const totals = perDriveTotals(report.groups.flatMap((g) => g.items));
    return new Map(totals.flatMap((t) => (t.mount ? [[t.mount, t.bytes]] : [])));
  });

  const selectedCount = () => appStore.state.selectedDrives.length;
  const isSelected = (drive: DriveEntry) =>
    appStore.state.selectedDrives.includes(drive.mount);

  const lowSpace = (drive: DriveEntry) =>
    drive.readable && (drive.usedPercent ?? 0) >= 85;

  const lastLabel = (drive: DriveEntry): string => {
    if (!drive.readable) return "unavailable";
    if (!appStore.state.report) return "not scanned yet";
    const bytes = lastFound().get(drive.mount) ?? 0;
    return bytes > 0 ? `${formatBytes(bytes)} last scan` : "clean last scan";
  };

  const addFolders = async () => {
    const folders = await pickFolders();
    for (const folder of folders) appStore.addFolder(folder);
  };

  const startScan = () => {
    if (selectedCount() === 0 && appStore.state.folders.length === 0) return;
    void appStore.runScan();
  };

  const canScan = () =>
    selectedCount() > 0 || appStore.state.folders.length > 0;

  return (
    <div class="stage animate-rise">
      {/* Top cards: aggregate free space + selected drive count */}
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Free space</div>
            <div class="s">
              {aggregate().count === 1
                ? selectedDrives()[0]?.mount ?? "Drive"
                : `across ${aggregate().count} drives`}
            </div>
          </div>
          <div class="val">{formatBytes(aggregate().freeBytes)}</div>
        </div>
        <div class="card topcard">
          <div>
            <div class="k">Drives</div>
            <div class="s">
              {selectedCount()} of {drives().length} selected
            </div>
          </div>
          <div class="val">
            {aggregate().usedPercent}%
            <span class="unit"> used</span>
          </div>
        </div>
      </div>

      {/* Drive scope */}
      <div class="card vol-panel">
        <div class="vol-head">
          <span class="k">Drives</span>
          <span class="s">
            {selectedCount()} of {drives().length} selected
          </span>
          <div class="grow" />
          <button
            type="button"
            class="link"
            onClick={() => appStore.setAllDrives(true)}
          >
            All
          </button>
          <button
            type="button"
            class="link"
            onClick={() => appStore.setAllDrives(false)}
          >
            None
          </button>
        </div>

        <Show
          when={drives().length > 0}
          fallback={
            <p class="sky-note">
              We couldn't read any local drives. Windows may be blocking volume
              access — add a folder instead.
            </p>
          }
        >
          <div class="vol-list">
            <For each={drives()}>
              {(drive) => {
                const on = isSelected(drive);
                return (
                  <label
                    class="vol-row"
                    data-on={on ? "true" : "false"}
                    data-disabled={!drive.readable ? "true" : "false"}
                  >
                    <input
                      type="checkbox"
                      class="sr-check"
                      checked={on}
                      disabled={!drive.readable}
                      aria-label={`Scan ${drive.mount} ${drive.label}`}
                      onChange={(e) =>
                        appStore.setDriveSelected(
                          drive.mount,
                          e.currentTarget.checked,
                        )
                      }
                    />
                    <span
                      class="vol-check"
                      data-on={on ? "true" : "false"}
                      aria-hidden="true"
                    />
                    <span class="vol-name">
                      <span class="nm">
                        {drive.mount}
                        <Show when={drive.label}>
                          <span class="vol-label"> · {drive.label}</span>
                        </Show>
                      </span>
                      <span class="ds">
                        {drive.readable
                          ? `${formatBytes(drive.freeBytes)} free of ${formatBytes(
                              drive.totalBytes,
                            )}`
                          : "Unavailable right now"}
                      </span>
                    </span>
                    <Show when={drive.system}>
                      <span class="vol-tag">System</span>
                    </Show>
                    <Show when={lowSpace(drive)}>
                      <span class="vol-tag hot">Low space</span>
                    </Show>
                    <span class="vol-meter" aria-hidden="true">
                      <i
                        data-hot={lowSpace(drive) ? "true" : "false"}
                        style={{ width: `${drive.usedPercent ?? 0}%` }}
                      />
                    </span>
                    <span class="vol-pct">
                      {drive.usedPercent === null
                        ? "—"
                        : `${Math.round(drive.usedPercent)}%`}
                    </span>
                    <span class="vol-last">{lastLabel(drive)}</span>
                  </label>
                );
              }}
            </For>
          </div>
        </Show>

        <Show when={appStore.state.folders.length > 0}>
          <div class="vol-folders">
            <For each={appStore.state.folders}>
              {(folder) => (
                <div class="vol-folder">
                  <span class="vol-folder-name" title={folder}>
                    {folder}
                  </span>
                  <button
                    type="button"
                    class="link"
                    title={`Stop scanning ${folder}`}
                    onClick={() => appStore.removeFolder(folder)}
                  >
                    Remove
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>

        <Show when={selectedCount() < drives().length}>
          <p class="sky-note">
            Known caches are only looked for on the drives you pick.
          </p>
        </Show>
      </div>

      {/* Dial + Scan button */}
      <div class="dialrow">
        <div class="dial-spacer" aria-hidden="true" />

        <Dial
          pct={aggregate().usedPercent}
          big={free().value}
          unit={free().unit}
          cap="Free"
          sub={
            aggregate().count === 1
              ? `${selectedDrives()[0]?.mount ?? "Drive"} · ${
                  aggregate().usedPercent
                }% used`
              : `across ${aggregate().count} drives · ${aggregate().usedPercent}% used`
          }
        />

        <div class="round-wrap">
          <button
            type="button"
            class="round mint"
            disabled={!canScan()}
            onClick={startScan}
            title={`Scan ${selectedCount()} drive${
              selectedCount() === 1 ? "" : "s"
            }`}
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
            <div class="k">Scan depth</div>
            <div class="cur">{appStore.state.deepScan ? "Deep" : "Quick"}</div>
          </div>
          <div class="segmented" role="group" aria-label="Scan depth">
            <button
              type="button"
              data-on={!appStore.state.deepScan ? "true" : "false"}
              onClick={() => appStore.setDeepScan(false)}
              title="Known caches and project folders"
            >
              Quick
            </button>
            <button
              type="button"
              data-on={appStore.state.deepScan ? "true" : "false"}
              onClick={() => appStore.setDeepScan(true)}
              title="Also sizes large folders on the selected drives"
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

      <Show when={appStore.state.deepScan && selectedCount() > 1}>
        <p class="sky-note">
          Deep scan also sizes large folders on each selected drive. It can take
          several minutes on a data drive; Quick finds known caches and projects
          faster.
        </p>
      </Show>

      {/* Secondary affordance: scan scope + add folder */}
      <div class="scope-row">
        <span>
          Scanning {selectedCount()} drive{selectedCount() === 1 ? "" : "s"}
          <Show when={appStore.state.folders.length > 0}>
            {" "}
            · {appStore.state.folders.length} folder
            {appStore.state.folders.length === 1 ? "" : "s"}
          </Show>
        </span>
        <span aria-hidden="true">·</span>
        <button
          type="button"
          class="link"
          onClick={addFolders}
          title="Add one or more of your own folders to the scan"
        >
          + Add folders
        </button>
        <Show when={!canScan()}>
          <span class="err-line">Pick at least one drive to scan.</span>
        </Show>
      </div>

      <Show when={loadError()} keyed>
        {(err) => <p class="err-line">{err}</p>}
      </Show>
    </div>
  );
};

export default Welcome;
