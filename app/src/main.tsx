import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import { App } from "./shell/App";
import "./shell/styles.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("index.html is missing the #root element");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
