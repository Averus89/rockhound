import { Badge, Group, Stack, Text } from "@mantine/core";
import { useEffect, useState } from "react";

import { onReading, type Reading } from "./api";
import { formatSignature, matchLabel, tierColor, unmatchedLabel } from "./format";

/** Compact readout for the click-through overlay window. */
export function OverlayCard({ reading }: { reading: Reading | null }) {
  // Nothing on screen means the game view stays clear.
  if (!reading || reading.value === null) return null;
  const best = reading.matches[0];
  const color = tierColor(best);
  return (
    <Group
      key={reading.at}
      className="rh-overlay-card rh-arrive"
      gap="md"
      wrap="nowrap"
      px="md"
      py={8}
      w="fit-content"
      style={{ borderRadius: 10, borderLeftColor: `var(--mantine-color-${color}-5)` }}
    >
      <Text className="rh-value" ff="monospace" fz={30} fw={700} lh={1} c="ember.4">
        {formatSignature(reading.value)}
      </Text>
      <Stack gap={2}>
        <Text fw={650} lh={1.2} lineClamp={1}>
          {best ? matchLabel(best) : unmatchedLabel(reading)}
        </Text>
        {best?.tier && (
          <Badge size="xs" variant="light" color={color}>
            {best.tier}
          </Badge>
        )}
      </Stack>
    </Group>
  );
}

export function Overlay() {
  const [reading, setReading] = useState<Reading | null>(null);
  useEffect(() => {
    // Let the game show through everywhere the card isn't.
    document.documentElement.style.setProperty("--mantine-color-body", "transparent");
    document.body.classList.add("rh-overlay-root");
    const unlisten = onReading(setReading);
    return () => {
      unlisten.then((off) => off());
    };
  }, []);
  return <OverlayCard reading={reading} />;
}
