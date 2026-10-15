import "@mantine/core/styles.css";

import { createTheme, MantineProvider } from "@mantine/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { getCurrentWindow } from "@tauri-apps/api/window";

import { App } from "./App";
import { Overlay } from "./Overlay";

const theme = createTheme({
  primaryColor: "orange",
  fontFamily: "Inter, system-ui, sans-serif",
  fontFamilyMonospace: "'JetBrains Mono', ui-monospace, monospace",
  defaultRadius: "md",
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MantineProvider theme={theme} forceColorScheme="dark">
      {getCurrentWindow().label === "overlay" ? <Overlay /> : <App />}
    </MantineProvider>
  </StrictMode>,
);
