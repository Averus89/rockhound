import { Alert, Button, Text } from "@mantine/core";
import { IconDownload } from "@tabler/icons-react";
import { useState } from "react";

import type { AvailableUpdate } from "./api";

export const RELEASES_URL = "github.com/Averus89/rockhound/releases";

export function UpdateNotice({ update }: { update: AvailableUpdate }) {
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const install = async () => {
    setError(null);
    setInstalling(true);
    try {
      // On success the installer closes the app and starts the new version.
      await update.install();
    } catch (e) {
      setError(String(e));
      setInstalling(false);
    }
  };

  return (
    <Alert
      color="ember"
      variant="light"
      icon={<IconDownload size={18} />}
      title={`Rockhound v${update.version} is available`}
    >
      {update.installed ? (
        <Button size="xs" color="ember" loading={installing} onClick={install}>
          Update and restart
        </Button>
      ) : (
        <Text size="sm">Download it from {RELEASES_URL}.</Text>
      )}
      {error && (
        <Text size="sm" c="red" mt="xs">
          Update failed: {error}
        </Text>
      )}
    </Alert>
  );
}
