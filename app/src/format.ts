import type { Category } from "./api";

export const formatSignature = (value: number) => value.toLocaleString("en-US");

export const CATEGORY_LABEL: Record<Category, string> = {
  ship_mining: "Ship mining",
  ground_deposit: "Ground deposit",
  salvage: "Salvage",
};

// Tier colours follow the usual rarity convention so they read at a glance.
export const TIER_COLOR: Record<string, string> = {
  legendary: "yellow",
  epic: "grape",
  rare: "blue",
  uncommon: "teal",
  common: "gray",
};
