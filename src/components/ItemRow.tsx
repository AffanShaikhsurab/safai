import { type Component, Show } from "solid-js";
import type { CleanupItem } from "../lib/types";
import { appStore } from "../state/store";
import { openPath } from "../lib/tauri";
import { formatBytes, relativeTime } from "../lib/format";

/**
 * A single cleanup finding (review list row): tier dot, mint check, name +
 * path, size and staleness. The check follows Solid's controlled pattern —
 * value in from the store leaf, change out via `toggleItem`. A hover "Reveal"
 * opens the path in the system explorer.
 */
const ItemRow: Component<{ item: CleanupItem }> = (props) => {
  const checked = () => appStore.state.selected[props.item.id] ?? false;

  const toggle = () =>
    appStore.toggleItem(props.item.id, !checked());

  const reveal = (e: MouseEvent) => {
    e.stopPropagation();
    void openPath(props.item.path);
  };

  return (
    <div
      class="row group"
      role="button"
      tabindex="0"
      aria-pressed={checked()}
      onClick={toggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle();
        }
      }}
    >
      <span class={`tier-dot ${props.item.tier}`} aria-hidden="true" />

      <span
        class="check"
        data-on={checked() ? "true" : "false"}
        title="Select for cleanup"
        onClick={(e) => e.stopPropagation()}
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="3"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M5 12l5 5 9-11" />
        </svg>
        <input
          type="checkbox"
          class="sr-check"
          checked={checked()}
          onChange={toggle}
          aria-label={`Select ${props.item.label}`}
        />
      </span>

      <div class="info">
        <div class="n">
          <span
            style={{
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "white-space": "nowrap",
            }}
          >
            {props.item.label}
          </span>
          <Show when={props.item.regenerates}>
            <span
              class="pill"
              style={{
                padding: "1px 8px",
                "font-size": "10px",
                "text-transform": "uppercase",
                "letter-spacing": "0.5px",
                color: "var(--mint-strong)",
                "border-color": "rgba(110,231,183,0.3)",
              }}
            >
              regenerates
            </span>
          </Show>
        </div>
        <div class="p" title={props.item.path}>
          {props.item.path}
        </div>
      </div>

      <div class="rsz">
        {formatBytes(props.item.sizeBytes)}
        <Show when={props.item.lastModifiedSecs !== null}>
          <span class="stale">
            {relativeTime(props.item.lastModifiedSecs as number)}
          </span>
        </Show>
      </div>

      <button
        type="button"
        class="opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
        style={{
          background: "transparent",
          border: "1px solid var(--border)",
          color: "var(--muted)",
          "border-radius": "999px",
          padding: "4px 10px",
          "font-size": "11px",
          cursor: "pointer",
          flex: "none",
        }}
        onClick={reveal}
        title={`Reveal in file explorer — ${props.item.path}`}
      >
        Reveal
      </button>
    </div>
  );
};

export default ItemRow;
