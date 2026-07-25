import { type Component, For, type JSX, Show } from "solid-js";
import { appStore, type View } from "../state/store";
import { formatBytes } from "../lib/format";
import { meterCells, usedPercent } from "../lib/sky";
import { NAV_ITEMS, SCREEN_TITLE } from "./nav";

/**
 * Shell for the `sky` layout family (Nebula + Void).
 *
 * A 64px icon-only rail and a 56px header, with content in a centred column.
 * The narrow rail and the column's margins exist for the same reason: to leave
 * the night sky visible around the content. A full-bleed layout with a 214px
 * labelled rail covers the thing the theme is built around.
 *
 * Trade-off worth naming: dropping to icons removes the nav labels and the
 * persistent "lifetime reclaimed" figure that the old rail carried. The labels
 * are recovered as tooltips and `aria-label`s, and the lifetime figure moves
 * into the Overview stat row where it reads better anyway.
 */
const SkyShell: Component<{ children: JSX.Element }> = (props) => {
  const drive = () => appStore.state.driveBefore;
  const title = () => SCREEN_TITLE(appStore.state.view, appStore.state.phase);

  const go = (id: View) => appStore.setView(id);

  return (
    <div class="sky-shell">
      <nav class="sky-rail" aria-label="Sections">
        <div class="sky-mark" aria-hidden="true">
          S
        </div>
        <For each={NAV_ITEMS}>
          {(item) => (
            <button
              type="button"
              class="sky-nav"
              data-on={appStore.state.view === item.id ? "true" : "false"}
              aria-current={appStore.state.view === item.id ? "page" : undefined}
              aria-label={item.label}
              title={item.label}
              onClick={() => go(item.id)}
            >
              <item.Icon />
              {/* A quiet pulse while automation is mid-run — the icon rail has
                  no room for a text badge. */}
              <Show
                when={item.id === "automation" && appStore.state.automation?.running}
              >
                <span class="sky-nav-pulse" aria-hidden="true" />
              </Show>
            </button>
          )}
        </For>
        <div class="grow" />
      </nav>

      <div class="sky-main">
        <header class="sky-top">
          <span class="ttl">{title()}</span>
          <Show when={drive()} keyed>
            {(d) => {
              const used = usedPercent(d.freeBytes, d.totalBytes);
              return (
                <div class="drv">
                  <span>
                    {d.mount} · {formatBytes(d.freeBytes)} free
                  </span>
                  <span class="drive-meter" aria-hidden="true">
                    <For each={meterCells(used)}>
                      {(state) => <i data-fill={state} />}
                    </For>
                  </span>
                  <span class="pc">{used}%</span>
                </div>
              );
            }}
          </Show>
        </header>

        <main class="sky-body scroll-region">
          <div class="sky-col">{props.children}</div>
        </main>
      </div>
    </div>
  );
};

export default SkyShell;
