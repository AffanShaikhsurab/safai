import { type Component, For, Show, createSignal } from "solid-js";
import { appStore } from "../../state/store";
import { formatBytes } from "../../lib/format";
import { openPath } from "../../lib/tauri";
import {
  INACTIVE_DAY_CHOICES,
  formatIdle,
  isInactive,
  type DevProject,
} from "../../lib/devprojects";
import { useClean } from "./model";

/**
 * Dev projects on the review screen: every project with regenerable folders
 * (node_modules, target, .gradle, .dart_tool, venvs…), most inactive first.
 * Picking a threshold selects the folders of every project idle at least that
 * long — the user still confirms before anything is deleted.
 */

const sumBytes = (ps: DevProject[]) => ps.reduce((n, p) => n + p.totalBytes, 0);

const idleLabel = (p: DevProject) =>
  p.idleDays === null ? "activity unknown" : `idle ${formatIdle(p.idleDays)}`;

function onKeyToggle(e: KeyboardEvent, fn: () => void) {
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    e.stopPropagation();
    fn();
  }
}

/** Sky-family layout: a hairline list like the category view. */
export const ProjectsSky: Component = () => {
  const m = useClean();
  const [open, setOpen] = createSignal<Record<string, boolean>>({});

  return (
    <Show when={m.projects().length > 0}>
      <div class="sky-sec">
        <h3>DEV PROJECTS</h3>
        <span class="m">
          {m.inactiveProjects().length} of {m.projects().length} idle{" "}
          {appStore.state.devInactiveDays}+ days ·{" "}
          {formatBytes(sumBytes(m.inactiveProjects()))}
        </span>
        <div class="sky-seg" role="group" aria-label="Inactive for at least">
          <For each={INACTIVE_DAY_CHOICES}>
            {(days) => (
              <button
                type="button"
                data-on={appStore.state.devInactiveDays === days ? "true" : "false"}
                aria-pressed={appStore.state.devInactiveDays === days}
                onClick={() => appStore.setDevInactiveDays(days)}
              >
                {days}d
              </button>
            )}
          </For>
        </div>
      </div>
      <p class="sky-lede">
        Dependency and build folders from your projects. Everything here comes
        back with one install or build command. Projects idle for{" "}
        <b>{appStore.state.devInactiveDays}+ days</b> are selected.
      </p>

      <div class="sky-list">
        <For each={m.projects()}>
          {(p) => {
            const isOpen = () => !!open()[p.root];
            const toggle = () =>
              appStore.toggleProject(p.root, !m.projectSelected(p));
            return (
              <>
                <div class="sky-row as-group">
                  <span
                    class="sky-check"
                    data-on={m.projectSelected(p) ? "true" : "false"}
                    data-mixed={m.projectMixed(p) ? "true" : "false"}
                    role="checkbox"
                    aria-checked={m.projectSelected(p)}
                    aria-label={`Select ${p.name}`}
                    tabindex="0"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggle();
                    }}
                    onKeyDown={(e) => onKeyToggle(e, toggle)}
                  />
                  <button
                    type="button"
                    class="txt as-btn"
                    aria-expanded={isOpen()}
                    onClick={() => setOpen((o) => ({ ...o, [p.root]: !o[p.root] }))}
                  >
                    <span class="nm">{p.name}</span>
                    <span class="ds">
                      {p.kind} · {idleLabel(p)} · {p.items.length} folder
                      {p.items.length === 1 ? "" : "s"}
                    </span>
                  </button>
                  <span
                    class={`sky-tier ${
                      isInactive(p, appStore.state.devInactiveDays) ? "safe" : "review"
                    }`}
                  >
                    {isInactive(p, appStore.state.devInactiveDays) ? "inactive" : "active"}
                  </span>
                  <span class="amt">
                    <b>{formatBytes(p.totalBytes)}</b>
                    <span>{isOpen() ? "hide" : "show"}</span>
                  </span>
                </div>
                <Show when={isOpen()}>
                  <div class="sky-sub">
                    <For each={p.items}>
                      {(item) => {
                        const checked = () => !!appStore.state.selected[item.id];
                        const flip = () => appStore.toggleItem(item.id, !checked());
                        return (
                          <div class="sky-file">
                            <span
                              class="sky-check"
                              data-on={checked() ? "true" : "false"}
                              role="checkbox"
                              aria-checked={checked()}
                              aria-label={`Select ${item.path}`}
                              tabindex="0"
                              onClick={flip}
                              onKeyDown={(e) => onKeyToggle(e, flip)}
                            />
                            <span class="fmeta">
                              <span class="fl">
                                {item.dev?.ecosystem} ·{" "}
                                {item.path.slice(p.root.length + 1) || item.path}
                              </span>
                              <span class="fp" title={item.note}>
                                {item.note}
                              </span>
                            </span>
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
    </Show>
  );
};

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

/** Dense-family layout: one table row per project. */
export const ProjectsDense: Component = () => {
  const m = useClean();

  return (
    <Show when={m.projects().length > 0}>
      <div class="dense-panel" style={{ "margin-top": "0" }}>
        <div class="dense-phead">
          Dev projects — most inactive first ·{" "}
          {m.inactiveProjects().length} idle {appStore.state.devInactiveDays}+ days
          <span class="r">{formatBytes(sumBytes(m.inactiveProjects()))}</span>
        </div>
        <div
          style={{ display: "flex", gap: "6px", padding: "8px 12px", "align-items": "center" }}
          role="group"
          aria-label="Inactive for at least"
        >
          <span>Inactive for at least</span>
          <For each={INACTIVE_DAY_CHOICES}>
            {(days) => (
              <button
                type="button"
                class="dense-btn"
                data-on={appStore.state.devInactiveDays === days ? "true" : "false"}
                aria-pressed={appStore.state.devInactiveDays === days}
                onClick={() => appStore.setDevInactiveDays(days)}
              >
                {days} days
              </button>
            )}
          </For>
        </div>
        <table class="dense-table">
          <thead>
            <tr>
              <th style={{ width: "34px" }} />
              <th>Project</th>
              <th>Kind</th>
              <th>Folders</th>
              <th class="num">Idle</th>
              <th class="num">Size</th>
            </tr>
          </thead>
          <tbody>
            <For each={m.projects()}>
              {(p) => {
                const on = () => m.projectSelected(p);
                const toggle = () => appStore.toggleProject(p.root, !on());
                return (
                  <tr data-sel={on() ? "true" : "false"} onClick={toggle}>
                    <td>
                      <span
                        class="dense-chk"
                        data-on={on() ? "true" : "false"}
                        role="checkbox"
                        aria-checked={on()}
                        aria-label={`Select ${p.name}`}
                        tabindex="0"
                        onClick={(e) => {
                          e.stopPropagation();
                          toggle();
                        }}
                        onKeyDown={(e) => onKeyToggle(e, toggle)}
                      >
                        {on() ? TICK : null}
                      </span>
                    </td>
                    <td class="path" title={p.root}>
                      {p.root}
                    </td>
                    <td>{p.kind}</td>
                    <td
                      class="path"
                      title={p.items.map((it) => it.path).join("\n")}
                    >
                      {p.items
                        .map((it) => it.path.slice(p.root.length + 1) || it.path)
                        .join(", ")}
                    </td>
                    <td class="num">{formatIdle(p.idleDays)}</td>
                    <td class="num">{formatBytes(p.totalBytes)}</td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  );
};
