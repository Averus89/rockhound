import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Container,
  Group,
  SegmentedControl,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import {
  IconAlertTriangle,
  IconMapPinFilled,
  IconPlayerPlayFilled,
  IconPlayerStopFilled,
  IconSettings,
} from "@tabler/icons-react";
import { useEffect, useState } from "react";

import {
  getStatus,
  type Location,
  onReading,
  onScannerStopped,
  onSignaturesUpdated,
  type Reading,
  saveSettings,
  type Settings,
  startScan,
  type Status,
  stopScan,
} from "./api";
import { History } from "./History";
import { Readout } from "./Readout";
import { SettingsModal } from "./SettingsModal";

const HISTORY_SIZE = 8;

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [reading, setReading] = useState<Reading | null>(null);
  const [history, setHistory] = useState<Reading[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [settingsOpen, settings] = useDisclosure(false);

  const refresh = () =>
    getStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)));

  useEffect(() => {
    refresh();
    const unlisten = [
      onReading((r) => {
        setReading(r);
        if (r.value === null) return;
        // A location change resends the shown reading re-ranked.
        setHistory((h) =>
          h[0]?.at === r.at ? [r, ...h.slice(1)] : [r, ...h].slice(0, HISTORY_SIZE),
        );
      }),
      onScannerStopped((e) => {
        if (e) setError(e);
        refresh();
      }),
      onSignaturesUpdated(() => refresh()),
    ];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, []);

  const toggle = async () => {
    setError(null);
    try {
      await (status?.scanning ? stopScan() : startScan());
    } catch (e) {
      setError(String(e));
    }
    refresh();
  };

  const save = async (next: Settings) => {
    await saveSettings(next);
    await refresh();
  };

  const setLocation = (location: Location) => {
    if (!status) return;
    // Saved even when showing the overlay fails, so refresh either way.
    saveSettings({ ...status.settings, location })
      .catch((e) => setError(String(e)))
      .finally(refresh);
  };

  return (
    <Container size="xs" py="md">
      <Stack>
        <Group justify="space-between">
          <Group gap={8}>
            <IconMapPinFilled size={26} color="var(--mantine-color-ember-4)" aria-hidden />
            <Title order={3} lts={0.3}>
              Rockhound
            </Title>
            <Badge
              variant="dot"
              color={status?.scanning ? "teal" : "gray"}
              size="sm"
              className={status?.scanning ? "rh-pulse" : undefined}
            >
              {status?.scanning ? "Scanning" : "Idle"}
            </Badge>
          </Group>
          <Group gap="xs">
            <Button
              onClick={toggle}
              color={status?.scanning ? "red" : "ember"}
              variant={status?.scanning ? "light" : "filled"}
              disabled={!status}
              leftSection={
                status?.scanning ? (
                  <IconPlayerStopFilled size={14} />
                ) : (
                  <IconPlayerPlayFilled size={14} />
                )
              }
            >
              {status?.scanning ? "Stop" : "Start"}
            </Button>
            <ActionIcon variant="default" size="lg" onClick={settings.open} aria-label="Settings">
              <IconSettings size={18} stroke={1.6} />
            </ActionIcon>
          </Group>
        </Group>

        {error && (
          <Alert
            color="red"
            variant="light"
            icon={<IconAlertTriangle size={18} />}
            withCloseButton
            onClose={() => setError(null)}
          >
            {error}
          </Alert>
        )}

        <SegmentedControl
          fullWidth
          size="xs"
          aria-label="Where you are"
          disabled={!status}
          value={status?.settings.location ?? "unknown"}
          onChange={(v) => setLocation(v as Location)}
          data={[
            { value: "unknown", label: "Anywhere" },
            { value: "space", label: "In space" },
            { value: "planet", label: "On a planet" },
          ]}
        />

        <Readout
          reading={reading}
          scanning={status?.scanning ?? false}
          location={status?.settings.location ?? "unknown"}
        />
        <History readings={history} />

        {status && (
          <Text size="xs" c="dimmed" ta="center" opacity={0.7}>
            Signature data {status.dataVersion} · Star Citizen {status.scVersion}
          </Text>
        )}
      </Stack>

      {status && (
        <SettingsModal
          opened={settingsOpen}
          settings={status.settings}
          onClose={settings.close}
          onSave={save}
        />
      )}
    </Container>
  );
}
