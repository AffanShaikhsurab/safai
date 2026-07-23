import { type Component, For, Show, createMemo, createSignal } from "solid-js";
import type { CategoryGroup } from "../lib/types";
import { appStore } from "../state/store";
import { formatBytes } from "../lib/format";
import { categoryMeta } from "../lib/categories";
import ItemRow from "./ItemRow";

/**
 * A review category card. The header (icon + name + item count + total + master
 * switch) is always visible so you can select or skim a whole section at a
 * glance; a chevron toggles the item rows so long lists stay readable. Items
 * arrive already sorted largest-first (see store.setReport).
 */
const CategoryCard: Component<{
  group: CategoryGroup;
  defaultOpen?: boolean;
}> = (props) => {
  const meta = createMemo(() => categoryMeta(props.group.category));
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);

  const count = () => props.group.items.length;
  const selectedCount = createMemo(
    () =>
      props.group.items.filter((item) => appStore.state.selected[item.id])
        .length,
  );
  const allSelected = () =>
    props.group.items.length > 0 &&
    selectedCount() === props.group.items.length;
  const someSelected = () =>
    selectedCount() > 0 && selectedCount() < props.group.items.length;

  const onToggleAll = () => {
    appStore.toggleCategory(props.group.category, !allSelected());
  };

  return (
    <section class="card cat" classList={{ "cat-open": open() }}>
      <div class="head">
        {/* clicking the icon/name area toggles the section */}
        <button
          type="button"
          class="cat-toggle"
          aria-expanded={open()}
          aria-label={`${open() ? "Collapse" : "Expand"} ${meta().label}`}
          onClick={() => setOpen(!open())}
        >
          <span class="icon" aria-hidden="true">
            {meta().Icon({})}
          </span>
          <span class="t">
            <span class="name">{meta().label}</span>
            <span class="desc">
              {count()} item{count() === 1 ? "" : "s"} · {meta().description}
            </span>
          </span>
          <svg
            class="chev"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </button>

        <div class="sz">{formatBytes(props.group.totalBytes)}</div>
        <span
          class="switch"
          data-on={allSelected() ? "true" : "false"}
          data-mixed={someSelected() ? "true" : "false"}
          role="switch"
          aria-checked={allSelected()}
          aria-label={`Select all in ${meta().label}`}
        >
          <input
            type="checkbox"
            class="sr-check"
            checked={allSelected()}
            onChange={onToggleAll}
          />
        </span>
      </div>

      <Show when={open()}>
        <div class="rows">
          <For each={props.group.items}>{(item) => <ItemRow item={item} />}</For>
        </div>
      </Show>
    </section>
  );
};

export default CategoryCard;
