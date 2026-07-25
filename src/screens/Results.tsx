import { type Component, Show, createSignal } from "solid-js";
import { appStore } from "../state/store";
import { layoutFamily } from "../lib/layout";
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
      <Show
        when={m.hasItems()}
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
