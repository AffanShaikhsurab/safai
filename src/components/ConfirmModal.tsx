import {
  type Component,
  For,
  Show,
  Suspense,
  createResource,
} from "solid-js";
import { Portal } from "solid-js/web";
import { previewDelete } from "../lib/tauri";
import { formatBytes } from "../lib/format";
import TierBadge from "./TierBadge";

// Confirmation modal. When open, it runs `preview_delete` as a dry run so the
// user sees exactly what will be removed (and what is blocked) before deleting.
const ConfirmModal: Component<{
  open: boolean;
  ids: string[];
  toRecycleBin: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}> = (props) => {
  // Only fetch a plan while the modal is open (falsy source => no fetch).
  const [plan] = createResource(
    () => (props.open ? props.ids : null),
    (ids) => previewDelete(ids),
  );

  const destination = () =>
    props.toRecycleBin ? "the Recycle Bin" : "permanent deletion";

  return (
    <Show when={props.open}>
      <Portal>
        <div
          class="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4 backdrop-blur-sm"
          onClick={props.onCancel}
        >
          <div
            class="glass animate-rise w-full max-w-lg overflow-hidden"
            role="dialog"
            aria-modal="true"
            aria-label="Confirm cleanup"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="modal-rule border-b px-6 py-5">
              <h2 class="text-base font-semibold text-white">
                Move selected items to {destination()}?
              </h2>
              <p class="mt-1 text-sm text-white/60">
                Reviewing exactly what will be removed.
              </p>
              {/* Destination echo: sky blue for Recycle Bin, rose for permanent. */}
              <span
                class="pill mt-3"
                classList={{
                  "!border-sky-400/40 !bg-sky-400/10 text-sky-300": props.toRecycleBin,
                  "!border-rose-400/40 !bg-rose-400/10 text-rose-300": !props.toRecycleBin,
                }}
                title={props.toRecycleBin ? "Recoverable from the Recycle Bin" : "This cannot be undone"}
              >
                <span
                  class="h-1.5 w-1.5 rounded-full"
                  classList={{
                    "bg-sky-400": props.toRecycleBin,
                    "bg-rose-400": !props.toRecycleBin,
                  }}
                  aria-hidden="true"
                />
                <Show
                  when={props.toRecycleBin}
                  fallback={<>Destination: Permanent deletion</>}
                >
                  Destination: Recycle Bin (safe)
                </Show>
              </span>
            </div>

            <div class="scroll-region max-h-80 px-6 py-4">
              <Suspense
                fallback={
                  <p class="text-sm text-white/60">Computing plan…</p>
                }
              >
                <Show when={plan()} keyed>
                  {(p) => (
                    <div class="space-y-4">
                      <div class="glass flex items-center justify-between px-4 py-3">
                        <span class="text-sm text-white/75">
                          {p.items.filter((i) => i.allowed).length} item
                          {p.items.filter((i) => i.allowed).length === 1
                            ? ""
                            : "s"}{" "}
                          to remove
                        </span>
                        <span class="text-lg font-semibold tabular-nums text-sky-300 neon-soft">
                          {formatBytes(p.totalBytes)}
                        </span>
                      </div>

                      <Show when={p.blockedCount > 0}>
                        <div class="rounded-2xl border border-rose-400/40 bg-rose-400/10 px-4 py-3">
                          <p class="text-sm font-medium text-rose-300">
                            {p.blockedCount} item
                            {p.blockedCount === 1 ? "" : "s"} blocked by
                            guardrails and will be skipped.
                          </p>
                        </div>
                      </Show>

                      <ul class="space-y-1.5">
                        <For each={p.items}>
                          {(item) => (
                            <li
                              class="flex items-center gap-2 rounded-xl px-2 py-1.5 text-sm"
                              classList={{ "opacity-50": !item.allowed }}
                            >
                              <TierBadge tier={item.tier} />
                              <span
                                class="min-w-0 flex-1 truncate text-white/75"
                                title={item.path}
                              >
                                {item.path}
                              </span>
                              <Show
                                when={item.allowed}
                                fallback={
                                  <span class="shrink-0 text-xs text-rose-300">
                                    {item.reason ?? "blocked"}
                                  </span>
                                }
                              >
                                <span class="shrink-0 tabular-nums text-white/60">
                                  {formatBytes(item.sizeBytes)}
                                </span>
                              </Show>
                            </li>
                          )}
                        </For>
                      </ul>
                    </div>
                  )}
                </Show>
              </Suspense>
            </div>

            <div class="modal-rule flex justify-end gap-2 border-t px-6 py-4">
              <button
                type="button"
                class="btn btn-ghost !px-4 !py-2 !text-sm"
                onClick={props.onCancel}
              >
                Cancel
              </button>
              <button
                type="button"
                class="btn !px-4 !py-2 !text-sm"
                classList={{
                  "btn-primary": props.toRecycleBin,
                  "btn-danger": !props.toRecycleBin,
                }}
                disabled={plan.loading}
                onClick={props.onConfirm}
              >
                {props.toRecycleBin ? "Move to Recycle Bin" : "Delete permanently"}
              </button>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default ConfirmModal;
