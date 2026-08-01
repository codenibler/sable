import { invoke } from "@tauri-apps/api/core";
import type { AddWalletInput, CryptoPortfolio, Dashboard } from "./types";

export const api = {
  dashboard: () => invoke<Dashboard>("get_dashboard"),
  setOpessociusPreviousMonthWinnings: (amount: number) =>
    invoke<void>("set_opessocius_previous_month_winnings", { amount }),
  portfolios: () => invoke<CryptoPortfolio[]>("list_crypto_portfolios"),
  addWallet: (input: AddWalletInput) => invoke<number>("add_wallet", { input }),
  removeWallet: (id: number) => invoke<void>("remove_wallet", { id }),
};
