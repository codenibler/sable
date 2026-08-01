import { invoke } from "@tauri-apps/api/core";
import type { AddWalletInput, CryptoPortfolio, Dashboard } from "./types";

export const api = {
  dashboard: () => invoke<Dashboard>("get_dashboard"),
  portfolios: () => invoke<CryptoPortfolio[]>("list_crypto_portfolios"),
  createPortfolio: (name: string) => invoke<number>("create_crypto_portfolio", { name }),
  deletePortfolio: (id: number) => invoke<void>("delete_crypto_portfolio", { id }),
  addWallet: (input: AddWalletInput) => invoke<number>("add_wallet", { input }),
  removeWallet: (id: number) => invoke<void>("remove_wallet", { id }),
};
