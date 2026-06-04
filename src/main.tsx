import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { LocaleProvider } from "./i18n";
import { DisplayPreferenceProvider } from "./preferences/display";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LocaleProvider>
      <DisplayPreferenceProvider>
        <App />
      </DisplayPreferenceProvider>
    </LocaleProvider>
  </React.StrictMode>,
);
