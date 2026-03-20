import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import PanelHost from "./components/PanelHost";

const params = new URLSearchParams(window.location.search);
const panelId = params.get('panel');

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {panelId ? <PanelHost panelId={panelId} /> : <App />}
  </React.StrictMode>,
);
