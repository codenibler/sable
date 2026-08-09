import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownRight,
  ArrowUpRight,
  BriefcaseBusiness,
  Building2,
  ChartNoAxesCombined,
  CircleAlert,
  CloudOff,
  HardDrive,
  KeyRound,
  LayoutDashboard,
  LogOut,
  Minus,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  RefreshCw,
  Square,
  WalletCards,
  X,
} from "lucide-react";
import { UnauthorizedError, api, clearCache, readCache, writeCache } from "./api";
import {
  clearToken,
  controlWindow,
  isDesktop,
  isReadOnly,
  onHistorySynced,
  readToken,
  writeToken,
} from "./platform";
import { PerformanceChart } from "./components/PerformanceChart";
import { NetWorthView } from "./components/NetWorthView";
import type { AddWalletInput, CryptoPortfolio, Dashboard, MonitoredPortfolio, MonthlyWinnings, NetWorthEntry, PeriodReturn } from "./types";

type View = "net-worth" | "overview" | "wallets" | `portfolio:${string}`;

const FONT_SCALE_KEY = "sable-font-scale";
const DEFAULT_FONT_SCALE = 100;
const MIN_FONT_SCALE = 75;
const MAX_FONT_SCALE = 130;
const FONT_SCALE_STEP = 5;

function initialFontScale() {
  try {
    const stored = Number.parseInt(localStorage.getItem(FONT_SCALE_KEY) ?? "", 10);
    return Number.isFinite(stored)
      ? Math.min(MAX_FONT_SCALE, Math.max(MIN_FONT_SCALE, stored))
      : DEFAULT_FONT_SCALE;
  } catch {
    return DEFAULT_FONT_SCALE;
  }
}

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
  const [view, setView] = useState<View>("net-worth");
  // Hydrating from the last good payload means the phone opens with numbers rather than a
  // spinner when the desktop is asleep.
  const [dashboard, setDashboard] = useState<Dashboard | null>(
    () => readCache<Dashboard>("dashboard")?.payload ?? null,
  );
  const [netWorthEntries, setNetWorthEntries] = useState<NetWorthEntry[]>(
    () => readCache<NetWorthEntry[]>("net-worth")?.payload ?? [],
  );
  const [loading, setLoading] = useState(true);
  const [netWorthLoading, setNetWorthLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [netWorthError, setNetWorthError] = useState<string | null>(null);
  // Tracked per dataset: the dashboard and net worth load concurrently, so one settling last must
  // not erase the other's verdict on how fresh the numbers on screen are.
  const [dashboardStaleSince, setDashboardStaleSince] = useState<string | null>(null);
  const [netWorthStaleSince, setNetWorthStaleSince] = useState<string | null>(null);
  const [needsToken, setNeedsToken] = useState(() => isReadOnly && !readToken());
  const [walletModal, setWalletModal] = useState(false);
  const [winningsModal, setWinningsModal] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [fontScale, setFontScale] = useState(initialFontScale);
  const refreshing = useRef<Promise<void> | null>(null);

  const runRefresh = useCallback(async (force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.dashboard(force);
      setDashboard(next);
      writeCache("dashboard", next);
      setDashboardStaleSince(null);
    } catch (caught) {
      if (caught instanceof UnauthorizedError) {
        setNeedsToken(true);
      } else {
        // Prefer dated numbers over an error screen: the desktop being asleep is the normal
        // offline case, not a failure the user needs to act on.
        const cached = readCache<Dashboard>("dashboard");
        if (cached) {
          setDashboard(cached.payload);
          setDashboardStaleSince(cached.at);
        } else {
          setError(messageOf(caught));
        }
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const refresh = useCallback((force = false): Promise<void> => {
    // Unforced callers (the poll, the initial load) still collapse into whatever is already in
    // flight — one fan-out at a time. A forced caller follows a mutation, so it cannot be dropped:
    // the in-flight call was served the pre-change snapshot, so we queue behind it and rebuild.
    const inFlight = refreshing.current;
    if (inFlight && !force) return inFlight;
    const next = (inFlight ?? Promise.resolve())
      .catch(() => undefined)
      .then(() => runRefresh(force));
    refreshing.current = next;
    void next.catch(() => undefined).finally(() => {
      if (refreshing.current === next) refreshing.current = null;
    });
    return next;
  }, [runRefresh]);

  const refreshNetWorth = useCallback(async () => {
    setNetWorthLoading(true);
    setNetWorthError(null);
    try {
      const entries = await api.netWorthEntries();
      setNetWorthEntries(entries);
      writeCache("net-worth", entries);
      setNetWorthStaleSince(null);
    } catch (caught) {
      if (caught instanceof UnauthorizedError) {
        setNeedsToken(true);
      } else {
        const cached = readCache<NetWorthEntry[]>("net-worth");
        if (cached) {
          setNetWorthEntries(cached.payload);
          setNetWorthStaleSince(cached.at);
        } else {
          setNetWorthError(messageOf(caught));
        }
      }
    } finally {
      setNetWorthLoading(false);
    }
  }, []);

  const refreshNetWorthAndDashboard = useCallback(async () => {
    await Promise.all([refreshNetWorth(), refresh(true)]);
  }, [refresh, refreshNetWorth]);

  const forgetToken = useCallback(() => {
    clearToken();
    // The cached payload is this token's data; it should not outlive the credential.
    clearCache();
    setDashboard(null);
    setNetWorthEntries([]);
    setDashboardStaleSince(null);
    setNetWorthStaleSince(null);
    setNeedsToken(true);
  }, []);

  useEffect(() => {
    if (needsToken) return;
    void refresh();
    void refreshNetWorth();
  }, [needsToken, refresh, refreshNetWorth]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    // Each backfill page changes the numbers the dashboard is built from, so an unforced rebuild
    // would just replay the cached snapshot and the event would never reach the screen. The poll
    // stays unforced because it has no such news to carry — it only exists to ride the cache.
    void onHistorySynced(() => void refresh(true)).then((stopListening) => {
      unlisten = stopListening;
    });
    return () => unlisten?.();
  }, [refresh]);

  useEffect(() => {
    if (!dashboard?.refreshIntervalMinutes) return;
    const interval = window.setInterval(
      () => void refresh(),
      Math.max(dashboard.refreshIntervalMinutes, 1) * 60_000,
    );
    return () => window.clearInterval(interval);
  }, [dashboard?.refreshIntervalMinutes, refresh]);

  useEffect(() => {
    document.documentElement.style.fontSize = `${fontScale}%`;
    try {
      localStorage.setItem(FONT_SCALE_KEY, String(fontScale));
    } catch {
      // The current session still uses the selected size if storage is unavailable.
    }
  }, [fontScale]);

  useEffect(() => {
    const handleFontScale = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey) return;
      const increase = event.key === "+" || event.key === "=" || event.code === "NumpadAdd";
      const decrease = event.key === "-" || event.code === "NumpadSubtract";
      const reset = event.key === "0" || event.code === "Numpad0";
      if (!increase && !decrease && !reset) return;

      event.preventDefault();
      setFontScale((current) => {
        if (reset) return DEFAULT_FONT_SCALE;
        const change = increase ? FONT_SCALE_STEP : -FONT_SCALE_STEP;
        return Math.min(MAX_FONT_SCALE, Math.max(MIN_FONT_SCALE, current + change));
      });
    };
    window.addEventListener("keydown", handleFontScale);
    return () => window.removeEventListener("keydown", handleFontScale);
  }, []);

  // Either dataset going stale is worth the banner, and quoting the older of the two keeps it
  // honest about the oldest numbers on screen. Both stamps are ISO, so they sort as strings.
  const staleSince = useMemo(() => {
    if (!dashboardStaleSince) return netWorthStaleSince;
    if (!netWorthStaleSince) return dashboardStaleSince;
    return netWorthStaleSince < dashboardStaleSince ? netWorthStaleSince : dashboardStaleSince;
  }, [dashboardStaleSince, netWorthStaleSince]);

  const currency = dashboard?.currency ?? "EUR";
  const money = useMemo(
    () => new Intl.NumberFormat(undefined, { style: "currency", currency, maximumFractionDigits: 2 }),
    [currency],
  );
  const wholeMoney = useMemo(
    () => new Intl.NumberFormat(undefined, { style: "currency", currency, maximumFractionDigits: 0 }),
    [currency],
  );
  const formatMoney = (value: number) => money.format(value);
  const formatWholeMoney = (value: number) => wholeMoney.format(value);
  const selectedPortfolio = view.startsWith("portfolio:")
    ? dashboard?.monitoredPortfolios.find((portfolio) => `portfolio:${portfolio.id}` === view)
    : undefined;
  const selectedCryptoPortfolio = selectedPortfolio?.kind === "crypto"
    ? dashboard?.portfolios.find((portfolio) => `portfolio-${portfolio.id}` === selectedPortfolio.id)
    : undefined;
  const pageLabel = view === "net-worth" ? "Personal balance sheet" : selectedPortfolio?.kind === "crypto" ? "Hardware wallet" : selectedPortfolio ? "Monitored portfolio" : view === "overview" ? "Live monitor" : "On-chain portfolios";
  const pageTitle = view === "net-worth" ? "Net Worth" : selectedPortfolio?.name ?? (view === "overview" ? "Overview" : "Crypto wallets");

  if (needsToken) {
    return <TokenGate onSaved={() => setNeedsToken(false)} />;
  }

  return (
    <div className={`app-shell ${sidebarCollapsed ? "sidebar-collapsed" : ""}`}>
      <aside className="sidebar">
        <button className="sidebar-toggle" onClick={() => setSidebarCollapsed((collapsed) => !collapsed)} aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}>
          {sidebarCollapsed ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
        </button>
        <nav>
          <button className={view === "net-worth" ? "active" : ""} onClick={() => setView("net-worth")}>
            <ChartNoAxesCombined size={18} /><span className="nav-copy">Net Worth</span>
          </button>
          <button className={view === "overview" ? "active" : ""} onClick={() => setView("overview")}>
            <LayoutDashboard size={18} /><span className="nav-copy">Overview</span>
          </button>
          <p className="section-label sidebar-section-label">Portfolios</p>
          {dashboard?.monitoredPortfolios.map((portfolio) => (
            <button className={view === `portfolio:${portfolio.id}` ? "active portfolio-nav-item" : "portfolio-nav-item"} key={portfolio.id} onClick={() => setView(`portfolio:${portfolio.id}`)} title={portfolio.name}>
              {portfolio.kind === "brokerage" ? <Building2 size={18} /> : portfolio.kind === "manual" ? <BriefcaseBusiness size={18} /> : <HardDrive size={18} />}
              <span className="nav-copy"><strong>{portfolio.name}</strong><small>{formatMoney(portfolio.value)}</small></span>
            </button>
          ))}
          {isDesktop && (
            <>
              <p className="section-label sidebar-section-label">Manage</p>
              <button className={view === "wallets" ? "active" : ""} onClick={() => setView("wallets")}>
                <WalletCards size={18} /><span className="nav-copy">Crypto wallets</span>
              </button>
            </>
          )}
        </nav>
        <p className="local-note"><span className="status-dot ok" /><span className="nav-copy">{isDesktop ? "Local-only storage" : "Read-only monitor"}</span></p>
      </aside>

      <main className="content">
        <header className="topbar" data-tauri-drag-region>
          <div className="topbar-title" data-tauri-drag-region>
            <p className="section-label" data-tauri-drag-region>{pageLabel}</p>
            <h1 data-tauri-drag-region>{pageTitle}</h1>
            {view === "net-worth" ? netWorthEntries.length > 0 && <p className="updated" data-tauri-drag-region>Latest snapshot {new Date(`${netWorthEntries.at(-1)!.date}T00:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}</p> : dashboard && <p className="updated" data-tauri-drag-region>Updated {new Date(dashboard.updatedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</p>}
          </div>
          <div className="topbar-actions">
            <button className="icon-button" onClick={() => void (view === "net-worth" ? refreshNetWorthAndDashboard() : refresh(true))} disabled={view === "net-worth" ? netWorthLoading || loading : loading} aria-label="Refresh portfolio" title="Refresh now">
              <RefreshCw size={18} className={(view === "net-worth" ? netWorthLoading || loading : loading) ? "spin" : ""} />
            </button>
            {isReadOnly && (
              <button className="icon-button" onClick={forgetToken} aria-label="Forget token" title="Forget token">
                <LogOut size={18} />
              </button>
            )}
            {isDesktop && (view === "wallets" || selectedCryptoPortfolio) && <button className="primary-button" onClick={() => setWalletModal(true)} disabled={!dashboard?.portfolios.length}><Plus size={17} /> Add wallet</button>}
            {isDesktop && <WindowControls />}
          </div>
        </header>

        {staleSince && <StaleBanner since={staleSince} />}

        {view !== "net-worth" && error && (
          <div className="error-banner"><CircleAlert size={18} /><div><strong>Sable could not start</strong><p>{error}</p></div></div>
        )}
        {view === "net-worth" && netWorthError && <div className="error-banner"><CircleAlert size={18} /><div><strong>Could not load net worth</strong><p>{netWorthError}</p></div></div>}

        {view === "net-worth" ? netWorthLoading && !netWorthEntries.length ? <Loading /> : <NetWorthView entries={netWorthEntries} formatMoney={formatMoney} onChanged={refreshNetWorthAndDashboard} /> : !dashboard && loading ? <Loading /> : dashboard && view === "overview" ? (
          <Overview dashboard={dashboard} formatMoney={formatMoney} formatWholeMoney={formatWholeMoney} onAddWinnings={isDesktop ? () => setWinningsModal(true) : undefined} />
        ) : dashboard && selectedPortfolio && selectedCryptoPortfolio ? (
          <CryptoPortfolioDetailView portfolio={selectedCryptoPortfolio} monitored={selectedPortfolio} formatMoney={formatMoney} formatWholeMoney={formatWholeMoney} />
        ) : dashboard && selectedPortfolio ? (
          <PortfolioDetailView portfolio={selectedPortfolio} formatMoney={formatMoney} formatWholeMoney={formatWholeMoney} onEditReturn={isDesktop && selectedPortfolio.id === "opessocius" && dashboard.opessociusMonthlyReturn ? () => setWinningsModal(true) : undefined} />
        ) : dashboard && isDesktop ? (
          <Wallets
            portfolios={dashboard.portfolios}
            formatMoney={formatMoney}
            onAddWallet={() => setWalletModal(true)}
            onChanged={() => refresh(true)}
          />
        ) : null}
      </main>

      {isDesktop && walletModal && dashboard && (
        <WalletModal portfolios={dashboard.portfolios} onClose={() => setWalletModal(false)} onSaved={async () => { setWalletModal(false); await refresh(true); }} />
      )}
      {isDesktop && winningsModal && dashboard?.opessociusMonthlyReturn && (
        <WinningsModal winnings={dashboard.opessociusMonthlyReturn} currency={dashboard.currency} onClose={() => setWinningsModal(false)} onSaved={async () => { setWinningsModal(false); await refresh(true); }} />
      )}
    </div>
  );
}

/**
 * The phone's only source of data is the desktop, so an unreachable desktop is an expected
 * state rather than a fault. Showing when the numbers are from keeps that honest.
 */
function StaleBanner({ since }: { since: string }) {
  const when = new Date(since);
  const sameDay = when.toDateString() === new Date().toDateString();
  return (
    <div className="stale-banner">
      <CloudOff size={17} />
      <p>
        Showing the last snapshot Sable could reach, from{" "}
        <strong>
          {sameDay
            ? when.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
            : when.toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}
        </strong>
        . Open Sable on your desktop to refresh.
      </p>
    </div>
  );
}

function TokenGate({ onSaved }: { onSaved: () => void }) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed = token.trim();
    // Matches the minimum the Rust side refuses to start with, so a truncated paste is
    // caught here rather than looking like a server rejection.
    if (trimmed.length < 32) {
      setError("That token looks too short — check the whole value was pasted.");
      return;
    }
    // A stale cache belongs to whoever held the previous token.
    clearCache();
    writeToken(trimmed);
    onSaved();
  };

  return (
    <div className="token-gate">
      <form className="token-card" onSubmit={submit}>
        <span className="token-mark"><KeyRound size={22} /></span>
        <h1>Connect to Sable</h1>
        <p>Paste the <code>MOBILE_API_TOKEN</code> from your desktop <code>.env</code>. It is stored on this device only.</p>
        <label>
          Access token
          <input
            autoFocus
            type="password"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            placeholder="64 hex characters"
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            required
          />
        </label>
        {error && <p className="form-error">{error}</p>}
        <button className="primary-button submit">Connect</button>
      </form>
    </div>
  );
}

function WindowControls() {
  const perform = (action: Parameters<typeof controlWindow>[0]) =>
    void controlWindow(action).catch(() => undefined);
  return (
    <div className="window-controls" aria-label="Window controls">
      <button className="window-control" onClick={() => perform("minimize")} aria-label="Minimize window" title="Minimize"><Minus size={14} /></button>
      <button className="window-control" onClick={() => perform("toggleMaximize")} aria-label="Maximize window" title="Maximize"><Square size={11} /></button>
      <button className="window-control window-close" onClick={() => perform("close")} aria-label="Close window" title="Close"><X size={14} /></button>
    </div>
  );
}

function Overview({ dashboard, formatMoney, formatWholeMoney, onAddWinnings }: { dashboard: Dashboard; formatMoney: (value: number) => string; formatWholeMoney: (value: number) => string; onAddWinnings?: () => void }) {
  const positive = dashboard.totalReturn >= 0;
  return (
    <div className="view-stack">
      {dashboard.notices.length > 0 && (
        <div className="notice-row">
          {dashboard.notices.map((notice) => <span key={notice}><CircleAlert size={14} />{notice}</span>)}
        </div>
      )}
      <section className="hero-metrics">
        <div className="balance-card">
          <p className="metric-label">Total balance</p>
          <p className="balance">{formatMoney(dashboard.totalValue)}</p>
          <span className={positive ? "change positive" : "change negative"}>
            {positive ? <ArrowUpRight size={16} /> : <ArrowDownRight size={16} />}
            {dashboard.returnPercent.toFixed(2)}% · {formatMoney(dashboard.totalReturn)}
          </span>
          <small className="return-method">
            {dashboard.historyBackfillComplete ? "Cash-flow adjusted" : "Current holdings basis"}
          </small>
        </div>
        <Metric
          label={dashboard.historyEventCount ? "Net deposits" : "Invested"}
          value={formatMoney(dashboard.historyEventCount ? dashboard.netContributions : dashboard.investedValue)}
          helper={dashboard.historyEventCount ? `${dashboard.historyEventCount} cash events` : undefined}
        />
        <Metric
          label="Total return"
          value={`${dashboard.returnPercent.toFixed(2)}%`}
          helper={formatMoney(dashboard.totalReturn)}
          valueClassName={dashboard.totalReturn >= 0 ? "positive-text" : "negative-text"}
        />
        <Metric label="Cash" value={formatMoney(dashboard.cashValue)} />
      </section>

      <PerformanceChart history={dashboard.history} format={formatMoney} formatAxis={formatWholeMoney} />

      <section className="lower-grid">
        <div className="panel holdings-panel">
          <div className="panel-heading"><div><p className="section-label">Assets</p><h2>Top holdings</h2></div><span className="muted">{dashboard.holdings.length} total</span></div>
          {dashboard.holdings.length ? (
            <div className="table-wrap"><table><thead><tr><th>Asset</th><th>Source</th><th>Allocation</th><th>Value</th><th>Return</th></tr></thead><tbody>
              {dashboard.holdings.slice(0, 8).map((holding) => (
                <tr key={holding.id}><td><div className="asset-cell"><span className="asset-mark">{holding.symbol.slice(0, 2)}</span><div><strong>{holding.symbol}</strong><small>{holding.name}</small></div></div></td><td className="muted">{holding.source}</td><td><div className="allocation"><span>{holding.allocation.toFixed(1)}%</span><i><b style={{ width: `${Math.min(holding.allocation, 100)}%` }} /></i></div></td><td>{formatMoney(holding.value)}</td><td className={holding.returnValue >= 0 ? "positive-text" : "negative-text"}>{holding.returnValue ? formatMoney(holding.returnValue) : "—"}</td></tr>
              ))}
            </tbody></table></div>
          ) : <Empty icon={<BriefcaseBusiness size={22} />} title="No holdings yet" text="Connected positions and wallets will appear here." />}
        </div>
        <aside className="overview-aside">
          <div className="panel sources-panel">
            <div className="panel-heading"><div><p className="section-label">Breakdown</p><h2>Sources</h2></div></div>
            <div className="source-list">
              {dashboard.sources.map((source) => (
                <div className="source-item" key={source.id}><span className={`source-icon ${source.kind}`}><span className={source.connected ? "status-dot ok" : "status-dot"} />{source.kind === "brokerage" ? "T2" : source.kind === "crypto" ? "₿" : "OP"}</span><div className="source-copy"><strong>{source.name}</strong><small>{source.kind === "manual" ? source.message : source.connected ? source.kind : source.message}</small>{source.id === "opessocius" && dashboard.opessociusMonthlyReturn && onAddWinnings && <button className="source-action" onClick={onAddWinnings}>{dashboard.opessociusMonthlyReturn.isOverride ? "Edit return" : "Override return"}</button>}</div><div className="source-value"><strong>{formatMoney(source.value)}</strong><small className={source.returnValue >= 0 ? "positive-text" : "negative-text"}>{source.returnValue ? formatMoney(source.returnValue) : "—"}</small></div></div>
              ))}
            </div>
          </div>
          <PeriodReturns monthly={dashboard.monthlyReturn} yearly={dashboard.yearlyReturn} formatMoney={formatMoney} />
        </aside>
      </section>
    </div>
  );
}

function PortfolioDetailView({ portfolio, formatMoney, formatWholeMoney, onEditReturn }: { portfolio: MonitoredPortfolio; formatMoney: (value: number) => string; formatWholeMoney: (value: number) => string; onEditReturn?: () => void }) {
  const positive = portfolio.totalReturn >= 0;
  const latestPeriod = portfolio.periods.at(-1);
  return <div className="view-stack portfolio-detail-view">
    {!portfolio.connected && portfolio.message && <div className="notice-row"><span><CircleAlert size={14} />{portfolio.message}</span></div>}
    <section className="hero-metrics portfolio-metrics">
      <div className="balance-card">
        <p className="metric-label">Current equity</p>
        <p className="balance">{formatMoney(portfolio.value)}</p>
        <span className={positive ? "change positive" : "change negative"}>{positive ? <ArrowUpRight size={16} /> : <ArrowDownRight size={16} />}{portfolio.returnPercent.toFixed(2)}% · {formatMoney(portfolio.totalReturn)}</span>
        <small className="return-method">{portfolio.connected ? portfolio.id === "trading212" ? "Net Worth history · live account connected" : "Portfolio connected" : "Showing last available history"}</small>
      </div>
      <Metric label="Invested" value={formatMoney(portfolio.investedValue)} />
      <Metric label="Total return" value={`${portfolio.returnPercent.toFixed(2)}%`} helper={formatMoney(portfolio.totalReturn)} valueClassName={positive ? "positive-text" : "negative-text"} />
      <Metric label={latestPeriod ? "Latest month" : portfolio.itemLabel} value={latestPeriod ? `${latestPeriod.returnPercent.toFixed(2)}%` : String(portfolio.itemCount)} helper={latestPeriod ? formatMoney(latestPeriod.returnValue) : `${portfolio.itemCount} ${portfolio.itemLabel}`} valueClassName={latestPeriod ? latestPeriod.returnValue >= 0 ? "positive-text" : "negative-text" : undefined} />
    </section>
    <PerformanceChart history={portfolio.history} format={formatMoney} formatAxis={formatWholeMoney} valueLabel={portfolio.id === "trading212" ? "Account equity" : undefined} />
    {portfolio.periods.length > 0 && <section className="panel history-panel">
      <div className="panel-heading"><div><p className="section-label">Monthly ledger</p><h2>Opessocius history</h2></div>{onEditReturn && <button className="secondary-button" onClick={onEditReturn}>Edit latest return</button>}</div>
      <div className="table-wrap"><table><thead><tr><th>Month</th><th>Rate</th><th>Return</th><th>Deposits</th><th>Withdrawals</th><th>Ending equity</th></tr></thead><tbody>
        {[...portfolio.periods].reverse().map((period) => <tr key={period.month}><td><strong>{period.label}</strong></td><td className={period.returnValue >= 0 ? "positive-text" : "negative-text"}>{period.returnPercent.toFixed(2)}%</td><td>{formatMoney(period.returnValue)}</td><td>{period.deposits ? formatMoney(period.deposits) : "—"}</td><td>{period.withdrawals ? formatMoney(period.withdrawals) : "—"}</td><td>{formatMoney(period.endingValue)}</td></tr>)}
      </tbody></table></div>
    </section>}
  </div>;
}

function CryptoPortfolioDetailView({ portfolio, monitored, formatMoney, formatWholeMoney }: { portfolio: CryptoPortfolio; monitored: MonitoredPortfolio; formatMoney: (value: number) => string; formatWholeMoney: (value: number) => string }) {
  const [selectedAssetId, setSelectedAssetId] = useState("all");
  const selectedAsset = selectedAssetId === "all" ? undefined : portfolio.assets.find((asset) => asset.id === selectedAssetId);
  const visibleWallets = selectedAsset
    ? portfolio.wallets.filter((wallet) => wallet.assets.some((asset) => asset.id === selectedAsset.id))
    : portfolio.wallets;
  const value = selectedAsset?.value ?? monitored.value;
  const baseline = selectedAsset?.investedValue ?? monitored.investedValue;
  const totalReturn = selectedAsset?.totalReturn ?? monitored.totalReturn;
  const returnPercent = selectedAsset?.returnPercent ?? monitored.returnPercent;
  const positive = totalReturn >= 0;
  const history = selectedAsset?.history ?? monitored.history;
  const subject = selectedAsset?.name ?? portfolio.name;

  return <div className="view-stack crypto-detail-view">
    {!monitored.connected && monitored.message && <div className="notice-row"><span><CircleAlert size={14} />{monitored.message}</span></div>}
    <section className="hero-metrics portfolio-metrics">
      <div className="balance-card">
        <p className="metric-label">{selectedAsset ? `${selectedAsset.symbol} equity` : "Current equity"}</p>
        <p className="balance">{formatMoney(value)}</p>
        <span className={positive ? "change positive" : "change negative"}>{positive ? <ArrowUpRight size={16} /> : <ArrowDownRight size={16} />}{returnPercent.toFixed(2)}% · {formatMoney(totalReturn)}</span>
        <small className="return-method">Since Sable began tracking this {selectedAsset ? "asset" : "wallet"}</small>
      </div>
      <Metric label="Tracked baseline" value={formatMoney(baseline)} />
      <Metric label="Tracked return" value={`${returnPercent.toFixed(2)}%`} helper={formatMoney(totalReturn)} valueClassName={positive ? "positive-text" : "negative-text"} />
      <Metric
        label={selectedAsset ? "Coin balance" : "Wallets"}
        value={selectedAsset ? `${selectedAsset.balance.toLocaleString(undefined, { maximumFractionDigits: 8 })} ${selectedAsset.symbol}` : String(portfolio.wallets.length)}
        helper={selectedAsset ? `${selectedAsset.walletCount} wallet${selectedAsset.walletCount === 1 ? "" : "s"}` : `${portfolio.assets.length} cryptocurrencies`}
      />
    </section>

    <section className="panel asset-breakdown-panel">
      <div className="panel-heading"><div><p className="section-label">Holdings</p><h2>Cryptocurrencies</h2></div><span className="muted">Select to investigate</span></div>
      <div className="asset-selector" role="list" aria-label="Cryptocurrency performance">
        <button className={selectedAssetId === "all" ? "asset-select active" : "asset-select"} onClick={() => setSelectedAssetId("all")}>
          <span className="network-mark all">Σ</span><span><small>Combined</small><strong>{formatMoney(portfolio.value)}</strong></span><span className={monitored.totalReturn >= 0 ? "positive-text" : "negative-text"}>{monitored.returnPercent.toFixed(2)}%</span>
        </button>
        {portfolio.assets.map((asset) => <button className={selectedAssetId === asset.id ? "asset-select active" : "asset-select"} key={asset.id} onClick={() => setSelectedAssetId(asset.id)}>
          <span className={`network-mark ${asset.id}`}>{asset.symbol.slice(0, 1)}</span><span><small>{asset.name} · {asset.allocation.toFixed(1)}%</small><strong>{formatMoney(asset.value)}</strong></span><span className={asset.totalReturn >= 0 ? "positive-text" : "negative-text"}>{asset.returnPercent.toFixed(2)}%</span>
        </button>)}
      </div>
    </section>

    <PerformanceChart
      history={history}
      format={formatMoney}
      formatAxis={formatWholeMoney}
      title={`${subject} performance`}
      valueLabel={selectedAsset ? `${selectedAsset.symbol} value` : "Wallet value"}
      baselineLabel="First tracked value"
      emptyText={`Sable recorded the first ${selectedAsset ? selectedAsset.symbol : "wallet"} snapshot today. Performance will appear after the next scheduled snapshot.`}
    />

    <section className="panel portfolio-panel crypto-wallet-panel">
      <div className="portfolio-header"><div><p className="section-label">Inside {portfolio.name}</p><h2>{selectedAsset ? `${selectedAsset.symbol} wallets` : "Wallets"}</h2></div><div className="portfolio-total"><strong>{visibleWallets.length}</strong><span className="muted">shown</span></div></div>
      {visibleWallets.length ? <div className="wallet-list">{visibleWallets.map((wallet) => {
        const holding = selectedAsset ? wallet.assets.find((asset) => asset.id === selectedAsset.id) : undefined;
        return <div className="wallet-row wallet-row-readonly" key={wallet.id}><span className={`network-mark ${holding?.id ?? wallet.network}`}>{(holding?.symbol ?? wallet.symbol).slice(0, 1)}</span><div className="wallet-identity"><strong>{wallet.label}</strong><small>{wallet.walletType === "xpub" ? `Bitcoin XPUB · ${wallet.addressCount} active derived address${wallet.addressCount === 1 ? "" : "es"}` : `${wallet.displayAddress.slice(0, 10)}…${wallet.displayAddress.slice(-8)}`}</small></div><div className="wallet-balance"><strong>{holding ? `${holding.balance.toLocaleString(undefined, { maximumFractionDigits: 8 })} ${holding.symbol}` : formatMoney(wallet.value)}</strong><small>{wallet.message ?? (holding ? formatMoney(holding.value) : wallet.assets.map((asset) => asset.symbol).join(" · "))}</small></div></div>;
      })}</div> : <Empty icon={<WalletCards size={22} />} title="No matching wallets" text="Add a public address or XPUB to begin tracking this cryptocurrency." />}
    </section>
    <p className="tracking-disclaimer">Tracked return measures price performance since Sable began tracking. Changes in coin balance are treated as deposits or withdrawals at the current market price; this is not tax-lot cost basis.</p>
  </div>;
}

function PeriodReturns({ monthly, yearly, formatMoney }: { monthly: PeriodReturn; yearly: PeriodReturn; formatMoney: (value: number) => string }) {
  const rows = [
    { label: "This month", short: "MTD", value: monthly },
    { label: "This year", short: "YTD", value: yearly },
  ];
  return <div className="panel period-returns-panel">
    <div className="panel-heading"><div><p className="section-label">Performance</p><h2>Period returns</h2></div></div>
    <div className="period-return-list">
      {rows.map(({ label, short, value }) => {
        const valueClass = value.amount >= 0 ? "positive-text" : "negative-text";
        return <div className="period-return-item" key={short}>
          <div className="period-return-label"><span>{label}</span><small>{short}</small></div>
          <strong className={valueClass}>{value.percent >= 0 ? "+" : ""}{value.percent.toFixed(2)}%</strong>
          <p className={valueClass}>{value.amount >= 0 ? "+" : ""}{formatMoney(value.amount)}</p>
        </div>;
      })}
    </div>
    <p className="period-return-note">Simple return · deposit timing ignored</p>
  </div>;
}

function Metric({ label, value, helper, valueClassName }: { label: string; value: string; helper?: string; valueClassName?: string }) {
  return <div className="metric-card"><p className="metric-label">{label}</p><strong className={valueClassName}>{value}</strong>{helper && <small>{helper}</small>}</div>;
}

function Wallets({ portfolios, formatMoney, onAddWallet, onChanged }: { portfolios: CryptoPortfolio[]; formatMoney: (value: number) => string; onAddWallet: () => void; onChanged: () => Promise<void> }) {
  const portfolio = portfolios[0];
  const removeWallet = async (id: number) => {
    await api.removeWallet(id);
    await onChanged();
  };
  return <div className="view-stack">
    <div className="wallet-actions"><button className="primary-button" onClick={onAddWallet}><Plus size={16} /> Add wallet</button></div>
    {portfolio ? (
      <section className="panel portfolio-panel" key={portfolio.id}>
        <div className="portfolio-header"><div><p className="section-label">On-chain assets</p><h2>Wallets</h2></div><div className="portfolio-total"><strong>{formatMoney(portfolio.value)}</strong></div></div>
        {portfolio.wallets.length ? <div className="wallet-list">{portfolio.wallets.map((wallet) => (
          <div className="wallet-row" key={wallet.id}><span className={`network-mark ${wallet.network}`}>{wallet.symbol.slice(0, 1)}</span><div className="wallet-identity"><strong>{wallet.label}</strong><small>{wallet.walletType === "xpub" ? `${wallet.addressCount} active derived address${wallet.addressCount === 1 ? "" : "es"}` : `${wallet.displayAddress.slice(0, 8)}…${wallet.displayAddress.slice(-6)}`}</small></div><div className="wallet-balance"><strong>{formatMoney(wallet.value)}</strong><small>{wallet.message ?? wallet.assets.map((asset) => `${asset.balance.toLocaleString(undefined, { maximumFractionDigits: 6 })} ${asset.symbol}`).join(" · ")}</small></div><button className="row-action" onClick={() => void removeWallet(wallet.id)} aria-label={`Remove ${wallet.label}`}><X size={16} /></button></div>
        ))}</div> : <Empty icon={<WalletCards size={22} />} title="No wallets yet" text="Add BTC addresses or XPUBs, ETH addresses, and SOL addresses to see their cumulative balance." action={<button className="primary-button" onClick={onAddWallet}><Plus size={16} /> Add first wallet</button>} />}
      </section>
    ) : null}
  </div>;
}

function WalletModal({ portfolios, onClose, onSaved }: { portfolios: CryptoPortfolio[]; onClose: () => void; onSaved: () => Promise<void> }) {
  const [form, setForm] = useState<AddWalletInput>({ portfolioId: portfolios[0]?.id ?? 0, network: "btc", address: "", label: "" });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => { event.preventDefault(); setSaving(true); setError(null); try { await api.addWallet(form); await onSaved(); } catch (caught) { setError(messageOf(caught)); setSaving(false); } };
  const networks = [{ value: "btc", label: "BTC" }, { value: "btc-xpub", label: "BTC XPUB" }, { value: "eth", label: "ETH" }, { value: "sol", label: "SOL" }] as const;
  const isXpub = form.network === "btc-xpub";
  return <Modal title="Add wallet" subtitle="Public addresses and extended public keys only. Never enter a seed phrase or private key." onClose={onClose}><form onSubmit={submit}>
    <label>Network<div className="network-options">{networks.map((network) => <button type="button" className={form.network === network.value ? "active" : ""} key={network.value} onClick={() => setForm({ ...form, network: network.value, address: "" })}>{network.label}</button>)}</div></label>
    <label>{isXpub ? "Extended public key" : "Wallet address"}<input autoFocus value={form.address} onChange={(event) => setForm({ ...form, address: event.target.value })} placeholder={form.network === "eth" ? "0x…" : form.network === "btc" ? "bc1…" : isXpub ? "xpub… / ypub… / zpub…" : "Public address"} required />{isXpub && <small className="field-note">Mainnet account-level keys only. Derivation happens locally; only derived addresses are queried.</small>}</label>
    <label>Label <span>optional</span><input value={form.label} onChange={(event) => setForm({ ...form, label: event.target.value })} placeholder="Cold wallet" /></label>
    {error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Adding…" : "Add wallet"}</button>
  </form></Modal>;
}

function WinningsModal({ winnings, currency, onClose, onSaved }: { winnings: MonthlyWinnings; currency: string; onClose: () => void; onSaved: () => Promise<void> }) {
  const [amount, setAmount] = useState(String(winnings.amount));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = Number(amount);
    if (!Number.isFinite(parsed)) {
      setError("Enter a valid monthly return amount.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.setOpessociusMonthlyReturn(parsed);
      await onSaved();
    } catch (caught) {
      setError(messageOf(caught));
      setSaving(false);
    }
  };
  return <Modal title={`${winnings.label} return`} subtitle={`Sable applies a ${winnings.defaultRatePercent.toFixed(2)}% month-end return by default. Override the total return for ${winnings.label} here.`} onClose={onClose}><form onSubmit={submit}>
    <label>Total return ({currency})<input autoFocus type="number" step="0.01" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} placeholder="0.00" required /><small className="field-note">Saving replaces the default. A lower or negative amount is attributed across this month and is never counted twice.</small></label>
    {error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Saving…" : "Save override"}</button>
  </form></Modal>;
}

function Modal({ title, subtitle, onClose, children }: { title: string; subtitle: string; onClose: () => void; children: React.ReactNode }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><div className="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title"><div className="modal-heading"><div><h2 id="modal-title">{title}</h2><p>{subtitle}</p></div><button className="icon-button" onClick={onClose} aria-label="Close"><X size={18} /></button></div>{children}</div></div>;
}

function Empty({ icon, title, text, action }: { icon: React.ReactNode; title: string; text: string; action?: React.ReactNode }) { return <div className="empty-state"><span>{icon}</span><strong>{title}</strong><p>{text}</p>{action}</div>; }
function Loading() { return <div className="loading-grid"><div /><div /><div /><div className="loading-chart" /></div>; }

export default App;
