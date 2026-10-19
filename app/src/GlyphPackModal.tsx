import { ActionIcon, Alert, Button, Group, Modal, NumberInput, Stack, Text } from "@mantine/core";
import { IconTrash } from "@tabler/icons-react";
import { useEffect, useState } from "react";

import { deleteGlyphSample, exportGlyphPack, type GlyphSample, listGlyphSamples } from "./api";

interface Row extends GlyphSample {
  url: string;
}

interface Props {
  opened: boolean;
  onClose: () => void;
}

/** Lets the player check each sample's number before sending the pack, so
 * a misread (likely on a HUD size the templates don't know yet) is fixed
 * rather than taught. */
export function GlyphPackModal({ opened, onClose }: Props) {
  const [rows, setRows] = useState<Row[]>([]);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!opened) return;
    setSaved(null);
    setError(null);
    let urls: string[] = [];
    listGlyphSamples()
      .then((samples) => {
        const loaded = samples.map((s) => ({
          ...s,
          url: URL.createObjectURL(new Blob([new Uint8Array(s.png)], { type: "image/png" })),
        }));
        urls = loaded.map((r) => r.url);
        setRows(loaded);
      })
      .catch((e) => setError(String(e)));
    return () => urls.forEach((u) => URL.revokeObjectURL(u));
  }, [opened]);

  const setValue = (file: string, value: number) =>
    setRows((rs) => rs.map((r) => (r.file === file ? { ...r, value } : r)));

  const remove = (file: string) =>
    deleteGlyphSample(file)
      .then(() => setRows((rs) => rs.filter((r) => r.file !== file)))
      .catch((e) => setError(String(e)));

  const send = () =>
    exportGlyphPack(rows.map(({ file, value }) => ({ file, value })))
      .then((path) => {
        setSaved(path);
        setRows([]);
      })
      .catch((e) => setError(String(e)));

  return (
    <Modal opened={opened} onClose={onClose} title="Digit samples" centered size="md">
      <Stack>
        {saved ? (
          <Alert color="teal" variant="light">
            Saved {saved}. Attach it to the GitHub issue that just opened (or open a new "Glyph
            pack" issue on github.com/Averus89/rockhound). Thank you!
          </Alert>
        ) : (
          <Text size="sm" c="dimmed">
            Check each number against its picture. Fix any that are wrong and remove any you can't
            read, then send.
          </Text>
        )}
        {!saved && rows.length === 0 && !error && (
          <Text size="sm">
            No samples yet. Turn on "Collect digit samples" in Settings and scan a few rocks.
          </Text>
        )}
        <Stack gap="xs" mah={360} style={{ overflowY: "auto" }}>
          {rows.map((r) => (
            <Group key={r.file} gap="sm" wrap="nowrap">
              <img src={r.url} alt={`Sample read as ${r.value}`} style={{ height: 32 }} />
              <NumberInput
                aria-label="Number shown"
                size="xs"
                min={1}
                hideControls
                value={r.value}
                onChange={(v) => setValue(r.file, Number(v) || 0)}
                style={{ flex: 1 }}
              />
              <ActionIcon
                variant="subtle"
                color="gray"
                aria-label="Remove sample"
                onClick={() => remove(r.file)}
              >
                <IconTrash size={16} />
              </ActionIcon>
            </Group>
          ))}
        </Stack>
        {error && (
          <Text c="red" size="sm">
            {error}
          </Text>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>
            Close
          </Button>
          <Button onClick={send} disabled={rows.length === 0 || rows.some((r) => r.value < 1)}>
            Send {rows.length} {rows.length === 1 ? "sample" : "samples"}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
