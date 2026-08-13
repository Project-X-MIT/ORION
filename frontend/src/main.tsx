import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { AppProviders } from "./providers/AppProviders";
import "./shared/styles/global.css";

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("Missing application root element");

createRoot(rootElement).render(
  <StrictMode>
    <AppProviders>
      <App />
    </AppProviders>
  </StrictMode>,
);
