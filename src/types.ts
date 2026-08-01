export type DataPoint = {
  timestamp: string;
  value: number;
  invested: number;
};

export type SourceSummary = {
  id: string;
  name: string;
  kind: "brokerage" | "crypto";
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
  address: string;
  label: string;
  balance: number;
  symbol: string;
  value: number;
  message: string | null;
};

export type CryptoPortfolio = {
  id: number;
  name: string;
  value: number;
  returnValue: number;
  wallets: Wallet[];
};

export type Dashboard = {
  totalValue: number;
  investedValue: number;
  cashValue: number;
  totalReturn: number;
  returnPercent: number;
  currency: string;
  updatedAt: string;
  history: DataPoint[];
  sources: SourceSummary[];
  holdings: Holding[];
  portfolios: CryptoPortfolio[];
  notices: string[];
};

export type AddWalletInput = {
  portfolioId: number;
  network: Wallet["network"];
  address: string;
  label: string;
};
