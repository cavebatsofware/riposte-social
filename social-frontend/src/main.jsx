import React, { Suspense } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { AuthProvider } from "./contexts/AuthContext";
import { SiteConfigProvider } from "./contexts/SiteConfigContext";
import { ThemeProvider } from "./contexts/ThemeContext";
// Side-effect import: configures the i18next singleton before the app
// mounts. Must run before any component calls `useTranslation`.
import "./i18n";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <ThemeProvider>
      <BrowserRouter>
        <AuthProvider>
          <SiteConfigProvider>
            {/* Suspense catches the brief async window while the active
                language's `common` catalog loads. Fallback is null so we
                don't flash a spinner on a normal page-load — the catalogs
                resolve in tens of milliseconds locally. */}
            <Suspense fallback={null}>
              <App />
            </Suspense>
          </SiteConfigProvider>
        </AuthProvider>
      </BrowserRouter>
    </ThemeProvider>
  </React.StrictMode>,
);
