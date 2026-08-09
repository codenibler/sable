import { createRoot } from "react-dom/client";
import App from "./App";
import { isDesktop } from "./platform";
import "./styles.css";

createRoot(document.getElementById("root")!).render(<App />);

// Desktop already serves its frontend from the Tauri binary, so a worker there would only
// add a second, staler copy of the same assets.
if (!isDesktop && "serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    void navigator.serviceWorker.register("/sw.js").catch(() => {
      // The app works without it; only the offline shell is lost.
    });
  });
}
