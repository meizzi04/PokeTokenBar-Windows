import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Pet from "./Pet";

const isPet = location.hash === "#pet";
if (isPet) document.documentElement.classList.add("pet-window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isPet ? <Pet /> : <App />}</React.StrictMode>,
);
