import { IconPick, IconRecycle, IconShovel } from "@tabler/icons-react";

import type { Category } from "./api";

const ICONS = { ship_mining: IconPick, ground_deposit: IconShovel, salvage: IconRecycle };

export function CategoryIcon({ category, size = 16 }: { category: Category; size?: number }) {
  const Icon = ICONS[category];
  return <Icon size={size} stroke={1.6} aria-hidden />;
}
