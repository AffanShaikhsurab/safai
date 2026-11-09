import { type Component, For, Show, createMemo } from "solid-js";
import { appStore } from "../state/store";
import { formatBytes, splitBytes } from "../lib/format";
import { openPath } from "../lib/tauri";
import Dial from "../components/Dial";

/** Last path component (file/folder name) for a compact display label. */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

/**
 * Clean · done — a full dial with the reclaimed headline, a before/after free
 * readout, and a round "Scan again" button that resets the flow.
 */
const Done: Component = () => {
  const progress = () => appStore.state.progress;
  const before = () => appStore.state.driveBefore;
  const after = () => appStore.state.driveAfter;

  const reclaimed = createMemo(() => splitBytes(progress().reclaimedBytes));

  const afterDrives = () =>
    appStore.state.drivesAfter.length > 0
      ? appStore.state.drivesAfter
      : appStore.state.drives;

  const freeNow = () => {
    const affected = new Set(
      Object.keys(appStore.state.reclaimedByDrive).filter((m) => m !== "other"),
    );
    const rows = afterDrives().filter((drive) => affected.has(drive.mount));
    if (rows.length > 0) {
      return rows.reduce((sum, drive) => sum + drive.freeBytes, 0);
    }
    return after()?.freeBytes ?? before()?.freeBytes ?? 0;
  };

  const mount = () => {
    const affected = Object.keys(appStore.state.reclaimedByDrive).filter(
      (m) => m !== "other",
    );
    const system = appStore.state.drives.find((d) => d.system)?.mount;
    if (system && affected.includes(system)) return system;
    return affected[0] ?? after()?.mount ?? before()?.mount ?? "Drive C:";
  };

  const driveRows = createMemo(() => {
    const reclaimed = appStore.state.reclaimedByDrive;
    return Object.entries(reclaimed)
      .filter(([mount]) => mount !== "other")
      .map(([mount, bytes]) => {
        const drive = afterDrives().find((d) => d.mount === mount);
        return {
          mount,
          reclaimed: bytes,
          free: drive?.freeBytes ?? null,
          before: drive ? drive.freeBytes + bytes : null,
        };
      })
      .sort((a, b) => b.reclaimed - a.reclaimed);
  });

  return (
    <div class="stage animate-rise">
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Free space</div>
            <div class="s">
              {Object.keys(appStore.state.reclaimedByDrive).filter(
                (m) => m !== "other",
              ).length > 1
                ? "across cleaned drives"
                : mount()}
            </div>
          </div>
          <div class="val mint">{formatBytes(freeNow())}</div>
        </div>
        <div class="card topcard">
          <div>
            <div class="k">Removed</div>
            <div class="s">
              {progress().deleted} item{progress().deleted === 1 ? "" : "s"}
            </div>
          </div>
          <div class="val">{progress().skipped} skipped</div>
        </div>
      </div>

      <div class="dialrow">
        <div class="dial-spacer" aria-hidden="true" />

        <Dial
          pct={100}
          big={reclaimed().value}
          unit={reclaimed().unit}
          cap="Reclaimed"
          sub={
            before()
              ? `on ${mount()} · was ${formatBytes(before()!.freeBytes)}`
              : `on ${mount()}`
          }
        />

        <div class="round-wrap">
          <button
            type="button"
            class="round mint"
            onClick={() => appStore.reset()}
            title="Scan again"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 12a9 9 0 1 1-2.6-6.4" />
              <path d="M21 3v5h-5" />
            </svg>
          </button>
          <span class="cap">Scan again</span>
        </div>
      </div>

      <div class="card infocard">
        <div class="l">Result</div>
        <div class="r">
          <Show
            when={before()}
            fallback={<>Reclaimed {formatBytes(progress().reclaimedBytes)}</>}
            keyed
          >
            {(b) => (
              <>
                Free now {formatBytes(freeNow())} (was{" "}
                {formatBytes(b.freeBytes)})
              </>
            )}
          </Show>
        </div>
      </div>

      <Show when={driveRows().length > 1}>
        <section class="card skip-card">
          <div class="skip-head">
            <span class="t">By drive</span>
            <span class="s">Reclaimed and free space on each drive.</span>
          </div>
          <div class="drive-breakdown">
            <For each={driveRows()}>
              {(row) => (
                <div class="row">
                  <span class="mnt">{row.mount}</span>
                  <span>{formatBytes(row.reclaimed)} reclaimed</span>
                  <span class="r">
                    {row.free === null
                      ? "—"
                      : `${formatBytes(row.free)} free now${
                          row.before === null
                            ? ""
                            : ` · was ${formatBytes(row.before)}`
                        }`}
                  </span>
                </div>
              )}
            </For>
          </div>
        </section>
      </Show>

      {/* Skipped items — show exactly what was left behind and why. */}
      <Show when={progress().skippedItems.length > 0}>
        <section class="card skip-card">
          <div class="skip-head">
            <span class="t">
              {progress().skippedItems.length} item
              {progress().skippedItems.length === 1 ? "" : "s"} skipped
            </span>
            <span class="s">These weren't removed — see why below.</span>
          </div>
          <ul class="skip-list">
            <For each={progress().skippedItems}>
              {(item) => (
                <li class="skip-row">
                  <div class="skip-info">
                    <span class="skip-name" title={item.path}>
                      {baseName(item.path)}
                    </span>
                    <span class="skip-path" title={item.path}>
                      {item.path}
                    </span>
                    <span class="skip-reason">{item.reason}</span>
                  </div>
                  <Show when={item.path}>
                    <button
                      type="button"
                      class="btn btn-ghost skip-reveal"
                      title="Show in File Explorer"
                      onClick={() => {
                        void openPath(item.path).catch(() => {});
                      }}
                    >
                      Reveal
                    </button>
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </section>
      </Show>
    </div>
  );
};

export default Done;
