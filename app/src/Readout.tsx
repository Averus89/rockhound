import { Badge, Box, Group, Paper, Stack, Text } from "@mantine/core";
import { IconRadar } from "@tabler/icons-react";

import type { Reading } from "./api";
import { CategoryIcon } from "./CategoryIcon";
import {
  CATEGORY_LABEL,
  SHOWN_MIXES,
  formatSignature,
  matchLabel,
  mixLabel,
  tierColor,
  unmatchedLabel,
} from "./format";

export function Readout({ reading, scanning }: { reading: Reading | null; scanning: boolean }) {
  if (!reading || reading.value === null) {
    return (
      <Paper className="rh-panel" p="xl" radius="lg">
        <Stack align="center" gap="xs">
          <IconRadar
            size={40}
            stroke={1.3}
            className={scanning ? "rh-pulse" : undefined}
            color="var(--mantine-color-ember-4)"
            aria-hidden
          />
          <Text c="dimmed" ta="center">
            {scanning ? "Looking for a signature…" : "Press Start, then ping in game."}
          </Text>
        </Stack>
      </Paper>
    );
  }

  const [best, ...others] = reading.matches;
  const color = tierColor(best);
  return (
    <Paper
      className="rh-panel rh-arrive"
      key={reading.at}
      p="lg"
      radius="lg"
      style={{ borderLeft: `3px solid var(--mantine-color-${color}-5)` }}
    >
      <Stack gap="sm">
        <Group justify="space-between" align="center">
          <Text size="xs" tt="uppercase" fw={700} c="dimmed" lts={1.2}>
            Signature
          </Text>
          {reading.corrected && (
            <Badge variant="light" color="yellow" size="sm" title={`Read as ${reading.raw}`}>
              corrected
            </Badge>
          )}
        </Group>
        <Text className="rh-value" ff="monospace" fz={52} fw={700} lh={1} c="ember.4">
          {formatSignature(reading.value)}
        </Text>
        {best ? (
          <Group gap="sm" wrap="nowrap">
            <Text fz="xl" fw={650} truncate>
              {matchLabel(best)}
            </Text>
            {best.tier && (
              <Badge variant="light" color={color} size="sm">
                {best.tier}
              </Badge>
            )}
          </Group>
        ) : (
          <Text c="dimmed">{unmatchedLabel(reading)}</Text>
        )}
        {reading.mixes.length > 0 && (
          <Stack gap={2}>
            {reading.mixes.slice(0, SHOWN_MIXES).map((pair) => (
              <Text key={mixLabel(pair)} size="sm">
                {mixLabel(pair)}
              </Text>
            ))}
            {reading.mixes.length > SHOWN_MIXES && (
              <Text size="xs" c="dimmed">
                and {reading.mixes.length - SHOWN_MIXES} other mixes
              </Text>
            )}
          </Stack>
        )}
        {best && (
          <Group gap={6} c="dimmed">
            <CategoryIcon category={best.category} />
            <Text size="sm">{CATEGORY_LABEL[best.category]}</Text>
          </Group>
        )}
        {others.length > 0 && (
          <Box>
            <Text size="xs" c="dimmed" mb={4}>
              Could also be: {others.map(matchLabel).join(", ")}
            </Text>
          </Box>
        )}
      </Stack>
    </Paper>
  );
}
