import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { I18nextProvider } from "react-i18next";
import { ThemeProvider, type ThemeShade } from "@cavebatsofware/riposte-design-system/theme";
import App from "./App";
import themeI18n from "./themeI18n";
// Design-system tokens + component styles first, then the pickers' layout
// (ThemePicker/LanguagePicker popovers, which consume those tokens), then this
// app's reset + remaining feature styles.
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
    <I18nextProvider i18n={themeI18n}>
      <ThemeProvider defaultColorway={defaultColorway} defaultShade={defaultShade}>
        <BrowserRouter basename="/admin">
          <App />
        </BrowserRouter>
      </ThemeProvider>
    </I18nextProvider>
  </React.StrictMode>,
);
