import { type Component, For, Show } from "solid-js";
import { appStore } from "../../state/store";
import { formatBytes, relativeTime } from "../../lib/format";
import { useClean } from "./model";

const TICK = (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="3.4"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M20 6L9 17l-5-5" />
  </svg>
);

/**
 * Clean · review, `dense` family.
 *
 * One flat table of every finding, globally sorted largest-first — no category
 * grouping and no expansion. That's the philosophical difference: the sky layout
 * says "here are your categories, drill in", the instrument panel says "here is
 * everything, sort it yourself".
 *
 * Every column the old inspector duplicated (path, category, tier, modified,
 * size) is already on the row, so clicking anywhere on a row simply toggles its
 * selection.
 */
const CleanDense: Component<{ onRequestClean: () => void }> = (props) => {
  const m = useClean();

  const allSelected = () =>
    m.flat().length > 0 && m.flat().every((r) => appStore.state.selected[r.item.id]);

  const toggleAll = () => {
    const next = !allSelected();
    for (const row of m.flat()) appStore.toggleItem(row.item.id, next);
  };

  return (
    <>
      <div class="dense-panel" style={{ "margin-top": "0" }}>
        <div class="dense-phead">
          Findings — all paths, largest first
          <span class="r">{formatBytes(m.report()?.totalReclaimableBytes ?? 0)}</span>
        </div>
        <table class="dense-table">
          <thead>
            <tr>
              <th style={{ width: "34px" }}>
                <span
                  class="dense-chk"
                  data-on={allSelected() ? "true" : "false"}
                  role="checkbox"
                  aria-checked={allSelected()}
                  aria-label="Select all"
                  tabindex="0"
                  onClick={toggleAll}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      toggleAll();
                    }
                  }}
                >
                  {allSelected() ? TICK : null}
                </span>
              </th>
              <th>Path</th>
              <th>Category</th>
              <th>Tier</th>
              <th class="num">Modified</th>
              <th class="num">Size ↓</th>
            </tr>
          </thead>
          <tbody>
            <For each={m.flat()}>
              {(row) => {
                const checked = () => !!appStore.state.selected[row.item.id];
                // The whole row toggles selection: with no detail panel to
                // populate, a click has only one sensible meaning.
                return (
                  <tr
                    data-sel={checked() ? "true" : "false"}
                    onClick={() => appStore.toggleItem(row.item.id, !checked())}
                  >
                    <td>
                      <span
                        class="dense-chk"
                        data-on={checked() ? "true" : "false"}
                        role="checkbox"
                        aria-checked={checked()}
                        aria-label={`Select ${row.item.label}`}
                        tabindex="0"
                        onClick={(e) => {
                          e.stopPropagation();
                          appStore.toggleItem(row.item.id, !checked());
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            e.stopPropagation();
                            appStore.toggleItem(row.item.id, !checked());
                          }
                        }}
                      >
                        {checked() ? TICK : null}
                      </span>
                    </td>
                    <td class="path" title={row.item.path}>
                      {row.item.path}
                    </td>
                    <td>
                      <span
                        class="dense-swatch"
                        style={{ background: row.color }}
                        aria-hidden="true"
                      />
                      {row.categoryLabel}
                    </td>
                    <td>
                      <span class={`dense-tier ${row.item.tier}`}>
                        {row.item.tier}
                      </span>
                    </td>
                    <td class="num">
                      {row.item.lastModifiedSecs === null
                        ? "—"
                        : relativeTime(row.item.lastModifiedSecs)}
                    </td>
                    <td class="num">{formatBytes(row.item.sizeBytes)}</td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={(m.report()?.warnings.length ?? 0) > 0}>
        <div class="dense-panel">
          <div class="dense-phead">Warnings</div>
          <div class="dense-log">
            <For each={m.report()!.warnings}>{(w) => <div>{w}</div>}</For>
          </div>
        </div>
      </Show>

      {/* Sticky within the scroll region so the table can be long. */}
      <div class="dense-foot sticky">
        <span>
          {m.selectedCount()} of {m.flat().length} selected
        </span>
        <span class="mono">{formatBytes(m.selectedBytes())}</span>
        <div class="grow" />
        <button
          type="button"
          class="dense-btn"
          data-on={m.toRecycleBin() ? "true" : "false"}
          onClick={() => appStore.setDestination(true)}
        >
          Recycle Bin
        </button>
        <button
          type="button"
          class="dense-btn"
          data-on={!m.toRecycleBin() ? "true" : "false"}
          onClick={() => appStore.setDestination(false)}
        >
          Permanent
        </button>
        <button
          type="button"
          class="dense-btn pri"
          disabled={m.selectedCount() === 0}
          onClick={props.onRequestClean}
        >
          Delete {m.selectedCount()} item{m.selectedCount() === 1 ? "" : "s"}
        </button>
      </div>
    </>
  );
};

export default CleanDense;
