import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import BloomeryApp from "./app/BloomeryApp";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BloomeryApp />
  </StrictMode>,
);
