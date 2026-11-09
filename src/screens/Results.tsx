import { type Component, For, Show, createSignal } from "solid-js";
import { appStore } from "../state/store";
import { layoutFamily } from "../lib/layout";
import { formatBytes } from "../lib/format";
import { useClean } from "./clean/model";
import CleanSky from "./clean/Sky";
import CleanDense from "./clean/Dense";
import ConfirmModal from "../components/ConfirmModal";

/**
 * Clean · review — dispatches to one of two layout families.
 *
 * `sky` groups findings by category with expandable sub-lists; `dense` shows one
 * flat, globally sorted table. Different DOM and a different
 * data shape, so they are separate components — but the selection state, the
 * dry-run confirmation and the orchestration all live here, above the split, so
 * the two variants cannot diverge on behaviour.
 */
const Results: Component = () => {
  const [modalOpen, setModalOpen] = createSignal(false);
  const m = useClean();

  const startClean = () => {
    const ids = appStore.selectedIds();
    if (ids.length === 0) return;
    setModalOpen(false);
    // Store-owned orchestration: the cleanup survives navigation between views.
    void appStore.runClean(ids, appStore.state.toRecycleBin);
  };

  const requestClean = () => {
    if (appStore.selectedCount() > 0) setModalOpen(true);
  };

  return (
    <>
      <Show when={m.driveTotals().length > 1}>
        <div class="drive-filter" role="group" aria-label="Filter by drive">
          <span class="lbl">Show findings on</span>
          <button
            type="button"
            class="chip"
            data-on={m.driveFilter() === null ? "true" : "false"}
            onClick={() => m.setDriveFilter(null)}
          >
            All drives
          </button>
          <For each={m.driveTotals()}>
            {(total) => (
              <Show when={total.mount}>
                {(mount) => (
                  <button
                    type="button"
                    class="chip"
                    data-on={m.driveFilter() === mount() ? "true" : "false"}
                    onClick={() => m.setDriveFilter(mount())}
                  >
                    {mount()} · {formatBytes(total.bytes)}
                  </button>
                )}
              </Show>
            )}
          </For>
        </div>
      </Show>

      <Show
        when={m.hasVisibleItems()}
        fallback={
          <Show
            when={m.hasAnyItems()}
            fallback={
              <div class="sky-empty">
                <div class="sky-eyebrow">ALL CLEAR</div>
                <div class="sky-display">NOTHING
                  <br />
                  TO CLEAN
                </div>
                <p class="sky-lede">
                  No reclaimable space found. Everything looks tidy.
                </p>
                <div class="sky-acts">
                  <button
                    type="button"
                    class="sky-btn"
                    onClick={() => appStore.setPhase("welcome")}
                  >
                    SCAN AGAIN
                  </button>
                </div>
              </div>
            }
          >
            <div class="sky-empty">
              <div class="sky-eyebrow">NOTHING HERE</div>
              <div class="sky-display">
                NOTHING TO CLEAN
                <br />
                ON {m.driveFilter()}
              </div>
              <p class="sky-lede">
                Other drives still have findings. Switch back to see them.
              </p>
              <div class="sky-acts">
                <button
                  type="button"
                  class="sky-btn"
                  onClick={() => m.setDriveFilter(null)}
                >
                  SHOW ALL DRIVES
                </button>
              </div>
            </div>
          </Show>
        }
      >
        <Show
          when={layoutFamily(appStore.state.theme) === "dense"}
          fallback={<CleanSky onRequestClean={requestClean} />}
        >
          <CleanDense onRequestClean={requestClean} />
        </Show>
      </Show>

      <ConfirmModal
        open={modalOpen()}
        ids={appStore.selectedIds()}
        toRecycleBin={appStore.state.toRecycleBin}
        onCancel={() => setModalOpen(false)}
        onConfirm={startClean}
      />
    </>
  );
};

export default Results;
