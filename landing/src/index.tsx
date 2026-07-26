/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";
import { initTheme } from "./theme";
import "./landing.css";

initTheme();

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root");

render(() => <App />, root);
