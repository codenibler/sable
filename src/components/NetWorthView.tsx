import { FormEvent, useMemo, useState } from "react";
import { ArrowDownRight, ArrowUpRight, ChevronDown, ChevronUp, CircleAlert, Pencil, Plus, Trash2, Trophy, X } from "lucide-react";
import { api } from "../api";
import type { NetWorthEntry, SaveNetWorthInput } from "../types";

type CategoryKey = "stocks" | "opessocius" | "crypto" | "savings" | "spending" | "receivables" | "cash" | "misc";

const categories: { key: CategoryKey; label: string; color: string }[] = [
  { key: "stocks", label: "Stocks", color: "#bda66f" },
  { key: "opessocius", label: "Opessocius", color: "#7f8f86" },
  { key: "crypto", label: "Crypto", color: "#8b7ca8" },
  { key: "savings", label: "Savings", color: "#6f8d98" },
  { key: "spending", label: "Spending", color: "#a87467" },
  { key: "receivables", label: "Receivables", color: "#98865e" },
  { key: "cash", label: "Cash", color: "#688074" },
  { key: "misc", label: "Misc", color: "#716d68" },
];

const day = 24 * 60 * 60 * 1000;
const dateValue = (date: string) => new Date(`${date}T00:00:00`).getTime();
const formatDate = (date: string, options?: Intl.DateTimeFormatOptions) => new Date(`${date}T00:00:00`).toLocaleDateString(undefined, options ?? { day: "numeric", month: "short", year: "numeric" });

export function NetWorthView({ entries, formatMoney, onChanged }: { entries: NetWorthEntry[]; formatMoney: (value: number) => string; onChanged: () => Promise<void> }) {
  const [editing, setEditing] = useState<NetWorthEntry | "new" | null>(null);
  const [selectedDate, setSelectedDate] = useState(entries.at(-1)?.date ?? "");
  const [actionError, setActionError] = useState<string | null>(null);

  const analytics = useMemo(() => calculateAnalytics(entries), [entries]);
  const selected = entries.find((entry) => entry.date === selectedDate) ?? entries.at(-1);

  const remove = async (entry: NetWorthEntry) => {
    if (!window.confirm(`Remove the net worth snapshot from ${formatDate(entry.date)}?`)) return;
    setActionError(null);
    try {
      await api.removeNetWorthEntry(entry.date);
      await onChanged();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    }
  };

  if (!entries.length) {
    return <div className="view-stack"><section className="panel empty-state"><Trophy size={24} /><strong>No net worth history yet</strong><p>Add your first dated balance snapshot to start the progression chart.</p><button className="primary-button" onClick={() => setEditing("new")}><Plus size={16} /> Add snapshot</button></section>{editing && <NetWorthModal entry={editing === "new" ? undefined : editing} latest={entries.at(-1)} formatMoney={formatMoney} onClose={() => setEditing(null)} onSaved={async () => { setEditing(null); await onChanged(); }} />}</div>;
  }

  const latest = entries.at(-1)!;
  return <div className="view-stack net-worth-view">
    {actionError && <div className="error-banner"><CircleAlert size={18} /><div><strong>Could not update net worth</strong><p>{actionError}</p></div></div>}
    <section className="hero-metrics net-worth-metrics">
      <div className="balance-card">
        <p className="metric-label">Current net worth</p>
        <p className="balance">{formatMoney(latest.netWorth)}</p>
        <span className={analytics.latestChange >= 0 ? "change positive" : "change negative"}>{analytics.latestChange >= 0 ? <ArrowUpRight size={16} /> : <ArrowDownRight size={16} />}{analytics.latestChangePercent.toFixed(2)}% · {formatMoney(analytics.latestChange)}</span>
        <small className="return-method">Since {formatDate(entries.at(-2)?.date ?? entries[0].date, { day: "numeric", month: "short" })}</small>
      </div>
      <NetWorthMetric label="Since first entry" value={`${analytics.totalGrowthPercent >= 0 ? "+" : ""}${analytics.totalGrowthPercent.toFixed(1)}%`} helper={formatMoney(analytics.totalGrowth)} positive={analytics.totalGrowth >= 0} />
      <NetWorthMetric label="Monthly growth pace" value={`${analytics.monthlyGrowthRate >= 0 ? "+" : ""}${analytics.monthlyGrowthRate.toFixed(2)}%`} helper={`${analytics.positiveChanges} of ${analytics.changeCount} gains`} positive={analytics.monthlyGrowthRate >= 0} />
      <NetWorthMetric label="All-time high" value={formatMoney(analytics.high.netWorth)} helper={formatDate(analytics.high.date)} positive />
    </section>

    <section className="panel net-worth-chart-panel">
      <div className="panel-heading"><div><p className="section-label">Progression</p><h2>Net worth by date</h2></div><div className="chart-focus"><strong>{formatMoney(selected?.netWorth ?? 0)}</strong><small>{selected ? formatDate(selected.date) : ""}</small></div></div>
      <div className="net-worth-chart-scroll">
        <div className="net-worth-bars" style={{ gridTemplateColumns: `repeat(${entries.length}, minmax(42px, 1fr))` }}>
          {entries.map((entry, index) => {
            const previous = entries[index - 1];
            const change = previous ? entry.netWorth - previous.netWorth : 0;
            const height = Math.max((entry.netWorth / analytics.high.netWorth) * 100, 3);
            const isHigh = entry.date === analytics.high.date;
            return <button className={`net-worth-bar-column ${selected?.date === entry.date ? "selected" : ""}`} key={entry.date} onClick={() => setSelectedDate(entry.date)} title={`${formatDate(entry.date)} · ${formatMoney(entry.netWorth)}`}>
              <span className={change >= 0 ? "bar-change positive-text" : "bar-change negative-text"}>{index ? `${change >= 0 ? "+" : ""}${Math.round(change).toLocaleString()}` : "Start"}</span>
              <span className={`net-worth-bar ${isHigh ? "ath" : ""}`} style={{ height: `${height}%` }}>{isHigh && <Trophy size={12} />}</span>
              <small>{formatDate(entry.date, { month: "short", year: "2-digit" })}</small>
            </button>;
          })}
        </div>
      </div>
      <div className="net-worth-chart-legend"><span>Bars start at zero</span><span>{entries.length} snapshots · {Math.round(analytics.elapsedDays)} days</span></div>
    </section>

    <section className="net-worth-lower-grid">
      <div className="panel allocation-panel">
        <div className="panel-heading"><div><p className="section-label">Latest snapshot</p><h2>Allocation</h2></div><span className="muted">{formatDate(latest.date)}</span></div>
        <div className="net-worth-allocation-list">{analytics.allocation.map((category) => <div className="net-worth-allocation" key={category.key}><div><span>{category.label}</span><strong>{formatMoney(category.value)}</strong></div><i><b style={{ width: `${category.percent}%` }} /></i><small>{category.percent.toFixed(1)}%</small></div>)}</div>
      </div>
      <div className="panel growth-insights-panel">
        <div className="panel-heading"><div><p className="section-label">Analytics</p><h2>Growth signals</h2></div></div>
        <div className="growth-insights">
          <Insight label="Average change" value={formatMoney(analytics.averageChange)} note="per recorded snapshot" positive={analytics.averageChange >= 0} />
          <Insight label="Largest increase" value={`+${formatMoney(analytics.best.amount)}`} note={analytics.best.date ? formatDate(analytics.best.date) : "—"} positive />
          <Insight label="Largest decrease" value={formatMoney(analytics.worst.amount)} note={analytics.worst.date ? formatDate(analytics.worst.date) : "—"} positive={analytics.worst.amount >= 0} />
          <Insight label="Current streak" value={`${analytics.streak} ${analytics.streakDirection}`} note="consecutive snapshots" positive={analytics.streakDirection === "gains"} />
        </div>
      </div>
    </section>
    <CompositionCharts entries={entries} latest={latest} formatMoney={formatMoney} />
    <p className="tracking-disclaimer">Growth includes deposits, withdrawals, and reclassification between categories; it is a balance-sheet progression, not investment return. MoM columns compare each snapshot with the prior recorded date, including multiple entries within one month.</p>

    <section className="panel net-worth-history-panel">
      <div className="panel-heading"><div><p className="section-label">Ledger</p><h2>Snapshot history</h2></div><button className="primary-button" onClick={() => setEditing("new")}><Plus size={15} /> Add snapshot</button></div>
      <div className="table-wrap"><table className="net-worth-table"><thead><tr><th>Date</th><th>Net worth</th>{categories.map((category) => <th key={category.key}>{category.label}</th>)}<th>MoM Δ</th><th>MoM (+)</th><th>MoM (−)</th><th>ATH</th><th /></tr></thead><tbody>
        {[...entries].reverse().map((entry, reverseIndex) => {
          const index = entries.length - 1 - reverseIndex;
          const change = index ? entry.netWorth - entries[index - 1].netWorth : 0;
          return <tr key={entry.date}><td><strong>{formatDate(entry.date)}</strong></td><td><strong>{formatMoney(entry.netWorth)}</strong></td>{categories.map((category) => <td key={category.key}>{formatMoney(entry[category.key])}</td>)}<td className={change >= 0 ? "positive-text" : "negative-text"}>{index ? formatMoney(change) : "—"}</td><td>{index && change > 0 ? formatMoney(change) : "—"}</td><td>{index && change < 0 ? formatMoney(change) : "—"}</td><td>{entry.date === analytics.high.date ? <span className="ath-badge"><Trophy size={11} /> ATH</span> : "—"}</td><td><div className="ledger-actions"><button className="row-action" onClick={() => setEditing(entry)} aria-label={`Edit ${formatDate(entry.date)}`}><Pencil size={14} /></button><button className="row-action" onClick={() => void remove(entry)} aria-label={`Remove ${formatDate(entry.date)}`}><Trash2 size={14} /></button></div></td></tr>;
        })}
      </tbody></table></div>
    </section>

    {editing && <NetWorthModal entry={editing === "new" ? undefined : editing} latest={latest} formatMoney={formatMoney} onClose={() => setEditing(null)} onSaved={async () => { setEditing(null); await onChanged(); }} />}
  </div>;
}

function NetWorthMetric({ label, value, helper, positive }: { label: string; value: string; helper: string; positive: boolean }) {
  return <div className="metric-card"><p className="metric-label">{label}</p><strong className={positive ? "positive-text" : "negative-text"}>{value}</strong><small>{helper}</small></div>;
}

function Insight({ label, value, note, positive }: { label: string; value: string; note: string; positive: boolean }) {
  return <div className="growth-insight"><span>{label}</span><strong className={positive ? "positive-text" : "negative-text"}>{value}</strong><small>{note}</small></div>;
}

function CompositionCharts({ entries, latest, formatMoney }: { entries: NetWorthEntry[]; latest: NetWorthEntry; formatMoney: (value: number) => string }) {
  const monthlyEntries = useMemo(() => {
    const byMonth = new Map<string, NetWorthEntry>();
    entries.forEach((entry) => byMonth.set(entry.date.slice(0, 7), entry));
    return [...byMonth.values()];
  }, [entries]);
  const latestComposition = categories
    .map((category) => ({ ...category, value: latest[category.key], percent: latest.netWorth > 0 ? latest[category.key] / latest.netWorth * 100 : 0 }))
    .filter((category) => category.value > 0)
    .sort((left, right) => right.value - left.value);
  let turn = 0;
  const pieStops = latestComposition.map((category) => {
    const start = turn;
    turn += category.percent;
    return `${category.color} ${start}% ${turn}%`;
  }).join(", ");

  return <section className="panel composition-panel">
    <div className="panel-heading composition-heading">
      <div><p className="section-label">Portfolio structure</p><h2>Net worth composition</h2></div>
      <span className="muted">Monthly share of total</span>
    </div>
    <div className="composition-grid">
      <div className="composition-history">
        <div className="composition-subheading"><div><strong>Composition over time</strong><small>Latest snapshot from each month</small></div></div>
        <div className="composition-scroll">
          <div className="composition-bars" style={{ gridTemplateColumns: `repeat(${monthlyEntries.length}, minmax(52px, 1fr))` }}>
            {monthlyEntries.map((entry) => <div className="composition-month" key={entry.date}>
              <div className="composition-stack" aria-label={`${formatDate(entry.date, { month: "long", year: "numeric" })} composition`}>
                {categories.map((category) => {
                  const percent = entry.netWorth > 0 ? entry[category.key] / entry.netWorth * 100 : 0;
                  if (percent <= 0) return null;
                  return <span key={category.key} style={{ height: `${percent}%`, background: category.color }} title={`${category.label}: ${percent.toFixed(1)}% · ${formatMoney(entry[category.key])}`}>
                    {percent >= 10 && <small>{percent.toFixed(0)}%</small>}
                  </span>;
                })}
              </div>
              <small>{formatDate(entry.date, { month: "short", year: "2-digit" })}</small>
            </div>)}
          </div>
        </div>
        <div className="composition-legend">{categories.map((category) => <span key={category.key}><i style={{ background: category.color }} />{category.label}</span>)}</div>
      </div>
      <div className="latest-composition">
        <div className="composition-subheading"><div><strong>Latest composition</strong><small>{formatDate(latest.date)}</small></div></div>
        <div className="composition-pie-wrap">
          <div className="composition-pie" style={{ background: pieStops ? `conic-gradient(${pieStops})` : "#24231f" }} role="img" aria-label={`Latest net worth composition as of ${formatDate(latest.date)}`}>
            <div><span>Total</span><strong>{formatMoney(latest.netWorth)}</strong></div>
          </div>
        </div>
        <div className="composition-breakdown">{latestComposition.map((category) => <div key={category.key}>
          <i style={{ background: category.color }} /><span>{category.label}</span><strong>{category.percent.toFixed(1)}%</strong><small>{formatMoney(category.value)}</small>
        </div>)}</div>
      </div>
    </div>
  </section>;
}

function NetWorthModal({ entry, latest, formatMoney, onClose, onSaved }: { entry?: NetWorthEntry; latest?: NetWorthEntry; formatMoney: (value: number) => string; onClose: () => void; onSaved: () => Promise<void> }) {
  const starting = entry ?? latest;
  const [date, setDate] = useState(entry?.date ?? new Date().toISOString().slice(0, 10));
  const [values, setValues] = useState<Record<CategoryKey, string>>(() => Object.fromEntries(categories.map(({ key }) => [key, String(starting?.[key] ?? 0)])) as Record<CategoryKey, string>);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const total = categories.reduce((sum, { key }) => sum + (Number(values[key]) || 0), 0);
  const adjustValue = (key: CategoryKey, direction: 1 | -1) => {
    const current = Number(values[key]) || 0;
    const next = Math.max(0, Math.round((current + direction * 0.01) * 100) / 100);
    setValues((currentValues) => ({ ...currentValues, [key]: String(next) }));
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const amounts = Object.fromEntries(categories.map(({ key }) => [key, Number(values[key])])) as Record<CategoryKey, number>;
    if (!date || Object.values(amounts).some((amount) => !Number.isFinite(amount) || amount < 0)) {
      setError("Enter a date and non-negative amount for every category.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.saveNetWorthEntry({ date, ...amounts } as SaveNetWorthInput);
      await onSaved();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setSaving(false);
    }
  };

  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><div className="modal net-worth-modal" role="dialog" aria-modal="true" aria-labelledby="net-worth-modal-title"><div className="modal-heading"><div><h2 id="net-worth-modal-title">{entry ? "Edit snapshot" : "Add net worth snapshot"}</h2><p>Enter each balance category. Sable calculates the total and progression automatically.</p></div><button className="icon-button" onClick={onClose} aria-label="Close"><X size={18} /></button></div><form onSubmit={submit}>
    <label>Date<input autoFocus type="date" value={date} disabled={Boolean(entry)} onChange={(event) => setDate(event.target.value)} required /></label>
    <div className="net-worth-fields">{categories.map(({ key, label }) => <label key={key}>{label} (€)<span className="number-input"><input type="number" min="0" step="0.01" inputMode="decimal" value={values[key]} onChange={(event) => setValues({ ...values, [key]: event.target.value })} required /><span className="number-stepper"><button type="button" onClick={() => adjustValue(key, 1)} aria-label={`Increase ${label}`}><ChevronUp size={12} /></button><button type="button" onClick={() => adjustValue(key, -1)} aria-label={`Decrease ${label}`}><ChevronDown size={12} /></button></span></span></label>)}</div>
    <div className="net-worth-total-preview"><span>Calculated net worth</span><strong>{formatMoney(total)}</strong></div>
    {error && <p className="form-error">{error}</p>}<button className="primary-button submit" disabled={saving}>{saving ? "Saving…" : entry ? "Save changes" : "Add snapshot"}</button>
  </form></div></div>;
}

function calculateAnalytics(entries: NetWorthEntry[]) {
  const latest = entries.at(-1)!;
  const previous = entries.at(-2);
  const first = entries[0];
  const latestChange = previous ? latest.netWorth - previous.netWorth : 0;
  const changes = entries.slice(1).map((entry, index) => ({ date: entry.date, amount: entry.netWorth - entries[index].netWorth }));
  const high = entries.reduce((best, entry) => entry.netWorth > best.netWorth ? entry : best, first);
  const elapsedDays = Math.max((dateValue(latest.date) - dateValue(first.date)) / day, 0);
  const elapsedMonths = elapsedDays / (365.2425 / 12);
  const totalGrowth = latest.netWorth - first.netWorth;
  const monthlyGrowthRate = first.netWorth > 0 && elapsedMonths > 0 ? (Math.pow(latest.netWorth / first.netWorth, 1 / elapsedMonths) - 1) * 100 : 0;
  const best = changes.reduce((value, change) => change.amount > value.amount ? change : value, { date: "", amount: 0 });
  const worst = changes.reduce((value, change) => change.amount < value.amount ? change : value, { date: "", amount: 0 });
  const latestDirection = (changes.at(-1)?.amount ?? 0) >= 0 ? 1 : -1;
  let streak = 0;
  for (const change of [...changes].reverse()) {
    if ((change.amount >= 0 ? 1 : -1) !== latestDirection) break;
    streak += 1;
  }
  const allocation = categories.map((category) => ({ ...category, value: latest[category.key], percent: latest.netWorth > 0 ? latest[category.key] / latest.netWorth * 100 : 0 })).sort((left, right) => right.value - left.value);
  return {
    latestChange,
    latestChangePercent: previous?.netWorth ? latestChange / previous.netWorth * 100 : 0,
    totalGrowth,
    totalGrowthPercent: first.netWorth ? totalGrowth / first.netWorth * 100 : 0,
    monthlyGrowthRate,
    positiveChanges: changes.filter((change) => change.amount > 0).length,
    changeCount: changes.length,
    averageChange: changes.length ? changes.reduce((sum, change) => sum + change.amount, 0) / changes.length : 0,
    best,
    worst,
    high,
    elapsedDays,
    allocation,
    streak,
    streakDirection: latestDirection > 0 ? "gains" : "declines",
  };
}
