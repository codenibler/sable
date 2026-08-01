import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDownRight,
  ArrowUpRight,
  BriefcaseBusiness,
  CircleAlert,
  LayoutDashboard,
  Plus,
  RefreshCw,
  WalletCards,
  X,
} from "lucide-react";
import { api } from "./api";
import { PerformanceChart } from "./components/PerformanceChart";
import type { AddWalletInput, CryptoPortfolio, Dashboard, MonthlyWinnings, PeriodReturn } from "./types";

type View = "overview" | "wallets";

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

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup" aria-label="Sable">
          <span className="brand-glyph">S</span>
          <strong>Sable</strong>
        </div>
        <nav>
          <button className={view === "overview" ? "active" : ""} onClick={() => setView("overview")}>
            <LayoutDashboard size={18} /> Overview
          </button>
          <button className={view === "wallets" ? "active" : ""} onClick={() => setView("wallets")}>
            <WalletCards size={18} /> Crypto wallets
          </button>
        </nav>
        <div className="sidebar-status">
          <p className="section-label">Connections</p>
          {dashboard?.sources.map((source) => (
            <div className="connection" key={source.id} title={source.message ?? undefined}>
              <span className={source.connected ? "status-dot ok" : "status-dot"} />
              <span>{source.name}</span>
            </div>
          ))}
        </div>
        <p className="local-note"><span className="status-dot ok" /> Local-only storage</p>
      </aside>

      <main className="content">
        <header className="topbar">
          <div>
            <p className="section-label">{view === "overview" ? "Net worth" : "On-chain portfolios"}</p>
            <h1>{view === "overview" ? "Overview" : "Crypto wallets"}</h1>
            {dashboard && <p className="updated">Updated {new Date(dashboard.updatedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</p>}
          </div>
          <div className="topbar-actions">
            <button className="icon-button" onClick={() => void refresh()} disabled={loading} aria-label="Refresh portfolio">
              <RefreshCw size={18} className={loading ? "spin" : ""} />
            </button>
            <button className="primary-button" onClick={() => setWalletModal(true)} disabled={!dashboard?.portfolios.length}>
              <Plus size={17} /> Add wallet
            </button>
          </div>
        </header>

        {error && (
          <div className="error-banner"><CircleAlert size={18} /><div><strong>Sable could not start</strong><p>{error}</p></div></div>
        )}

        {!dashboard && loading ? <Loading /> : dashboard && view === "overview" ? (
          <Overview dashboard={dashboard} formatMoney={formatMoney} onAddWinnings={() => setWinningsModal(true)} />
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
      {winningsModal && dashboard && (
        <WinningsModal winnings={dashboard.opessociusPreviousMonth} currency={dashboard.currency} onClose={() => setWinningsModal(false)} onSaved={async () => { setWinningsModal(false); await refresh(); }} />
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
                <div className="source-item" key={source.id}><span className={`source-icon ${source.kind}`}><span className={source.connected ? "status-dot ok" : "status-dot"} />{source.kind === "brokerage" ? "T2" : source.kind === "crypto" ? "₿" : "OP"}</span><div className="source-copy"><strong>{source.name}</strong><small>{source.kind === "manual" ? source.message : source.connected ? source.kind : source.message}</small>{source.id === "opessocius" && <button className="source-action" onClick={onAddWinnings}>{dashboard.opessociusPreviousMonth.amount > 0 ? "Edit winnings" : "Add winnings"}</button>}</div><div className="source-value"><strong>{formatMoney(source.value)}</strong><small className={source.returnValue >= 0 ? "positive-text" : "negative-text"}>{source.returnValue ? formatMoney(source.returnValue) : "—"}</small></div></div>
              ))}
            </div>
          </div>
          <PeriodReturns monthly={dashboard.monthlyReturn} yearly={dashboard.yearlyReturn} formatMoney={formatMoney} />
        </aside>
      </section>
    </div>
  );
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
  const [amount, setAmount] = useState(winnings.amount > 0 ? String(winnings.amount) : "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = Number(amount);
    if (!Number.isFinite(parsed) || parsed < 0) {
      setError("Enter a non-negative winnings amount.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.setOpessociusPreviousMonthWinnings(parsed);
      await onSaved();
    } catch (caught) {
      setError(messageOf(caught));
      setSaving(false);
    }
  };
  return <Modal title={`${winnings.label} winnings`} subtitle={`Add the total Opessocius winnings for ${winnings.label}. Sable spreads them evenly across the entire month for return calculations.`} onClose={onClose}><form onSubmit={submit}>
    <label>Total winnings ({currency})<input autoFocus type="number" min="0" step="0.01" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} placeholder="0.00" required /><small className="field-note">Saving replaces the existing value for this month, so edits are never counted twice.</small></label>
    {error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Saving…" : winnings.amount > 0 ? "Update winnings" : "Add winnings"}</button>
  </form></Modal>;
}

function Modal({ title, subtitle, onClose, children }: { title: string; subtitle: string; onClose: () => void; children: React.ReactNode }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><div className="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title"><div className="modal-heading"><div><h2 id="modal-title">{title}</h2><p>{subtitle}</p></div><button className="icon-button" onClick={onClose} aria-label="Close"><X size={18} /></button></div>{children}</div></div>;
}

function Empty({ icon, title, text, action }: { icon: React.ReactNode; title: string; text: string; action?: React.ReactNode }) { return <div className="empty-state"><span>{icon}</span><strong>{title}</strong><p>{text}</p>{action}</div>; }
function Loading() { return <div className="loading-grid"><div /><div /><div /><div className="loading-chart" /></div>; }

export default App;
