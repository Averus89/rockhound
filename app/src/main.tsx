import "@mantine/core/styles.css";
import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "./styles.css";

import { MantineProvider } from "@mantine/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { getCurrentWindow } from "@tauri-apps/api/window";

import { App } from "./App";
import { Overlay } from "./Overlay";
import { theme } from "./theme";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MantineProvider theme={theme} forceColorScheme="dark">
      {getCurrentWindow().label === "overlay" ? <Overlay /> : <App />}
    </MantineProvider>
  </StrictMode>,
);
