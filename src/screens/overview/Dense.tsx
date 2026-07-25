import { type Component, For, Show } from "solid-js";
import { appStore } from "../../state/store";
import { formatBytes, relativeTime } from "../../lib/format";
import { needsDarkInk } from "../../lib/layout";
import { squarify } from "../../lib/treemap";
import { useOverview, type GroupRow } from "./model";

/**
 * Overview, `dense` family (Pulsar).
 *
 * A different philosophy: a metric strip, a proportional disk map, and a
 * sortable breakdown table. Nothing is centred, nothing is decorative, and the
 * numbers are tabular so columns line up.
 *
 * The disk map uses a real squarified treemap (`lib/treemap.ts`), not the
 * mockup's hardcoded three-column arrangement — that assumed exactly six
 * categories in one specific size order, and real reports have one to seven.
 */
const OverviewDense: Component = () => {
  const m = useOverview();

  const tiles = () =>
    squarify(
      m.rows().map((row) => ({ id: row.group.category, value: row.group.totalBytes, row })),
    );

  /** Both the map and the table jump straight into review. */
  const openInClean = () => {
    appStore.setView("clean");
    appStore.setPhase("results");
  };

  return (
    <>
      {/* Metric strip — only figures the backend actually provides. */}
      <div class="dense-metrics">
        <div class="dense-metric">
          <div class="k">Reclaimable</div>
          <div class="v key">{formatBytes(m.reclaimable())}</div>
          <div class="d">{m.itemCount()} items</div>
        </div>
        <div class="dense-metric">
          <div class="k">Free</div>
          <div class="v">{formatBytes(m.drive()?.freeBytes ?? 0)}</div>
          <div class="d">{m.usedPct()}% used</div>
        </div>
        <div class="dense-metric">
          <div class="k">After cleanup</div>
          <div class="v">{formatBytes(m.freeAfter())}</div>
          <div class="d">{m.usedAfterPct()}% used</div>
        </div>
        <div class="dense-metric">
          <div class="k">Lifetime</div>
          <div class="v">{formatBytes(m.stats().lifetimeReclaimedBytes)}</div>
          <div class="d">{m.stats().cleanupCount} runs</div>
        </div>
        <div class="dense-metric">
          <div class="k">Last scan</div>
          <div class="v">
            {m.stats().lastScanAt ? relativeTime(m.stats().lastScanAt!) : "—"}
          </div>
          <div class="d">{m.stats().lastScanItems ?? 0} items</div>
        </div>
      </div>

      <Show
        when={m.rows().length > 0}
        fallback={
          <div class="dense-panel">
            <div class="dense-phead">Disk map</div>
            <p class="dense-empty">No scan data. Run a scan to populate the map.</p>
          </div>
        }
      >
        {/* Disk map */}
        <div class="dense-panel">
          <div class="dense-phead">
            Disk map — reclaimable by category
            <span class="r">{formatBytes(m.reclaimable())}</span>
          </div>
          <div class="dense-treemap">
            <For each={tiles()}>
              {(tile) => {
                const row = tile.item.row as GroupRow;
                return (
                  <button
                    type="button"
                    class="dense-tile"
                    title={`${row.label} — ${formatBytes(row.group.totalBytes)}`}
                    style={{
                      left: `${tile.x}%`,
                      top: `${tile.y}%`,
                      width: `${tile.w}%`,
                      height: `${tile.h}%`,
                      background: row.color,
                      // Contrast flip computed from luminance, so it stays
                      // correct for every ramp and any number of groups.
                      color: needsDarkInk(row.color) ? "#0a0b0f" : "#e6e9f2",
                    }}
                    onClick={openInClean}
                  >
                    <span class="tn">{row.label}</span>
                    <span class="tv">{formatBytes(row.group.totalBytes)}</span>
                  </button>
                );
              }}
            </For>
          </div>
        </div>

        {/* Breakdown table */}
        <div class="dense-panel">
          <div class="dense-phead">Breakdown</div>
          <table class="dense-table">
            <thead>
              <tr>
                <th>Category</th>
                <th>Tier</th>
                <th class="num">Items</th>
                <th class="num">Size ↓</th>
                <th class="num">Share</th>
              </tr>
            </thead>
            <tbody>
              <For each={m.rows()}>
                {(row) => (
                  <tr onClick={openInClean}>
                    <td>
                      <span
                        class="dense-swatch"
                        style={{ background: row.color }}
                        aria-hidden="true"
                      />
                      {row.label}
                    </td>
                    <td>
                      <span class={`dense-tier ${row.tier}`}>{row.tier}</span>
                    </td>
                    <td class="num">{row.items}</td>
                    <td class="num">{formatBytes(row.group.totalBytes)}</td>
                    <td class="num">{row.share}%</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </>
  );
};

export default OverviewDense;
