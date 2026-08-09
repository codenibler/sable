/**
 * Sable runs as a Tauri desktop window and as a read-only PWA served over a tunnel by that
 * same desktop process. One bundle serves both, so everything Tauri-only lives here behind
 * dynamic imports: a browser must never evaluate `@tauri-apps/api`, which throws when the
 * IPC globals are missing.
 */
export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The phone is a monitor: it reads the desktop's data and never writes back. */
export const isReadOnly = !isDesktop;

const TOKEN_KEY = "sable-api-token";

/**
 * iOS private browsing and blocked site data make `localStorage` throw. Without a fallback the
 * gate would accept a valid token, lose it, and reappear on the very next request -- forever.
 * Holding it here keeps the session usable until the tab closes.
 */
let memoryToken: string | null = null;

export function readToken(): string | null {
  if (isDesktop) return null;
  try {
    return localStorage.getItem(TOKEN_KEY) ?? memoryToken;
  } catch {
    return memoryToken;
  }
}

export function writeToken(token: string) {
  memoryToken = token.trim();
  try {
    localStorage.setItem(TOKEN_KEY, memoryToken);
  } catch {
    // Private browsing blocks storage; the token still works for this session.
  }
}

export function clearToken() {
  memoryToken = null;
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // Nothing to clear if storage was never writable.
  }
}

/**
 * The desktop backfills Trading 212 history in the background and emits when a page lands.
 * On the web there is no event channel, and the poll interval covers it instead.
 */
export async function onHistorySynced(handler: () => void): Promise<() => void> {
  if (!isDesktop) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen("history-synced", handler);
}

type WindowAction = "minimize" | "toggleMaximize" | "close";

export async function controlWindow(action: WindowAction): Promise<void> {
  if (!isDesktop) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const current = getCurrentWindow();
  if (action === "minimize") return current.minimize();
  if (action === "toggleMaximize") return current.toggleMaximize();
  return current.close();
}
