import { type Component, For, Show, createMemo, createSignal } from "solid-js";
import { appStore } from "../state/store";
import { formatBytes } from "../lib/format";
import CategoryCard from "../components/CategoryCard";
import ConfirmModal from "../components/ConfirmModal";

/**
 * Clean · review — the big Reclaimable header, clean category cards with item
 * rows, and a sticky action bar (running selection, destination segmented
 * control, Clean up). Selection + dry-run confirmation wiring is preserved.
 */
const Results: Component = () => {
  const [modalOpen, setModalOpen] = createSignal(false);

  const report = () => appStore.state.report;
  const selectedBytes = () => appStore.reclaimableSelectedBytes();
  const selectedCount = () => appStore.selectedCount();
  const toRecycleBin = () => appStore.state.toRecycleBin;

  const hasItems = createMemo(() => {
    const r = report();
    return !!r && r.groups.some((g) => g.items.length > 0);
  });

  const startClean = () => {
    const ids = appStore.selectedIds();
    if (ids.length === 0) return;
    setModalOpen(false);
    // Store-owned orchestration: the cleanup survives navigation between views.
    void appStore.runClean(ids, toRecycleBin());
  };

  return (
    <div class="stage stage-top animate-rise">
      <Show
        when={report() && hasItems()}
        fallback={
          <div class="card empty-panel">
            <div class="t">Your disk is clear</div>
            <p class="s">
              No reclaimable space found. Everything looks tidy.
            </p>
            <button
              type="button"
              class="btn btn-primary"
              onClick={() => appStore.setPhase("welcome")}
            >
              Scan again
            </button>
          </div>
        }
      >
        {/* Header */}
        <div class="rev-head">
          <div>
            <div class="k">Reclaimable</div>
            <div class="big">
              {formatBytes(report()!.totalReclaimableBytes)}
            </div>
            <div class="sub">Safe items are pre-selected. Review the rest.</div>
          </div>
          <button
            type="button"
            class="btn btn-ghost"
            onClick={() => appStore.setPhase("welcome")}
          >
            Back
          </button>
        </div>

        {/* Category list */}
        <div class="rev-list">
          <For each={report()!.groups}>
            {(group, i) => <CategoryCard group={group} defaultOpen={i() === 0} />}
          </For>

          <Show when={report()!.warnings.length > 0}>
            <section class="card warn-card">
              <p class="t">Warnings</p>
              <ul>
                <For each={report()!.warnings}>{(w) => <li>{w}</li>}</For>
              </ul>
            </section>
          </Show>
        </div>

        {/* Action bar */}
        <div class="card actionbar">
          <div class="sel">
            Selected:{" "}
            <b>
              {selectedCount()} item{selectedCount() === 1 ? "" : "s"} ·{" "}
              {formatBytes(selectedBytes())}
            </b>
          </div>
          <div class="right">
            <div
              class="segmented"
              role="group"
              aria-label="Deletion destination"
            >
              <button
                type="button"
                data-on={toRecycleBin() ? "true" : "false"}
                onClick={() => appStore.setDestination(true)}
              >
                Recycle
              </button>
              <button
                type="button"
                data-on={!toRecycleBin() ? "true" : "false"}
                onClick={() => appStore.setDestination(false)}
              >
                Permanent
              </button>
            </div>
            <button
              type="button"
              class="btn btn-primary"
              disabled={selectedCount() === 0}
              onClick={() => setModalOpen(true)}
            >
              Clean up
            </button>
          </div>
        </div>
      </Show>

      <ConfirmModal
        open={modalOpen()}
        ids={appStore.selectedIds()}
        toRecycleBin={toRecycleBin()}
        onCancel={() => setModalOpen(false)}
        onConfirm={startClean}
      />
    </div>
  );
};

export default Results;
