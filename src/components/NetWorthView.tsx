import { FormEvent, useMemo, useState } from "react";
import { ArrowDownRight, ArrowUpRight, CircleAlert, Pencil, Plus, Trash2, Trophy, X } from "lucide-react";
import { api } from "../api";
import type { NetWorthEntry, SaveNetWorthInput } from "../types";

type CategoryKey = "stocks" | "opessocius" | "crypto" | "savings" | "spending" | "receivables" | "cash" | "misc";

const categories: { key: CategoryKey; label: string }[] = [
  { key: "stocks", label: "Stocks" },
  { key: "opessocius", label: "Opessocius" },
  { key: "crypto", label: "Crypto" },
  { key: "savings", label: "Savings" },
  { key: "spending", label: "Spending" },
  { key: "receivables", label: "Receivables" },
  { key: "cash", label: "Cash" },
  { key: "misc", label: "Misc" },
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
  const movingAveragePoints = entries.flatMap((_, index) => {
    if (index < 2) return [];
    const average = entries.slice(index - 2, index + 1).reduce((sum, entry) => sum + entry.netWorth, 0) / 3;
    return [{ x: ((index + 0.5) / entries.length) * 1000, y: 100 - (average / analytics.high.netWorth) * 100 }];
  });

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
    <div className="net-worth-actions"><button className="primary-button" onClick={() => setEditing("new")}><Plus size={16} /> Add snapshot</button></div>

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
          {movingAveragePoints.length > 1 && <svg className="net-worth-moving-average" viewBox="0 0 1000 100" preserveAspectRatio="none" role="img" aria-label="Three-entry moving average of net worth"><polyline points={movingAveragePoints.map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" ")} /></svg>}
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
      <div className="net-worth-chart-legend"><span>Bars start at zero</span><span className="moving-average-legend"><i />3-entry moving average</span><span>{entries.length} snapshots · {Math.round(analytics.elapsedDays)} days</span></div>
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
    <p className="tracking-disclaimer">Growth includes deposits, withdrawals, and reclassification between categories; it is a balance-sheet progression, not investment return. MoM columns compare each snapshot with the prior recorded date, including multiple entries within one month.</p>

    <section className="panel net-worth-history-panel">
      <div className="panel-heading"><div><p className="section-label">Ledger</p><h2>Snapshot history</h2></div><button className="secondary-button" onClick={() => setEditing("new")}><Plus size={15} /> Add entry</button></div>
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

function NetWorthModal({ entry, latest, formatMoney, onClose, onSaved }: { entry?: NetWorthEntry; latest?: NetWorthEntry; formatMoney: (value: number) => string; onClose: () => void; onSaved: () => Promise<void> }) {
  const starting = entry ?? latest;
  const [date, setDate] = useState(entry?.date ?? new Date().toISOString().slice(0, 10));
  const [values, setValues] = useState<Record<CategoryKey, string>>(() => Object.fromEntries(categories.map(({ key }) => [key, String(starting?.[key] ?? 0)])) as Record<CategoryKey, string>);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const total = categories.reduce((sum, { key }) => sum + (Number(values[key]) || 0), 0);

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
    <div className="net-worth-fields">{categories.map(({ key, label }) => <label key={key}>{label} (€)<input type="number" min="0" step="0.01" inputMode="decimal" value={values[key]} onChange={(event) => setValues({ ...values, [key]: event.target.value })} required /></label>)}</div>
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
