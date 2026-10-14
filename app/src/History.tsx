import { ColorSwatch, Group, Paper, Stack, Text } from "@mantine/core";

import type { Reading } from "./api";
import { formatSignature, matchLabel, tierColor } from "./format";

export function History({ readings }: { readings: Reading[] }) {
  if (readings.length === 0) return null;
  return (
    <Paper className="rh-panel" p="md" radius="lg">
      <Text size="xs" tt="uppercase" c="dimmed" fw={700} lts={1.2} mb="xs">
        Recent
      </Text>
      <Stack gap={6}>
        {readings.map((r) => {
          const best = r.matches[0];
          return (
            <Group key={r.at} gap="sm" wrap="nowrap">
              <ColorSwatch
                size={8}
                color={`var(--mantine-color-${tierColor(best)}-5)`}
                withShadow={false}
              />
              <Text ff="monospace" fw={500} w={80}>
                {formatSignature(r.value ?? 0)}
              </Text>
              <Text size="sm" truncate style={{ flex: 1 }}>
                {best ? matchLabel(best) : "Unknown"}
              </Text>
              <Text size="xs" c="dimmed">
                {new Date(r.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </Text>
            </Group>
          );
        })}
      </Stack>
    </Paper>
  );
}
