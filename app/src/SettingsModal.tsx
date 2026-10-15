import {
  Button,
  Group,
  Modal,
  NumberInput,
  SimpleGrid,
  Stack,
  Switch,
  Text,
} from "@mantine/core";
import { useEffect, useState } from "react";

import type { Settings } from "./api";

const REGION_FIELDS = ["X", "Y", "Width", "Height"] as const;

interface Props {
  opened: boolean;
  settings: Settings;
  onClose: () => void;
  onSave: (settings: Settings) => Promise<void>;
}

export function SettingsModal({ opened, settings, onClose, onSave }: Props) {
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setDraft(settings);
    setError(null);
  }, [settings, opened]);

  const region = draft.region ?? [0, 0, 0, 0];
  const setRegion = (i: number, v: number) => {
    const next = [...region] as [number, number, number, number];
    next[i] = v;
    setDraft({ ...draft, region: next });
  };

  const save = async () => {
    try {
      await onSave(draft);
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Settings" centered>
      <Stack>
        <Switch
          label="Find the readout automatically"
          description="Follows the orange map pin. Turn off to set a fixed box."
          checked={draft.region === null}
          onChange={(e) =>
            setDraft({
              ...draft,
              region: e.currentTarget.checked
                ? null
                : (settings.region ?? [0, 0, 0, 0]),
            })
          }
        />
        <Text size="sm" c="dimmed">
          Fixed readout box in game-window pixels, just right of the pin.
        </Text>
        <SimpleGrid cols={2}>
          {REGION_FIELDS.map((label, i) => (
            <NumberInput
              key={label}
              label={label}
              min={0}
              disabled={draft.region === null}
              value={region[i]}
              onChange={(v) => setRegion(i, Number(v) || 0)}
            />
          ))}
        </SimpleGrid>
        <Switch
          label="Show overlay over the game"
          description="A small click-through readout at the top of the screen."
          checked={draft.overlay}
          onChange={(e) => setDraft({ ...draft, overlay: e.currentTarget.checked })}
        />
        <NumberInput
          label="Read every (ms)"
          min={50}
          max={2000}
          step={50}
          value={draft.intervalMs}
          onChange={(v) => setDraft({ ...draft, intervalMs: Number(v) || 250 })}
        />
        {error && (
          <Text c="red" size="sm">
            {error}
          </Text>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={save}>Save</Button>
        </Group>
      </Stack>
    </Modal>
  );
}
