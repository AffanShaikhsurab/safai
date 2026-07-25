import { type Component, For, Show, createMemo } from "solid-js";
import { appStore } from "../state/store";
import { formatBytes } from "../lib/format";
import {
  eligibleRules,
  formatRelative,
  isRuleAllowed,
  nextRuleAllowList,
} from "../lib/automation";
import type {
  Cadence,
  Category,
  RuleInfo,
  SafetyTier,
  TriggerKind,
} from "../lib/types";

// ---------------------------------------------------------------------------
// Static option tables
// ---------------------------------------------------------------------------

const CADENCES: { id: Cadence; label: string }[] = [
  { id: "daily", label: "Daily" },
  { id: "everyThreeDays", label: "Every 3 days" },
  { id: "weekly", label: "Weekly" },
  { id: "manual", label: "Off" },
];

/** Categories autopilot may be pointed at, in ascending order of risk. */
const AUTO_CATEGORIES: { id: Category; label: string; detail: string }[] = [
  {
    id: "packageCache",
    label: "Package caches",
    detail: "npm, pip, cargo, gradle… refill on the next install.",
  },
  {
    id: "temp",
    label: "Temporary files",
    detail: "Windows and app scratch space.",
  },
  {
    id: "browser",
    label: "Browser binaries",
    detail: "Playwright downloads, re-fetched on demand.",
  },
  {
    id: "buildArtifact",
    label: "Build artifacts",
    detail: "node_modules, target, dist — a rebuild brings them back.",
  },
  {
    id: "editorStorage",
    label: "Editor storage",
    detail: "Workspace state caches. Loses some editor history.",
  },
];

const TIER_LABELS: Record<SafetyTier, string> = {
  safe: "Safe",
  review: "Review",
  caution: "Caution",
};

const TRIGGER_LABELS: Record<TriggerKind, string> = {
  cadence: "Scheduled",
  threshold: "Disk full",
  manual: "Manual",
};

const GIB = 1024 ** 3;
const CAP_CHOICES = [5, 20, 50, 100];

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function formatClock(unixSecs: number | null): string {
  if (!unixSecs) return "";
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * Automation — the proactive side of Safai.
 *
 * Two independent triggers (a cadence and a disk-usage threshold), a set of
 * conditions that defer a run rather than cancel it, and an explicit choice
 * between "tell me" and "just handle it". Every control writes straight through
 * to the Rust scheduler, which sanitizes the value and echoes back the result —
 * so what's rendered here is always what the backend actually holds.
 */
const Automation: Component = () => {
  const status = () => appStore.state.automation;
  const cfg = () => appStore.state.automation?.config;
  const patch = appStore.patchAutomation;

  /** Rules the current tier + category choice would let autopilot touch. */
  const eligible = createMemo<RuleInfo[]>(() => {
    const c = cfg();
    return c ? eligibleRules(appStore.state.rules, c) : [];
  });

  const toggleCategory = (id: Category, on: boolean) => {
    const c = cfg();
    if (!c) return;
    const next = on
      ? [...new Set([...c.autoCleanCategories, id])]
      : c.autoCleanCategories.filter((x) => x !== id);
    void patch({ autoCleanCategories: next });
  };

  const toggleTier = (tier: SafetyTier, on: boolean) => {
    const c = cfg();
    if (!c) return;
    const next = on
      ? [...new Set([...c.autoCleanTiers, tier])]
      : c.autoCleanTiers.filter((x) => x !== tier);
    void patch({ autoCleanTiers: next });
  };

  const toggleRule = (id: string, on: boolean) => {
    const c = cfg();
    if (!c) return;
    void patch({
      autoCleanRuleIds: nextRuleAllowList(
        c.autoCleanRuleIds,
        eligible().map((r) => r.id),
        id,
        on,
      ),
    });
  };

  const ruleEnabled = (id: string) =>
    isRuleAllowed(cfg()?.autoCleanRuleIds ?? [], id);

  return (
    <div class="set-wrap animate-rise">
      <div>
        <div class="set-title">Automation</div>
        <div class="set-sub">
          Let Safai watch your disk and reclaim space on its own.
        </div>
      </div>

      <Show
        when={status() && cfg()}
        fallback={<div class="card set-card">Loading automation…</div>}
      >
        {/* ---------------------------------------------------------------
            Live status
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">Status</div>

          <div class="auto-status">
            <span
              class="auto-dot"
              data-state={
                status()!.running
                  ? "running"
                  : cfg()!.enabled
                    ? "armed"
                    : "off"
              }
              aria-hidden="true"
            />
            <div class="auto-status-text">
              <div class="rl">
                <Show when={status()!.running} fallback={
                  cfg()!.enabled ? "Armed" : "Automation is off"
                }>
                  {status()!.phase === "cleaning"
                    ? "Freeing up space…"
                    : "Scanning…"}
                </Show>
              </div>
              <div class="rd">
                <Show
                  when={status()!.running}
                  fallback={
                    <Show
                      when={status()!.deferredReason}
                      fallback={
                        cfg()!.enabled
                          ? `${status()!.cadenceLabel} · next run ${formatRelative(status()!.nextDueAt)}`
                          : "Nothing runs until you switch it on."
                      }
                    >
                      {status()!.deferredReason}
                    </Show>
                  }
                >
                  {status()!.phase === "cleaning"
                    ? `${status()!.progress.deleted} removed · ${formatBytes(status()!.progress.reclaimedBytes)} freed`
                    : `${status()!.progress.itemCount} found · ${formatBytes(status()!.progress.foundBytes)}`}
                </Show>
              </div>
            </div>

            <div class="set-actions">
              <Show
                when={status()!.running}
                fallback={
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    onClick={() => void appStore.triggerAutomationRun()}
                  >
                    Run now
                  </button>
                }
              >
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  onClick={() => void appStore.haltAutomationRun()}
                >
                  Stop
                </button>
              </Show>
              <span
                class="switch"
                data-on={cfg()!.enabled ? "true" : "false"}
                role="switch"
                aria-checked={cfg()!.enabled}
                aria-label="Enable automation"
              >
                <input
                  type="checkbox"
                  class="sr-check"
                  checked={cfg()!.enabled}
                  onChange={(e) =>
                    void patch({ enabled: e.currentTarget.checked })
                  }
                />
              </span>
            </div>
          </div>

          <Show when={status()!.running && status()!.progress.currentPath}>
            <div class="auto-path" title={status()!.progress.currentPath}>
              {status()!.progress.currentPath}
            </div>
          </Show>

          <Show when={status()!.disk}>
            <div class="auto-meter">
              <div class="auto-meter-head">
                <span>
                  {status()!.disk!.mount} · {formatBytes(status()!.disk!.freeBytes)} free
                </span>
                <span>{(status()!.diskUsedPercent ?? 0).toFixed(0)}% used</span>
              </div>
              <div class="auto-meter-track">
                <div
                  class="auto-meter-fill"
                  data-over={
                    (status()!.diskUsedPercent ?? 0) >= cfg()!.thresholdPercent
                      ? "true"
                      : "false"
                  }
                  style={{ width: `${Math.min(status()!.diskUsedPercent ?? 0, 100)}%` }}
                />
                <Show when={cfg()!.thresholdEnabled}>
                  <span
                    class="auto-meter-mark"
                    style={{ left: `${cfg()!.thresholdPercent}%` }}
                    title={`Trigger at ${cfg()!.thresholdPercent}% full`}
                  />
                </Show>
              </div>
            </div>
          </Show>
        </div>

        {/* ---------------------------------------------------------------
            When to run
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">When to run</div>

          <div class="set-row">
            <div>
              <div class="rl">On a schedule</div>
              <div class="rd">
                Counted from the last run, so a machine that was asleep catches
                up instead of skipping.
              </div>
            </div>
            <div class="segmented" role="group" aria-label="Cadence">
              <For each={CADENCES}>
                {(opt) => (
                  <button
                    type="button"
                    data-on={cfg()!.cadence === opt.id ? "true" : "false"}
                    onClick={() => void patch({ cadence: opt.id })}
                  >
                    {opt.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <Show when={cfg()!.cadence !== "manual"}>
            <div class="set-row">
              <div>
                <div class="rl">Preferred time</div>
                <div class="rd">Local time. Late hours keep it out of your way.</div>
              </div>
              <select
                class="auto-select"
                aria-label="Preferred hour"
                value={String(cfg()!.preferredHour)}
                onChange={(e) =>
                  void patch({ preferredHour: Number(e.currentTarget.value) })
                }
              >
                <For each={Array.from({ length: 24 }, (_, h) => h)}>
                  {(h) => (
                    <option value={String(h)}>
                      {String(h).padStart(2, "0")}:00
                    </option>
                  )}
                </For>
              </select>
            </div>
          </Show>

          <div class="set-row">
            <div>
              <div class="rl">When the drive fills up</div>
              <div class="rd">
                Checked every minute. Runs at most once every 6 hours so a
                stubbornly full drive isn't re-scanned on a loop.
              </div>
            </div>
            <span
              class="switch"
              data-on={cfg()!.thresholdEnabled ? "true" : "false"}
              role="switch"
              aria-checked={cfg()!.thresholdEnabled}
              aria-label="Run when the drive fills up"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={cfg()!.thresholdEnabled}
                onChange={(e) =>
                  void patch({ thresholdEnabled: e.currentTarget.checked })
                }
              />
            </span>
          </div>

          <Show when={cfg()!.thresholdEnabled}>
            <div class="set-row">
              <div>
                <div class="rl">Trigger at</div>
                <div class="rd">
                  {cfg()!.thresholdPercent}% full
                  <Show when={status()!.diskUsedPercent !== null}>
                    {" "}
                    · currently {(status()!.diskUsedPercent ?? 0).toFixed(0)}%
                  </Show>
                </div>
              </div>
              <input
                type="range"
                class="auto-range"
                min="50"
                max="99"
                step="1"
                aria-label="Disk usage trigger percentage"
                value={cfg()!.thresholdPercent}
                onInput={(e) =>
                  void patch({ thresholdPercent: Number(e.currentTarget.value) })
                }
              />
            </div>
          </Show>
        </div>

        {/* ---------------------------------------------------------------
            Conditions
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">Conditions</div>
          <div class="auto-note">
            Scheduled runs wait for these. They're held for at most 12 hours —
            after that the run happens anyway, so "only when idle" can't mean
            "never". A disk-full trigger ignores the idle wait.
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Only when I'm away</div>
              <div class="rd">
                Idle for {cfg()!.idleMinutes} minutes
                {" · "}
                currently {Math.floor(status()!.idleSecs / 60)}m idle
              </div>
            </div>
            <div class="set-actions">
              <Show when={cfg()!.runOnlyWhenIdle}>
                <select
                  class="auto-select"
                  aria-label="Idle minutes"
                  value={String(cfg()!.idleMinutes)}
                  onChange={(e) =>
                    void patch({ idleMinutes: Number(e.currentTarget.value) })
                  }
                >
                  <For each={[2, 5, 10, 15, 30, 60]}>
                    {(m) => <option value={String(m)}>{m} min</option>}
                  </For>
                </select>
              </Show>
              <span
                class="switch"
                data-on={cfg()!.runOnlyWhenIdle ? "true" : "false"}
                role="switch"
                aria-checked={cfg()!.runOnlyWhenIdle}
                aria-label="Only run when idle"
              >
                <input
                  type="checkbox"
                  class="sr-check"
                  checked={cfg()!.runOnlyWhenIdle}
                  onChange={(e) =>
                    void patch({ runOnlyWhenIdle: e.currentTarget.checked })
                  }
                />
              </span>
            </div>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Not on battery</div>
              <div class="rd">
                {status()!.onBattery ? "On battery right now." : "On power right now."}
                {" "}Overridden if the drive is nearly full.
              </div>
            </div>
            <span
              class="switch"
              data-on={cfg()!.skipOnBattery ? "true" : "false"}
              role="switch"
              aria-checked={cfg()!.skipOnBattery}
              aria-label="Skip when on battery"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={cfg()!.skipOnBattery}
                onChange={(e) =>
                  void patch({ skipOnBattery: e.currentTarget.checked })
                }
              />
            </span>
          </div>
        </div>

        {/* ---------------------------------------------------------------
            What it may do
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">What it may do</div>

          <div class="auto-modes">
            <button
              type="button"
              class="theme-opt"
              data-sel={!cfg()!.autoClean ? "true" : "false"}
              aria-pressed={!cfg()!.autoClean}
              onClick={() => void patch({ autoClean: false })}
            >
              <div>
                <div class="n">Scan and tell me</div>
                <div class="d">
                  Finds space and notifies you. Nothing is deleted until you say
                  so.
                </div>
              </div>
            </button>
            <button
              type="button"
              class="theme-opt"
              data-sel={cfg()!.autoClean ? "true" : "false"}
              aria-pressed={cfg()!.autoClean}
              onClick={() => void patch({ autoClean: true })}
            >
              <div>
                <div class="n">Autopilot</div>
                <div class="d">
                  Also removes what you pre-approve below, and tells you what it
                  freed.
                </div>
              </div>
            </button>
          </div>

          <Show when={cfg()!.autoClean}>
            <div class="auto-note warn">
              Autopilot never touches <strong>Caution</strong> items — your
              downloaded models, editor history and anything that doesn't
              regenerate on its own. That limit is enforced in the backend, not
              here.
            </div>

            <div class="set-row">
              <div>
                <div class="rl">Safety tiers</div>
                <div class="rd">
                  Safe regenerates on its own. Review usually does, but costs you
                  a rebuild.
                </div>
              </div>
              <div class="chips">
                <For each={["safe", "review"] as SafetyTier[]}>
                  {(tier) => (
                    <button
                      type="button"
                      class="chip"
                      data-on={cfg()!.autoCleanTiers.includes(tier) ? "true" : "false"}
                      aria-pressed={cfg()!.autoCleanTiers.includes(tier)}
                      onClick={() =>
                        toggleTier(tier, !cfg()!.autoCleanTiers.includes(tier))
                      }
                    >
                      <span
                        class="dot"
                        style={{
                          background:
                            tier === "safe" ? "var(--mint-strong)" : "var(--amber)",
                        }}
                        aria-hidden="true"
                      />
                      {TIER_LABELS[tier]}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <For each={AUTO_CATEGORIES}>
              {(cat) => (
                <div class="set-row">
                  <div>
                    <div class="rl">{cat.label}</div>
                    <div class="rd">{cat.detail}</div>
                  </div>
                  <span
                    class="switch"
                    data-on={
                      cfg()!.autoCleanCategories.includes(cat.id) ? "true" : "false"
                    }
                    role="switch"
                    aria-checked={cfg()!.autoCleanCategories.includes(cat.id)}
                    aria-label={`Autopilot may clean ${cat.label}`}
                  >
                    <input
                      type="checkbox"
                      class="sr-check"
                      checked={cfg()!.autoCleanCategories.includes(cat.id)}
                      onChange={(e) =>
                        toggleCategory(cat.id, e.currentTarget.checked)
                      }
                    />
                  </span>
                </div>
              )}
            </For>

            <div class="set-row">
              <div>
                <div class="rl">Per-run limit</div>
                <div class="rd">
                  A single automatic cleanup stops after this much. Largest items
                  go first.
                </div>
              </div>
              <div class="segmented" role="group" aria-label="Per-run limit">
                <For each={CAP_CHOICES}>
                  {(gib) => (
                    <button
                      type="button"
                      data-on={
                        cfg()!.maxAutoCleanBytes === gib * GIB ? "true" : "false"
                      }
                      onClick={() =>
                        void patch({ maxAutoCleanBytes: gib * GIB })
                      }
                    >
                      {gib} GB
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="set-row">
              <div>
                <div class="rl">Destination</div>
                <div class="rd">
                  Recycle Bin keeps an undo. Permanent frees the space
                  immediately.
                </div>
              </div>
              <div class="segmented" role="group" aria-label="Destination">
                <button
                  type="button"
                  data-on={cfg()!.toRecycleBin ? "true" : "false"}
                  onClick={() => void patch({ toRecycleBin: true })}
                >
                  Recycle
                </button>
                <button
                  type="button"
                  data-on={!cfg()!.toRecycleBin ? "true" : "false"}
                  onClick={() => void patch({ toRecycleBin: false })}
                >
                  Permanent
                </button>
              </div>
            </div>

            <Show when={eligible().length > 0}>
              <div class="auto-rules">
                <div class="auto-rules-head">
                  Rules autopilot will use ({eligible().filter((r) => ruleEnabled(r.id)).length}
                  {" of "}
                  {eligible().length})
                </div>
                <For each={eligible()}>
                  {(rule) => (
                    <label class="auto-rule">
                      <input
                        type="checkbox"
                        checked={ruleEnabled(rule.id)}
                        onChange={(e) =>
                          toggleRule(rule.id, e.currentTarget.checked)
                        }
                      />
                      <span class="auto-rule-body">
                        <span class="auto-rule-label">{rule.label}</span>
                        <span class="auto-rule-note">{rule.note}</span>
                      </span>
                    </label>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </div>

        {/* ---------------------------------------------------------------
            Staying resident
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">Staying available</div>
          <div class="auto-note">
            Windows has no way to wake an app when a disk fills up, so Safai
            stays in the notification area to watch. It sits at background I/O
            priority while it works, which keeps scans out of the way of
            whatever you're doing.
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Start with Windows</div>
              <div class="rd">
                Launches hidden to the tray at sign-in.
                <Show when={cfg()!.autostart !== status()!.autostartRegistered}>
                  {" "}
                  <span class="auto-warn">Could not update the startup entry.</span>
                </Show>
              </div>
            </div>
            <span
              class="switch"
              data-on={cfg()!.autostart ? "true" : "false"}
              role="switch"
              aria-checked={cfg()!.autostart}
              aria-label="Start with Windows"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={cfg()!.autostart}
                onChange={(e) =>
                  void patch({ autostart: e.currentTarget.checked })
                }
              />
            </span>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Close to tray</div>
              <div class="rd">
                Closing the window keeps automation alive. Quit from the tray
                menu to exit fully.
              </div>
            </div>
            <span
              class="switch"
              data-on={cfg()!.minimizeToTray ? "true" : "false"}
              role="switch"
              aria-checked={cfg()!.minimizeToTray}
              aria-label="Close to tray"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={cfg()!.minimizeToTray}
                onChange={(e) =>
                  void patch({ minimizeToTray: e.currentTarget.checked })
                }
              />
            </span>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Notify me</div>
              <div class="rd">A short summary after each automatic run.</div>
            </div>
            <span
              class="switch"
              data-on={cfg()!.notify ? "true" : "false"}
              role="switch"
              aria-checked={cfg()!.notify}
              aria-label="Notify after automatic runs"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={cfg()!.notify}
                onChange={(e) => void patch({ notify: e.currentTarget.checked })}
              />
            </span>
          </div>
        </div>

        {/* ---------------------------------------------------------------
            History
            --------------------------------------------------------------- */}
        <div class="card set-card">
          <div class="h">Recent runs</div>
          <Show
            when={status()!.history.length > 0}
            fallback={
              <div class="auto-note">
                Nothing yet. Runs show up here with what they found and removed.
              </div>
            }
          >
            <For each={status()!.history}>
              {(run) => (
                <div class="auto-run">
                  <div class="auto-run-main">
                    <span class="auto-run-when" title={formatClock(run.at)}>
                      {formatRelative(run.at)}
                    </span>
                    <span class="auto-run-trigger">
                      {TRIGGER_LABELS[run.trigger]}
                    </span>
                  </div>
                  <div class="auto-run-detail">
                    <Show
                      when={run.cleanedItems > 0}
                      fallback={
                        <>
                          Found {formatBytes(run.reclaimableBytes)} across{" "}
                          {run.scannedItems} item
                          {run.scannedItems === 1 ? "" : "s"}
                        </>
                      }
                    >
                      Freed {formatBytes(run.reclaimedBytes)} · {run.cleanedItems}{" "}
                      removed
                      <Show when={run.skippedItems > 0}>
                        {" "}
                        · {run.skippedItems} skipped
                      </Show>
                    </Show>
                    <Show when={run.error}>
                      <span class="auto-warn"> · {run.error}</span>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default Automation;
