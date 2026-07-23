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

  const freeNow = () => after()?.freeBytes ?? before()?.freeBytes ?? 0;
  const mount = () => after()?.mount ?? before()?.mount ?? "Drive C:";

  return (
    <div class="stage animate-rise">
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Free space</div>
            <div class="s">{mount()}</div>
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
