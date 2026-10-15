import { Badge, Group, Paper, Text } from "@mantine/core";
import { useEffect, useState } from "react";

import { onReading, type Reading } from "./api";
import { formatSignature, TIER_COLOR } from "./format";

/** Compact readout for the click-through overlay window. */
export function OverlayCard({ reading }: { reading: Reading | null }) {
  // Nothing on screen means the game view stays clear.
  if (!reading || reading.value === null) return null;
  const best = reading.matches[0];
  return (
    <Paper px="md" py="xs" radius="md" w="fit-content" bg="rgba(16, 17, 20, 0.78)">
      <Group gap="sm" wrap="nowrap">
        <Text fz={28} fw={700} ff="monospace" c="orange.4">
          {formatSignature(reading.value)}
        </Text>
        <div>
          <Text fw={600} lineClamp={1}>
            {best ? `${best.name}${best.count > 1 ? ` ×${best.count}` : ""}` : "Unknown"}
          </Text>
          {best?.tier && (
            <Badge size="xs" color={TIER_COLOR[best.tier] ?? "gray"}>
              {best.tier}
            </Badge>
          )}
        </div>
      </Group>
    </Paper>
  );
}

export function Overlay() {
  const [reading, setReading] = useState<Reading | null>(null);
  useEffect(() => {
    // Let the game show through everywhere the card isn't.
    document.documentElement.style.setProperty("--mantine-color-body", "transparent");
    const unlisten = onReading(setReading);
    return () => {
      unlisten.then((off) => off());
    };
  }, []);
  return <OverlayCard reading={reading} />;
}
