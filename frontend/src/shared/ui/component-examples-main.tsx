import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "../styles/global.css";
import { ComponentExamples } from "./ComponentExamples";

const root = document.getElementById("root");
if (!root) throw new Error("Missing component example root element");

createRoot(root).render(<StrictMode><ComponentExamples /></StrictMode>);
