import { Badge, Card, Group, Stack, Text, Title } from "@mantine/core";

import type { Reading } from "./api";
import { CATEGORY_LABEL, formatSignature, TIER_COLOR } from "./format";

export function Readout({ reading, scanning }: { reading: Reading | null; scanning: boolean }) {
  if (!reading || reading.value === null) {
    return (
      <Card withBorder padding="xl">
        <Text c="dimmed" ta="center">
          {scanning ? "Looking for a signature…" : "Press Start, then ping in game."}
        </Text>
      </Card>
    );
  }

  const [best, ...others] = reading.matches;
  return (
    <Card withBorder padding="lg">
      <Stack gap="xs">
        <Group justify="space-between" align="baseline">
          <Title order={1} ff="monospace">
            {formatSignature(reading.value)}
          </Title>
          {reading.corrected && (
            <Badge variant="light" color="yellow" title={`Read as ${reading.raw}`}>
              corrected
            </Badge>
          )}
        </Group>
        {best ? (
          <Group gap="xs">
            <Text size="xl" fw={600}>
              {best.name}
              {best.count > 1 && ` ×${best.count}`}
            </Text>
            {best.tier && (
              <Badge color={TIER_COLOR[best.tier] ?? "gray"}>{best.tier}</Badge>
            )}
            <Text size="sm" c="dimmed">
              {CATEGORY_LABEL[best.category]}
            </Text>
          </Group>
        ) : (
          <Text c="dimmed">Unknown signature</Text>
        )}
        {others.length > 0 && (
          <Text size="sm" c="dimmed">
            Could also be:{" "}
            {others.map((m) => `${m.name}${m.count > 1 ? ` ×${m.count}` : ""}`).join(", ")}
          </Text>
        )}
      </Stack>
    </Card>
  );
}
