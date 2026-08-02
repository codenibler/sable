import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDownRight,
  ArrowUpRight,
  BriefcaseBusiness,
  Building2,
  CircleAlert,
  HardDrive,
  LayoutDashboard,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  RefreshCw,
  WalletCards,
  X,
} from "lucide-react";
import { api } from "./api";
import { PerformanceChart } from "./components/PerformanceChart";
import type { AddWalletInput, CryptoPortfolio, Dashboard, MonitoredPortfolio, MonthlyWinnings, PeriodReturn } from "./types";

type View = "overview" | "wallets" | `portfolio:${string}`;

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
  const [view, setView] = useState<View>("overview");
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [walletModal, setWalletModal] = useState(false);
  const [winningsModal, setWinningsModal] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setDashboard(await api.dashboard());
    } catch (caught) {
      setError(messageOf(caught));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("history-synced", () => void refresh()).then((stopListening) => {
      unlisten = stopListening;
    });
    return () => unlisten?.();
  }, [refresh]);

  const currency = dashboard?.currency ?? "EUR";
  const money = useMemo(
    () => new Intl.NumberFormat(undefined, { style: "currency", currency, maximumFractionDigits: 2 }),
    [currency],
  );
  const formatMoney = (value: number) => money.format(value);
  const selectedPortfolio = view.startsWith("portfolio:")
    ? dashboard?.monitoredPortfolios.find((portfolio) => `portfolio:${portfolio.id}` === view)
    : undefined;
  const pageLabel = selectedPortfolio ? "Monitored portfolio" : view === "overview" ? "Net worth" : "On-chain portfolios";
  const pageTitle = selectedPortfolio?.name ?? (view === "overview" ? "Overview" : "Crypto wallets");

  return (
    <div className={`app-shell ${sidebarCollapsed ? "sidebar-collapsed" : ""}`}>
      <aside className="sidebar">
        <button className="sidebar-toggle" onClick={() => setSidebarCollapsed((collapsed) => !collapsed)} aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}>
          {sidebarCollapsed ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
        </button>
        <nav>
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
          <p className="section-label sidebar-section-label">Manage</p>
          <button className={view === "wallets" ? "active" : ""} onClick={() => setView("wallets")}>
            <WalletCards size={18} /><span className="nav-copy">Crypto wallets</span>
          </button>
        </nav>
        <p className="local-note"><span className="status-dot ok" /><span className="nav-copy">Local-only storage</span></p>
      </aside>

      <main className="content">
        <header className="topbar">
          <div>
            <p className="section-label">{pageLabel}</p>
            <h1>{pageTitle}</h1>
            {dashboard && <p className="updated">Updated {new Date(dashboard.updatedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</p>}
          </div>
          <div className="topbar-actions">
            <button className="icon-button" onClick={() => void refresh()} disabled={loading} aria-label="Refresh portfolio">
              <RefreshCw size={18} className={loading ? "spin" : ""} />
            </button>
            {view === "wallets" && <button className="primary-button" onClick={() => setWalletModal(true)} disabled={!dashboard?.portfolios.length}><Plus size={17} /> Add wallet</button>}
          </div>
        </header>

        {error && (
          <div className="error-banner"><CircleAlert size={18} /><div><strong>Sable could not start</strong><p>{error}</p></div></div>
        )}

        {!dashboard && loading ? <Loading /> : dashboard && view === "overview" ? (
          <Overview dashboard={dashboard} formatMoney={formatMoney} onAddWinnings={() => setWinningsModal(true)} />
        ) : dashboard && selectedPortfolio ? (
          <PortfolioDetailView portfolio={selectedPortfolio} formatMoney={formatMoney} onEditReturn={selectedPortfolio.id === "opessocius" && dashboard.opessociusMonthlyReturn ? () => setWinningsModal(true) : undefined} />
        ) : dashboard ? (
          <Wallets
            portfolios={dashboard.portfolios}
            formatMoney={formatMoney}
            onAddWallet={() => setWalletModal(true)}
            onChanged={refresh}
          />
        ) : null}
      </main>

      {walletModal && dashboard && (
        <WalletModal portfolios={dashboard.portfolios} onClose={() => setWalletModal(false)} onSaved={async () => { setWalletModal(false); await refresh(); }} />
      )}
      {winningsModal && dashboard?.opessociusMonthlyReturn && (
        <WinningsModal winnings={dashboard.opessociusMonthlyReturn} currency={dashboard.currency} onClose={() => setWinningsModal(false)} onSaved={async () => { setWinningsModal(false); await refresh(); }} />
      )}
    </div>
  );
}

function Overview({ dashboard, formatMoney, onAddWinnings }: { dashboard: Dashboard; formatMoney: (value: number) => string; onAddWinnings: () => void }) {
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

      <PerformanceChart history={dashboard.history} format={formatMoney} />

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
                <div className="source-item" key={source.id}><span className={`source-icon ${source.kind}`}><span className={source.connected ? "status-dot ok" : "status-dot"} />{source.kind === "brokerage" ? "T2" : source.kind === "crypto" ? "₿" : "OP"}</span><div className="source-copy"><strong>{source.name}</strong><small>{source.kind === "manual" ? source.message : source.connected ? source.kind : source.message}</small>{source.id === "opessocius" && dashboard.opessociusMonthlyReturn && <button className="source-action" onClick={onAddWinnings}>{dashboard.opessociusMonthlyReturn.isOverride ? "Edit return" : "Override return"}</button>}</div><div className="source-value"><strong>{formatMoney(source.value)}</strong><small className={source.returnValue >= 0 ? "positive-text" : "negative-text"}>{source.returnValue ? formatMoney(source.returnValue) : "—"}</small></div></div>
              ))}
            </div>
          </div>
          <PeriodReturns monthly={dashboard.monthlyReturn} yearly={dashboard.yearlyReturn} formatMoney={formatMoney} />
        </aside>
      </section>
    </div>
  );
}

function PortfolioDetailView({ portfolio, formatMoney, onEditReturn }: { portfolio: MonitoredPortfolio; formatMoney: (value: number) => string; onEditReturn?: () => void }) {
  const positive = portfolio.totalReturn >= 0;
  const latestPeriod = portfolio.periods.at(-1);
  return <div className="view-stack portfolio-detail-view">
    {!portfolio.connected && portfolio.message && <div className="notice-row"><span><CircleAlert size={14} />{portfolio.message}</span></div>}
    <section className="hero-metrics portfolio-metrics">
      <div className="balance-card">
        <p className="metric-label">Current equity</p>
        <p className="balance">{formatMoney(portfolio.value)}</p>
        <span className={positive ? "change positive" : "change negative"}>{positive ? <ArrowUpRight size={16} /> : <ArrowDownRight size={16} />}{portfolio.returnPercent.toFixed(2)}% · {formatMoney(portfolio.totalReturn)}</span>
        <small className="return-method">{portfolio.connected ? "Portfolio connected" : "Showing last available history"}</small>
      </div>
      <Metric label="Invested" value={formatMoney(portfolio.investedValue)} />
      <Metric label="Total return" value={`${portfolio.returnPercent.toFixed(2)}%`} helper={formatMoney(portfolio.totalReturn)} valueClassName={positive ? "positive-text" : "negative-text"} />
      <Metric label={latestPeriod ? "Latest month" : portfolio.itemLabel} value={latestPeriod ? `${latestPeriod.returnPercent.toFixed(2)}%` : String(portfolio.itemCount)} helper={latestPeriod ? formatMoney(latestPeriod.returnValue) : `${portfolio.itemCount} ${portfolio.itemLabel}`} valueClassName={latestPeriod ? latestPeriod.returnValue >= 0 ? "positive-text" : "negative-text" : undefined} />
    </section>
    <PerformanceChart history={portfolio.history} format={formatMoney} />
    {portfolio.periods.length > 0 && <section className="panel history-panel">
      <div className="panel-heading"><div><p className="section-label">Monthly ledger</p><h2>Opessocius history</h2></div>{onEditReturn && <button className="secondary-button" onClick={onEditReturn}>Edit latest return</button>}</div>
      <div className="table-wrap"><table><thead><tr><th>Month</th><th>Rate</th><th>Return</th><th>Deposits</th><th>Withdrawals</th><th>Ending equity</th></tr></thead><tbody>
        {[...portfolio.periods].reverse().map((period) => <tr key={period.month}><td><strong>{period.label}</strong></td><td className={period.returnValue >= 0 ? "positive-text" : "negative-text"}>{period.returnPercent.toFixed(2)}%</td><td>{formatMoney(period.returnValue)}</td><td>{period.deposits ? formatMoney(period.deposits) : "—"}</td><td>{period.withdrawals ? formatMoney(period.withdrawals) : "—"}</td><td>{formatMoney(period.endingValue)}</td></tr>)}
      </tbody></table></div>
    </section>}
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
          <div className="wallet-row" key={wallet.id}><span className={`network-mark ${wallet.network}`}>{wallet.symbol.slice(0, 1)}</span><div className="wallet-identity"><strong>{wallet.label}</strong><small>{wallet.walletType === "xpub" ? `${wallet.addressCount} active derived address${wallet.addressCount === 1 ? "" : "es"}` : `${wallet.displayAddress.slice(0, 8)}…${wallet.displayAddress.slice(-6)}`}</small></div><div className="wallet-balance"><strong>{wallet.balance.toLocaleString(undefined, { maximumFractionDigits: 6 })} {wallet.symbol}</strong><small>{wallet.message ?? formatMoney(wallet.value)}</small></div><button className="row-action" onClick={() => void removeWallet(wallet.id)} aria-label={`Remove ${wallet.label}`}><X size={16} /></button></div>
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
