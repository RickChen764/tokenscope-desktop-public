import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { LocaleProvider } from "./i18n";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LocaleProvider>
      <App />
    </LocaleProvider>
  </React.StrictMode>,
);
