import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDownRight,
  ArrowUpRight,
  BriefcaseBusiness,
  ChevronRight,
  CircleAlert,
  LayoutDashboard,
  Plus,
  RefreshCw,
  Trash2,
  WalletCards,
  X,
} from "lucide-react";
import { api } from "./api";
import { PerformanceChart } from "./components/PerformanceChart";
import type { AddWalletInput, CryptoPortfolio, Dashboard } from "./types";

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
  const [portfolioModal, setPortfolioModal] = useState(false);

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
        <button className="brand" onClick={() => setView("overview")} aria-label="Portfolio 1 home">
          <span>P</span>
          <strong>Portfolio 1</strong>
        </button>
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
            <button className="primary-button" onClick={() => dashboard?.portfolios.length ? setWalletModal(true) : setPortfolioModal(true)}>
              <Plus size={17} /> Add wallet
            </button>
          </div>
        </header>

        {error && (
          <div className="error-banner"><CircleAlert size={18} /><div><strong>Portfolio 1 could not start</strong><p>{error}</p></div></div>
        )}

        {!dashboard && loading ? <Loading /> : dashboard && view === "overview" ? (
          <Overview dashboard={dashboard} formatMoney={formatMoney} />
        ) : dashboard ? (
          <Wallets
            portfolios={dashboard.portfolios}
            formatMoney={formatMoney}
            onAddPortfolio={() => setPortfolioModal(true)}
            onAddWallet={() => setWalletModal(true)}
            onChanged={refresh}
          />
        ) : null}
      </main>

      {walletModal && dashboard && (
        <WalletModal portfolios={dashboard.portfolios} onClose={() => setWalletModal(false)} onSaved={async () => { setWalletModal(false); await refresh(); }} />
      )}
      {portfolioModal && (
        <PortfolioModal onClose={() => setPortfolioModal(false)} onSaved={async () => { setPortfolioModal(false); await refresh(); setWalletModal(true); }} />
      )}
    </div>
  );
}

function Overview({ dashboard, formatMoney }: { dashboard: Dashboard; formatMoney: (value: number) => string }) {
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
          label="MWRR"
          value={dashboard.moneyWeightedReturnPercent == null ? "Building" : `${dashboard.moneyWeightedReturnPercent.toFixed(2)}%`}
          helper={dashboard.moneyWeightedReturnPercent == null ? "Completes after backfill" : "Annualised · Trading 212"}
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
        <div className="panel sources-panel">
          <div className="panel-heading"><div><p className="section-label">Breakdown</p><h2>Sources</h2></div></div>
          <div className="source-list">
            {dashboard.sources.map((source) => (
              <div className="source-item" key={source.id}><span className={`source-icon ${source.kind}`}><span className={source.connected ? "status-dot ok" : "status-dot"} />{source.kind === "brokerage" ? "T2" : "₿"}</span><div><strong>{source.name}</strong><small>{source.connected ? source.kind : source.message}</small></div><div className="source-value"><strong>{formatMoney(source.value)}</strong><small className={source.returnValue >= 0 ? "positive-text" : "negative-text"}>{source.returnValue ? formatMoney(source.returnValue) : "—"}</small></div></div>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}

function Metric({ label, value, helper }: { label: string; value: string; helper?: string }) {
  return <div className="metric-card"><p className="metric-label">{label}</p><strong>{value}</strong>{helper && <small>{helper}</small>}</div>;
}

function Wallets({ portfolios, formatMoney, onAddPortfolio, onAddWallet, onChanged }: { portfolios: CryptoPortfolio[]; formatMoney: (value: number) => string; onAddPortfolio: () => void; onAddWallet: () => void; onChanged: () => Promise<void> }) {
  const removePortfolio = async (portfolio: CryptoPortfolio) => {
    if (!window.confirm(`Delete “${portfolio.name}” and its local wallet list?`)) return;
    await api.deletePortfolio(portfolio.id);
    await onChanged();
  };
  const removeWallet = async (id: number) => {
    await api.removeWallet(id);
    await onChanged();
  };
  return <div className="view-stack">
    <div className="wallet-actions"><button className="secondary-button" onClick={onAddPortfolio}><Plus size={16} /> New portfolio</button><button className="primary-button" onClick={onAddWallet} disabled={!portfolios.length}><Plus size={16} /> Add wallet</button></div>
    {portfolios.length ? portfolios.map((portfolio) => (
      <section className="panel portfolio-panel" key={portfolio.id}>
        <div className="portfolio-header"><div><p className="section-label">Crypto portfolio</p><h2>{portfolio.name}</h2></div><div className="portfolio-total"><strong>{formatMoney(portfolio.value)}</strong><button className="ghost-danger" onClick={() => void removePortfolio(portfolio)} aria-label={`Delete ${portfolio.name}`}><Trash2 size={16} /></button></div></div>
        {portfolio.wallets.length ? <div className="wallet-list">{portfolio.wallets.map((wallet) => (
          <div className="wallet-row" key={wallet.id}><span className={`network-mark ${wallet.network}`}>{wallet.symbol.slice(0, 1)}</span><div className="wallet-identity"><strong>{wallet.label}</strong><small>{wallet.address.slice(0, 8)}…{wallet.address.slice(-6)}</small></div><div className="wallet-balance"><strong>{wallet.balance.toLocaleString(undefined, { maximumFractionDigits: 6 })} {wallet.symbol}</strong><small>{wallet.message ?? formatMoney(wallet.value)}</small></div><button className="row-action" onClick={() => void removeWallet(wallet.id)} aria-label={`Remove ${wallet.label}`}><X size={16} /></button></div>
        ))}</div> : <Empty icon={<WalletCards size={22} />} title="No wallets in this portfolio" text="Add BTC, ETH, or SOL addresses to see their cumulative balance." action={<button className="text-button" onClick={onAddWallet}>Add first wallet <ChevronRight size={15} /></button>} />}
      </section>
    )) : <section className="panel"><Empty icon={<WalletCards size={24} />} title="Create your first crypto portfolio" text="A portfolio can contain any collection of BTC, ETH, and SOL addresses." action={<button className="primary-button" onClick={onAddPortfolio}><Plus size={16} /> New portfolio</button>} /></section>}
  </div>;
}

function WalletModal({ portfolios, onClose, onSaved }: { portfolios: CryptoPortfolio[]; onClose: () => void; onSaved: () => Promise<void> }) {
  const [form, setForm] = useState<AddWalletInput>({ portfolioId: portfolios[0]?.id ?? 0, network: "btc", address: "", label: "" });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => { event.preventDefault(); setSaving(true); setError(null); try { await api.addWallet(form); await onSaved(); } catch (caught) { setError(messageOf(caught)); setSaving(false); } };
  return <Modal title="Add wallet" subtitle="Public addresses only. Portfolio 1 never requests seed phrases or private keys." onClose={onClose}><form onSubmit={submit}>
    <label>Portfolio<select value={form.portfolioId} onChange={(event) => setForm({ ...form, portfolioId: Number(event.target.value) })}>{portfolios.map((portfolio) => <option key={portfolio.id} value={portfolio.id}>{portfolio.name}</option>)}</select></label>
    <label>Network<div className="network-options">{(["btc", "eth", "sol"] as const).map((network) => <button type="button" className={form.network === network ? "active" : ""} key={network} onClick={() => setForm({ ...form, network })}>{network.toUpperCase()}</button>)}</div></label>
    <label>Wallet address<input autoFocus value={form.address} onChange={(event) => setForm({ ...form, address: event.target.value })} placeholder={form.network === "eth" ? "0x…" : form.network === "btc" ? "bc1…" : "Public address"} required /></label>
    <label>Label <span>optional</span><input value={form.label} onChange={(event) => setForm({ ...form, label: event.target.value })} placeholder="Cold wallet" /></label>
    {error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Adding…" : "Add wallet"}</button>
  </form></Modal>;
}

function PortfolioModal({ onClose, onSaved }: { onClose: () => void; onSaved: () => Promise<void> }) {
  const [name, setName] = useState(""); const [error, setError] = useState<string | null>(null); const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => { event.preventDefault(); setSaving(true); try { await api.createPortfolio(name); await onSaved(); } catch (caught) { setError(messageOf(caught)); setSaving(false); } };
  return <Modal title="New crypto portfolio" subtitle="Group any collection of public wallet addresses." onClose={onClose}><form onSubmit={submit}><label>Portfolio name<input autoFocus value={name} onChange={(event) => setName(event.target.value)} placeholder="Long-term crypto" required maxLength={60} /></label>{error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Creating…" : "Create portfolio"}</button></form></Modal>;
}

function Modal({ title, subtitle, onClose, children }: { title: string; subtitle: string; onClose: () => void; children: React.ReactNode }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><div className="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title"><div className="modal-heading"><div><h2 id="modal-title">{title}</h2><p>{subtitle}</p></div><button className="icon-button" onClick={onClose} aria-label="Close"><X size={18} /></button></div>{children}</div></div>;
}

function Empty({ icon, title, text, action }: { icon: React.ReactNode; title: string; text: string; action?: React.ReactNode }) { return <div className="empty-state"><span>{icon}</span><strong>{title}</strong><p>{text}</p>{action}</div>; }
function Loading() { return <div className="loading-grid"><div /><div /><div /><div className="loading-chart" /></div>; }

export default App;
