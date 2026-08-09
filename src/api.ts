import type { AddWalletInput, CryptoPortfolio, Dashboard, NetWorthEntry, SaveNetWorthInput } from "./types";
import { isDesktop, readToken } from "./platform";

/** Raised when the stored token is missing or rejected, so the UI can ask for a new one. */
export class UnauthorizedError extends Error {
  constructor() {
    super("This token was not accepted.");
    this.name = "UnauthorizedError";
  }
}

const READ_ONLY_MESSAGE = "Sable on mobile is read-only. Make this change on the desktop app.";

async function request<T>(path: string): Promise<T> {
  const token = readToken();
  if (!token) throw new UnauthorizedError();

  let response: Response;
  try {
    response = await fetch(path, {
      headers: { Authorization: `Bearer ${token}` },
      cache: "no-store",
    });
  } catch {
    // Offline, or the desktop is asleep and the tunnel has nothing to reach.
    throw new Error("Sable is unreachable. The desktop app may be asleep.");
  }

  if (response.status === 401) throw new UnauthorizedError();
  if (!response.ok) {
    throw new Error((await response.text()) || `Request failed (${response.status})`);
  }
  return response.json() as Promise<T>;
}

function readOnly<T>(): Promise<T> {
  return Promise.reject(new Error(READ_ONLY_MESSAGE));
}

type SableApi = {
  dashboard(force?: boolean): Promise<Dashboard>;
  netWorthEntries(): Promise<NetWorthEntry[]>;
  setOpessociusMonthlyReturn(amount: number): Promise<void>;
  portfolios(): Promise<CryptoPortfolio[]>;
  addWallet(input: AddWalletInput): Promise<number>;
  removeWallet(id: number): Promise<void>;
  saveNetWorthEntry(input: SaveNetWorthInput): Promise<void>;
  removeNetWorthEntry(date: string): Promise<void>;
};

/**
 * Both transports satisfy the same contract, so components never branch on platform. The
 * write methods reject on the web rather than being absent: a missed UI guard then surfaces
 * a readable message instead of crashing on an undefined call.
 */
const webApi: SableApi = {
  dashboard: () => request<Dashboard>("/api/dashboard"),
  netWorthEntries: () => request<NetWorthEntry[]>("/api/net-worth"),
  setOpessociusMonthlyReturn: readOnly,
  portfolios: readOnly,
  addWallet: readOnly,
  removeWallet: readOnly,
  saveNetWorthEntry: readOnly,
  removeNetWorthEntry: readOnly,
};

async function loadDesktopApi(): Promise<SableApi> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    // `force` bypasses the shared dashboard cache so the topbar refresh button still fetches
    // for real. The phone never sets it, protecting the shared rate-limit budget.
    dashboard: (force = false) => invoke<Dashboard>("get_dashboard", { force }),
    netWorthEntries: () => invoke<NetWorthEntry[]>("list_net_worth_entries"),
    setOpessociusMonthlyReturn: (amount) =>
      invoke<void>("set_opessocius_monthly_return", { amount }),
    portfolios: () => invoke<CryptoPortfolio[]>("list_crypto_portfolios"),
    addWallet: (input) => invoke<number>("add_wallet", { input }),
    removeWallet: (id) => invoke<void>("remove_wallet", { id }),
    saveNetWorthEntry: (input) => invoke<void>("save_net_worth_entry", { input }),
    removeNetWorthEntry: (date) => invoke<void>("remove_net_worth_entry", { date }),
  };
}

let desktopApi: Promise<SableApi> | null = null;

function transport(): Promise<SableApi> {
  if (!isDesktop) return Promise.resolve(webApi);
  desktopApi ??= loadDesktopApi();
  return desktopApi;
}

export const api: SableApi = {
  dashboard: (force) => transport().then((impl) => impl.dashboard(force)),
  netWorthEntries: () => transport().then((impl) => impl.netWorthEntries()),
  setOpessociusMonthlyReturn: (amount) =>
    transport().then((impl) => impl.setOpessociusMonthlyReturn(amount)),
  portfolios: () => transport().then((impl) => impl.portfolios()),
  addWallet: (input) => transport().then((impl) => impl.addWallet(input)),
  removeWallet: (id) => transport().then((impl) => impl.removeWallet(id)),
  saveNetWorthEntry: (input) => transport().then((impl) => impl.saveNetWorthEntry(input)),
  removeNetWorthEntry: (date) => transport().then((impl) => impl.removeNetWorthEntry(date)),
};

const CACHE_PREFIX = "sable-cache:";

type Cached<T> = { at: string; payload: T };

/**
 * The desktop is the only source of data, so when it sleeps the phone has nothing to fetch.
 * Keeping the last good payload turns that from an error screen into a dated snapshot.
 *
 * Deliberately localStorage rather than the service worker's Cache API: these responses carry
 * the full financial position, and one storage location means one place to clear.
 */
export function readCache<T>(key: string): Cached<T> | null {
  if (isDesktop) return null;
  try {
    const raw = localStorage.getItem(`${CACHE_PREFIX}${key}`);
    return raw ? (JSON.parse(raw) as Cached<T>) : null;
  } catch {
    return null;
  }
}

export function writeCache<T>(key: string, payload: T) {
  if (isDesktop) return;
  try {
    localStorage.setItem(
      `${CACHE_PREFIX}${key}`,
      JSON.stringify({ at: new Date().toISOString(), payload }),
    );
  } catch {
    // A full or unavailable store only costs the offline view.
  }
}

export function clearCache() {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith(CACHE_PREFIX)) localStorage.removeItem(key);
    }
  } catch {
    // Nothing cached if storage was never writable.
  }
}
