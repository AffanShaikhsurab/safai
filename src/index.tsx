import { render } from "solid-js/web";
import App from "./App";
import "./index.css";

// Dark is the default theme; ensure the class is present even if index.html
// was served without it.
document.documentElement.classList.add("dark");

const root = document.getElementById("root");
if (!root) throw new Error("Root element #root not found");

render(() => <App />, root);
