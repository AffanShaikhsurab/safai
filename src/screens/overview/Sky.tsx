import { type Component, For, Show } from "solid-js";
import { appStore } from "../../state/store";
import { formatBytes, relativeTime } from "../../lib/format";
import { useOverview } from "./model";

/**
 * Overview, `sky` family (Nebula + Void).
 *
 * The reclaimable figure leads at display size. That's the change that matters
 * most versus the old dashboard, where the number the app exists to produce sat
 * in a KPI tile between two vanity stats while the word "Overview" got the
 * largest type on screen.
 *
 * No card boxes anywhere: a hairline stat grid and hairline rows. Fewer edges
 * means less competing with the sky the theme is built around.
 */
const OverviewSky: Component = () => {
  const m = useOverview();

  const startScan = () => {
    appStore.setView("clean");
    const p = appStore.state.phase;
    if (p !== "scanning" && p !== "cleaning") appStore.setPhase("welcome");
  };

  const review = () => {
    appStore.setView("clean");
    appStore.setPhase("results");
  };

  const automation = () => appStore.state.automation;

  return (
    <div class="animate-rise">
      <Show
        when={m.reclaimable() > 0}
        fallback={
          <>
            <div class="sky-eyebrow">NOTHING FOUND YET</div>
            <div class="sky-display">
              READY WHEN
              <br />
              YOU ARE
            </div>
            <p class="sky-lede">
              Safai looks through the places developer tools quietly pile up —
              package caches, build output, editor history — and tells you what's
              safe to clear.
            </p>
            <div class="sky-acts">
              <button type="button" class="sky-btn" onClick={startScan}>
                SCAN MY DRIVE
              </button>
            </div>
          </>
        }
      >
        <div class="sky-eyebrow">READY TO RECLAIM</div>
        <div class="sky-display">
          {formatBytes(m.reclaimable())
            .replace(/\s*([A-Za-z]+)$/, "|$1")
            .split("|")
            .map((part, i) =>
              i === 1 ? <span class="unit">{part}</span> : <>{part} </>,
            )}
        </div>
        <p class="sky-lede">
          Across <b>{m.itemCount()} items</b>.
          <Show when={m.drive()} keyed>
            {(d) => (
              <>
                {" "}
                That takes {d.mount} from {formatBytes(d.freeBytes)} to{" "}
                <b>{formatBytes(m.freeAfter())}</b> free
              </>
            )}
          </Show>
          <Show when={appStore.state.toRecycleBin} fallback={<>.</>}>
            {" "}
            — and everything goes to the Recycle Bin first.
          </Show>
        </p>
        <div class="sky-acts">
          <button type="button" class="sky-btn" onClick={review}>
            FREE UP SPACE
          </button>
          <button type="button" class="sky-btn quiet" onClick={startScan}>
            SCAN AGAIN
          </button>
        </div>
      </Show>

      {/* Hairline stat grid — no boxes. */}
      <div class="sky-stats">
        <div class="sky-stat">
          <div class="k">In use</div>
          <div class="v">{m.usedPct()}%</div>
          <div class="n">
            <Show when={m.drive()} keyed fallback={<>drive unavailable</>}>
              {(d) => (
                <>
                  {formatBytes(d.totalBytes - d.freeBytes)} of{" "}
                  {formatBytes(d.totalBytes)}
                </>
              )}
            </Show>
          </div>
        </div>
        <div class="sky-stat">
          <div class="k">Lifetime</div>
          <div class="v">{formatBytes(m.stats().lifetimeReclaimedBytes)}</div>
          <div class="n">
            {m.stats().cleanupCount} cleanup
            {m.stats().cleanupCount === 1 ? "" : "s"}
          </div>
        </div>
        <div class="sky-stat">
          <div class="k">Last scan</div>
          <Show
            when={m.stats().lastScanAt}
            keyed
            fallback={
              <>
                <div class="v">—</div>
                <div class="n">no scan yet</div>
              </>
            }
          >
            {(at) => (
              <>
                <div class="v">{relativeTime(at)}</div>
                <div class="n">{m.stats().lastScanItems ?? 0} items found</div>
              </>
            )}
          </Show>
        </div>
        <div class="sky-stat">
          <div class="k">Automation</div>
          <Show
            when={automation()}
            keyed
            fallback={
              <>
                <div class="v">—</div>
                <div class="n">not configured</div>
              </>
            }
          >
            {(a) => (
              <>
                <div
                  class="v"
                  style={{ color: a.config.enabled ? "var(--ok)" : undefined }}
                >
                  {a.running ? "RUN" : a.config.enabled ? "ON" : "OFF"}
                </div>
                <div class="n">
                  {a.config.enabled ? a.cadenceLabel : "switched off"}
                </div>
              </>
            )}
          </Show>
        </div>
      </div>

      {/* Where it went */}
      <Show when={m.rows().length > 0}>
        <div class="sky-sec">
          <h3>WHERE IT WENT</h3>
          <span class="m">
            {m.rows().length} categor{m.rows().length === 1 ? "y" : "ies"}
          </span>
          <button type="button" class="lk" onClick={review}>
            Open in Clean →
          </button>
        </div>
        <div class="sky-list">
          <For each={m.rows()}>
            {(row) => (
              <button type="button" class="sky-row" onClick={review}>
                <span class="sw" style={{ background: row.color }} aria-hidden="true" />
                <span class="txt">
                  <span class="nm">{row.label}</span>
                  <span class="ds">
                    {row.items} item{row.items === 1 ? "" : "s"} · {row.blurb}
                  </span>
                </span>
                {/* Discrete pixel cells rather than a smooth bar. */}
                <span class="pxbar" aria-hidden="true">
                  <For each={Array.from({ length: 14 }, (_, i) => i)}>
                    {(i) => (
                      <i
                        style={
                          i < Math.round((row.relative / 100) * 14)
                            ? { background: row.color }
                            : undefined
                        }
                      />
                    )}
                  </For>
                </span>
                <span class={`sky-tier ${row.tier}`}>{row.tier}</span>
                <span class="amt">
                  <b>{formatBytes(row.group.totalBytes)}</b>
                  <span>{row.share}%</span>
                </span>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default OverviewSky;
