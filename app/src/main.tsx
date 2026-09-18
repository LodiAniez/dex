import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import { App } from "./shell/App";
import { PopoutView, popoutOfThisWindow } from "./shell/PopoutView";
import "./shell/styles.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("index.html is missing the #root element");
}

// A pane in a window of its own is told which, before the page loads (src-tauri/src/popout.rs).
const popout = popoutOfThisWindow();

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {popout ? <PopoutView paneId={popout.paneId} title={popout.title} /> : <App />}
  </React.StrictMode>,
);
