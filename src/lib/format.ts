// Display helpers. Pure functions, no reactivity — safe to call inside JSX.

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;
const TB = GB * 1024;

/**
 * Human-readable byte size, e.g. 0 B, 512 B, 3.4 KB, 12.0 MB, 1.25 GB.
 * Uses binary units (1 KB = 1024 B) to match on-disk sizes.
 */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  if (n < KB) return `${Math.round(n)} B`;
  if (n < MB) return `${(n / KB).toFixed(1)} KB`;
  if (n < GB) return `${(n / MB).toFixed(1)} MB`;
  if (n < TB) return `${(n / GB).toFixed(2)} GB`;
  return `${(n / TB).toFixed(2)} TB`;
}

/**
 * Split a byte size into its numeric value and unit, e.g. 1.25 GB →
 * { value: "1.25", unit: "GB" }. Handy for the dial, which renders the number
 * large and the unit small. Uses the same formatting as `formatBytes`.
 */
export function splitBytes(n: number): { value: string; unit: string } {
  if (!Number.isFinite(n) || n <= 0) return { value: "0", unit: "B" };
  // Compact for the big dial number: 1 decimal below 100, whole numbers at or
  // above 100, so the value stays short (max ~4 chars) and fits inside the ring.
  const fit = (v: number, unit: string) => ({
    value: v >= 100 ? Math.round(v).toString() : v.toFixed(1),
    unit,
  });
  if (n < KB) return { value: Math.round(n).toString(), unit: "B" };
  if (n < MB) return fit(n / KB, "KB");
  if (n < GB) return fit(n / MB, "MB");
  if (n < TB) return fit(n / GB, "GB");
  return fit(n / TB, "TB");
}

const MINUTE = 60;
const HOUR = MINUTE * 60;
const DAY = HOUR * 24;
const WEEK = DAY * 7;
const MONTH = DAY * 30;
const YEAR = DAY * 365;

/**
 * Relative time from a unix timestamp (seconds) to now, e.g. "3 months ago".
 * Returns "just now" for very recent times and "in the future" defensively.
 */
export function relativeTime(unixSecs: number): string {
  if (!Number.isFinite(unixSecs) || unixSecs <= 0) return "unknown";

  const nowSecs = Date.now() / 1000;
  const diff = nowSecs - unixSecs;

  if (diff < 0) return "in the future";
  if (diff < MINUTE) return "just now";

  const units: Array<[number, string]> = [
    [YEAR, "year"],
    [MONTH, "month"],
    [WEEK, "week"],
    [DAY, "day"],
    [HOUR, "hour"],
    [MINUTE, "minute"],
  ];

  for (const [secs, label] of units) {
    if (diff >= secs) {
      const value = Math.floor(diff / secs);
      return `${value} ${label}${value === 1 ? "" : "s"} ago`;
    }
  }

  return "just now";
}
