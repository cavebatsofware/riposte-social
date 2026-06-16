import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { I18nextProvider } from "react-i18next";
import { ThemeProvider } from "@cavebatsofware/riposte-design-system/theme";
import App from "./App";
import themeI18n from "./themeI18n";
// Design-system tokens + component styles first, then the pickers' layout
// (ThemePicker/LanguagePicker popovers, which consume those tokens), then this
// app's reset + remaining feature styles.
import "@cavebatsofware/riposte-design-system/styles";
import "@cavebatsofware/riposte-pickers/styles";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <I18nextProvider i18n={themeI18n}>
      <ThemeProvider>
        <BrowserRouter basename="/admin">
          <App />
        </BrowserRouter>
      </ThemeProvider>
    </I18nextProvider>
  </React.StrictMode>,
);
