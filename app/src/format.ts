import type { Category, SignatureMatch } from "./api";

export const formatSignature = (value: number) => value.toLocaleString("en-US");

/** "Copper ×3", or just "Copper" for a single object. */
export const matchLabel = (m: SignatureMatch) => (m.count > 1 ? `${m.name} ×${m.count}` : m.name);

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

/** Mantine colour for a match's tier; untiered sources use the accent. */
export const tierColor = (m: SignatureMatch | undefined) =>
  (m?.tier && TIER_COLOR[m.tier]) || "ember";
