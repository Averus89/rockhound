import { createTheme, type MantineColorsTuple } from "@mantine/core";

// The orange of the in-game map pin, so the app reads as part of the HUD.
const ember: MantineColorsTuple = [
  "#fff3e6",
  "#ffe3c7",
  "#ffc48c",
  "#ffa24d",
  "#ff8a1f",
  "#ff7a05",
  "#e86a00",
  "#c25800",
  "#9c4700",
  "#7a3700",
];

export const theme = createTheme({
  primaryColor: "ember",
  primaryShade: 4,
  colors: { ember },
  fontFamily: "'Inter Variable', system-ui, sans-serif",
  fontFamilyMonospace: "'JetBrains Mono', ui-monospace, monospace",
  headings: { fontFamily: "'Inter Variable', system-ui, sans-serif", fontWeight: "650" },
  defaultRadius: "md",
});
