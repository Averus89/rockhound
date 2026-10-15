import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Mantine reads the colour scheme through matchMedia, which jsdom lacks.
window.matchMedia ??= (query: string) =>
  ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }) as MediaQueryList;

afterEach(cleanup);
