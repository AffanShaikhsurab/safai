import { type Component, createMemo } from "solid-js";
import { appStore } from "../state/store";
import { splitBytes } from "../lib/format";
import Dial from "../components/Dial";

/**
 * Clean · cleaning — the dial fills as items are removed (deleted+skipped over
 * deleteTotal); the center shows the reclaimed total climbing. Driven entirely
 * by the streamed delete events in the store.
 */
const Cleaning: Component = () => {
  const progress = () => appStore.state.progress;
  const done = () => progress().deleted + progress().skipped;

  const pct = createMemo(() => {
    const total = progress().deleteTotal;
    if (total <= 0) return 0;
    return Math.min(100, Math.round((done() / total) * 100));
  });

  const reclaimed = createMemo(() => splitBytes(progress().reclaimedBytes));

  const currentName = () => {
    const p = progress().currentPath;
    if (!p) return "Removing…";
    const parts = p.split(/[\\/]/).filter(Boolean);
    return parts.length ? parts[parts.length - 1] : p;
  };

  return (
    <div class="stage animate-rise">
      <div class="toprow">
        <div class="card topcard">
          <div>
            <div class="k">Status</div>
            <div class="s">Removing…</div>
          </div>
          <div class="val">
            {done()}/{progress().deleteTotal}
          </div>
        </div>
        <div class="card topcard">
          <div>
            <div class="k">Skipped</div>
            <div class="s">by guardrails</div>
          </div>
          <div class="val">{progress().skipped}</div>
        </div>
      </div>

      <div class="dialrow">
        <div class="dial-spacer" aria-hidden="true" />

        <Dial
          pct={pct()}
          big={reclaimed().value}
          unit={reclaimed().unit}
          cap="Reclaimed"
          sub={currentName()}
        />

        <div class="dial-spacer" aria-hidden="true" />
      </div>

      <div class="card infocard">
        <div class="l">Cleaning</div>
        <div class="r" title={progress().currentPath}>
          {done()}/{progress().deleteTotal} · {progress().currentPath || "…"}
        </div>
      </div>
    </div>
  );
};

export default Cleaning;
