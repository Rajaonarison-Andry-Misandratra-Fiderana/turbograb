// Bundled, not fetched: the app must render correctly with no network.
// One ~48 KB latin file carries the whole 100–900 weight axis.
import "@fontsource-variable/inter";

import React from "react";
import ReactDOM from "react-dom/client";
import CssBaseline from "@mui/material/CssBaseline";
import { ThemeProvider } from "@mui/material/styles";
import App from "./App";
import { theme } from "./theme";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider
      theme={theme}
      defaultMode="system"
      modeStorageKey="turbograb.mode"
      colorSchemeStorageKey="turbograb.color-scheme"
      disableTransitionOnChange
    >
      {/* enableColorScheme so the webview's own scrollbars and form controls
          follow the scheme too, not just our components. */}
      <CssBaseline enableColorScheme />
      <App />
    </ThemeProvider>
  </React.StrictMode>,
);
