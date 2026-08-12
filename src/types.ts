export type DataPoint = {
  timestamp: string;
  value: number;
  invested: number;
};

export type SourceSummary = {
  id: string;
  name: string;
  kind: "brokerage" | "crypto" | "manual";
  value: number;
  returnValue: number;
  connected: boolean;
  message: string | null;
};

export type Holding = {
  id: string;
  symbol: string;
  name: string;
  source: string;
  quantity: number;
  price: number;
  value: number;
  returnValue: number;
  allocation: number;
};

export type Wallet = {
  id: number;
  portfolioId: number;
  network: "btc" | "eth" | "sol";
  displayAddress: string;
  label: string;
  walletType: "address" | "xpub" | "everstake";
  addressCount: number;
  balance: number;
  symbol: string;
  value: number;
  assets: WalletAsset[];
  message: string | null;
};

export type WalletAsset = {
  id: string;
  network: Wallet["network"];
  symbol: string;
  name: string;
  balance: number;
  price: number;
  value: number;
  message: string | null;
};

export type CryptoPortfolio = {
  id: number;
  name: string;
  value: number;
  returnValue: number;
  assets: CryptoAsset[];
  wallets: Wallet[];
};

export type CryptoAsset = {
  id: string;
  network: Wallet["network"];
  symbol: string;
  name: string;
  balance: number;
  value: number;
  investedValue: number;
  totalReturn: number;
  returnPercent: number;
  allocation: number;
  walletCount: number;
  history: DataPoint[];
};

export type Dashboard = {
  totalValue: number;
  investedValue: number;
  cashValue: number;
  totalReturn: number;
  returnPercent: number;
  monthlyReturn: PeriodReturn;
  yearlyReturn: PeriodReturn;
  opessociusMonthlyReturn: MonthlyWinnings | null;
  netContributions: number;
  historyEventCount: number;
  historyBackfillComplete: boolean;
  refreshIntervalMinutes: number;
  currency: string;
  updatedAt: string;
  history: DataPoint[];
  sources: SourceSummary[];
  holdings: Holding[];
  portfolios: CryptoPortfolio[];
  monitoredPortfolios: MonitoredPortfolio[];
  notices: string[];
};

export type MonitoredPortfolio = {
  id: string;
  name: string;
  kind: SourceSummary["kind"];
  value: number;
  investedValue: number;
  totalReturn: number;
  returnPercent: number;
  history: DataPoint[];
  periods: PortfolioPeriod[];
  itemCount: number;
  itemLabel: string;
  connected: boolean;
  message: string | null;
};

export type PortfolioPeriod = {
  month: string;
  label: string;
  returnPercent: number;
  returnValue: number;
  deposits: number;
  withdrawals: number;
  endingValue: number;
};

export type MonthlyWinnings = {
  month: string;
  label: string;
  amount: number;
  isOverride: boolean;
  defaultRatePercent: number;
};

export type PeriodReturn = {
  amount: number;
  percent: number;
};

export type AddWalletInput = {
  portfolioId: number;
  network: Wallet["network"] | "btc-xpub";
  address: string;
  label: string;
};

export type NetWorthEntry = {
  date: string;
  netWorth: number;
  trading212: number;
  opessocius: number;
  okx: number;
  trezor: number;
  bunq: number;
  t212Spending: number;
  ing: number;
  jointAccount: number;
  receivables: number;
  cash: number;
  misc: number;
  /** Retired categories, kept so snapshots recorded before the bank split stay accurate. */
  savings: number;
  spending: number;
};

export type SaveNetWorthInput = Omit<NetWorthEntry, "netWorth">;
