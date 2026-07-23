// Native OS notifications (via tauri-plugin-notification). Used to tell the
// user when a scan or cleanup finishes so they don't have to watch the window.
// All calls are best-effort: if permission is denied or the plugin is
// unavailable, they fail silently.

import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let resolved = false;
let granted = false;

/**
 * Ensure we have notification permission, requesting it once if needed.
 * Cached after the first resolution. Safe to call early (e.g. app mount).
 */
export async function ensureNotifyPermission(): Promise<boolean> {
  if (resolved) return granted;
  resolved = true;
  try {
    granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
  } catch {
    granted = false;
  }
  return granted;
}

/**
 * Send a notification. When `onlyWhenUnfocused` is true (the default for
 * completion pings), it is skipped if the app window currently has focus —
 * there's no point notifying the user about something they're already watching.
 */
export async function notify(
  title: string,
  body: string,
  opts: { onlyWhenUnfocused?: boolean } = { onlyWhenUnfocused: true },
): Promise<void> {
  try {
    if (
      opts.onlyWhenUnfocused &&
      typeof document !== "undefined" &&
      document.hasFocus()
    ) {
      return;
    }
    if (!(await ensureNotifyPermission())) return;
    sendNotification({ title, body });
  } catch {
    // Non-fatal — notifications are a convenience, not a requirement.
  }
}
