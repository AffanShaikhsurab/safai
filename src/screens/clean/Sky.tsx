import { type Component, For, Show, createSignal } from "solid-js";
import { appStore } from "../../state/store";
import { formatBytes, relativeTime } from "../../lib/format";
import { openPath } from "../../lib/tauri";
import { useClean } from "./model";

/**
 * Clean · review, `sky` family.
 *
 * Zero card boxes — one hairline list of category rows, each expanding into an
 * indented sub-list of paths. The previous version nested two levels of glass
 * card, which at 47 items produced a wall of boxes inside a 44vh inner
 * scroller. Here the page scrolls and the commit bar sticks.
 *
 * The headline is an instruction rather than a number: the number is the
 * Overview's job, and by the time you're here the question is "which of these".
 */
const CleanSky: Component<{ onRequestClean: () => void }> = (props) => {
  const m = useClean();
  const [open, setOpen] = createSignal<Record<string, boolean>>({});

  const isOpen = (key: string, first: boolean) => open()[key] ?? first;
  const toggleOpen = (key: string, first: boolean) =>
    setOpen((prev) => ({ ...prev, [key]: !(prev[key] ?? first) }));

  return (
    <div class="animate-rise">
      <div class="sky-eyebrow">REVIEW</div>
      <div class="sky-display">
        CHOOSE WHAT
        <br />
        TO CLEAR
      </div>
      <p class="sky-lede">
        Safe items are already selected. Open a category to see the exact folders
        before you commit — <b>{formatBytes(m.report()?.totalReclaimableBytes ?? 0)}</b>{" "}
        is available in total.
      </p>

      <div class="sky-sec">
        <h3>FINDINGS</h3>
        <span class="m">{m.groups().length} categories</span>
        <button
          type="button"
          class="lk"
          onClick={() => appStore.selectByTier("safe", true)}
        >
          Select all safe
        </button>
      </div>

      <div class="sky-list">
        <For each={m.groups()}>
          {(g, i) => {
            const key = g.group.category;
            const first = i() === 0;
            return (
              <>
                <div class="sky-row as-group">
                  <span
                    class="sky-check"
                    data-on={g.allSelected ? "true" : "false"}
                    data-mixed={g.someSelected ? "true" : "false"}
                    role="checkbox"
                    aria-checked={g.allSelected}
                    aria-label={`Select all in ${g.label}`}
                    tabindex="0"
                    onClick={(e) => {
                      e.stopPropagation();
                      appStore.toggleCategory(g.group.category, !g.allSelected);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        appStore.toggleCategory(g.group.category, !g.allSelected);
                      }
                    }}
                  />
                  <span class="sw" style={{ background: g.color }} aria-hidden="true" />
                  <button
                    type="button"
                    class="txt as-btn"
                    aria-expanded={isOpen(key, first)}
                    onClick={() => toggleOpen(key, first)}
                  >
                    <span class="nm">{g.label}</span>
                    <span class="ds">
                      {g.items.length} item{g.items.length === 1 ? "" : "s"} ·{" "}
                      {g.blurb}
                    </span>
                  </button>
                  <span class={`sky-tier ${g.tier}`}>{g.tier}</span>
                  <span class="amt">
                    <b>{formatBytes(g.group.totalBytes)}</b>
                    <span>{isOpen(key, first) ? "hide" : "show"}</span>
                  </span>
                </div>

                <Show when={isOpen(key, first)}>
                  <div class="sky-sub">
                    <For each={g.items}>
                      {(item) => {
                        const checked = () => !!appStore.state.selected[item.id];
                        return (
                          <div class="sky-file">
                            <span
                              class="sky-check"
                              data-on={checked() ? "true" : "false"}
                              role="checkbox"
                              aria-checked={checked()}
                              aria-label={`Select ${item.label}`}
                              tabindex="0"
                              onClick={() => appStore.toggleItem(item.id, !checked())}
                              onKeyDown={(e) => {
                                if (e.key === "Enter" || e.key === " ") {
                                  e.preventDefault();
                                  appStore.toggleItem(item.id, !checked());
                                }
                              }}
                            />
                            <span class="fmeta">
                              <span class="fl">{item.label}</span>
                              <span class="fp" title={item.path}>
                                {item.path}
                              </span>
                            </span>
                            <Show when={item.lastModifiedSecs !== null}>
                              <span class="fstale">
                                {relativeTime(item.lastModifiedSecs as number)}
                              </span>
                            </Show>
                            <span class="fs">{formatBytes(item.sizeBytes)}</span>
                            <button
                              type="button"
                              class="freveal"
                              title={`Reveal ${item.path}`}
                              onClick={() => void openPath(item.path).catch(() => {})}
                            >
                              Reveal
                            </button>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </>
            );
          }}
        </For>
      </div>

      <Show when={(m.report()?.warnings.length ?? 0) > 0}>
        <div class="sky-sec">
          <h3>WARNINGS</h3>
        </div>
        <div class="sky-warn">
          <For each={m.report()!.warnings}>{(w) => <div>{w}</div>}</For>
        </div>
      </Show>

      {/* Sticky commit bar — negative margins bleed it to the column edges. */}
      <div class="sky-commit">
        <div class="sel">
          Clearing <b>{formatBytes(m.selectedBytes())}</b> · {m.selectedCount()} item
          {m.selectedCount() === 1 ? "" : "s"}
        </div>
        <div class="grow" />
        <div class="sky-seg" role="group" aria-label="Deletion destination">
          <button
            type="button"
            data-on={m.toRecycleBin() ? "true" : "false"}
            onClick={() => appStore.setDestination(true)}
          >
            Recycle Bin
          </button>
          <button
            type="button"
            data-on={!m.toRecycleBin() ? "true" : "false"}
            onClick={() => appStore.setDestination(false)}
          >
            Permanent
          </button>
        </div>
        <button
          type="button"
          class="sky-btn"
          disabled={m.selectedCount() === 0}
          onClick={props.onRequestClean}
        >
          CLEAN UP
        </button>
      </div>
    </div>
  );
};

export default CleanSky;
