import { invoke } from "@tauri-apps/api/core";
import type { AddWalletInput, CryptoPortfolio, Dashboard } from "./types";

export const api = {
  dashboard: () => invoke<Dashboard>("get_dashboard"),
  portfolios: () => invoke<CryptoPortfolio[]>("list_crypto_portfolios"),
  addWallet: (input: AddWalletInput) => invoke<number>("add_wallet", { input }),
  removeWallet: (id: number) => invoke<void>("remove_wallet", { id }),
};
