import React, { Suspense } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { AuthProvider } from "./contexts/AuthContext";
import { SiteConfigProvider } from "./contexts/SiteConfigContext";
import { ThemeProvider, type ThemeShade } from "@cavebatsofware/riposte-design-system/theme";
// Side-effect import: configures the i18next singleton before the app
// mounts. Must run before any component calls `useTranslation`.
import "./i18n";
// Design-system tokens + component styles, then the pickers' layout (which
// consumes those tokens), then this app's reset + feature styles.
import "@cavebatsofware/riposte-design-system/styles";
import "@cavebatsofware/riposte-pickers/styles";
import "./index.css";

// Per-site theme defaults the backend injected into index.html, exposed as
// window globals by the no-flash script. Empty -> design-system fallback.
const themeGlobals = window as {
  __RS_DEFAULT_COLORWAY__?: string;
  __RS_DEFAULT_SHADE__?: string;
};
const defaultColorway = themeGlobals.__RS_DEFAULT_COLORWAY__ || undefined;
const defaultShade: ThemeShade | undefined =
  themeGlobals.__RS_DEFAULT_SHADE__ === "light" || themeGlobals.__RS_DEFAULT_SHADE__ === "dark"
    ? themeGlobals.__RS_DEFAULT_SHADE__
    : undefined;

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <ThemeProvider defaultColorway={defaultColorway} defaultShade={defaultShade}>
      <BrowserRouter>
        <AuthProvider>
          <SiteConfigProvider>
            {/* Suspense catches the brief async window while the active
                language's `common` catalog loads. Fallback is null so we
                don't flash a spinner on a normal page-load  the catalogs
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
