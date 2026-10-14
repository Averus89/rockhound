import { Group, Stack, Text } from "@mantine/core";

import type { Reading } from "./api";
import { formatSignature } from "./format";

export function History({ readings }: { readings: Reading[] }) {
  if (readings.length === 0) return null;
  return (
    <Stack gap={4}>
      <Text size="xs" tt="uppercase" c="dimmed" fw={700}>
        Recent
      </Text>
      {readings.map((r) => (
        <Group key={r.at} justify="space-between" wrap="nowrap">
          <Text ff="monospace">{formatSignature(r.value ?? 0)}</Text>
          <Text size="sm" truncate>
            {r.matches[0] ? `${r.matches[0].name}${r.matches[0].count > 1 ? ` ×${r.matches[0].count}` : ""}` : "Unknown"}
          </Text>
          <Text size="xs" c="dimmed">
            {new Date(r.at).toLocaleTimeString()}
          </Text>
        </Group>
      ))}
    </Stack>
  );
}
