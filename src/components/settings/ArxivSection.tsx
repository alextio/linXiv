import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getSettings, updateEnv } from "../../api/settings";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { SettingGroup, SettingGroupLabel, SettingRow } from "./SettingRow";

export function ArxivSection() {
  const { data: settings } = useQuery({
    queryKey: ["settings"],
    queryFn: getSettings,
  });

  const [arxivEmail, setArxivEmail] = useState("");
  const [populated, setPopulated] = useState(false);
  if (settings && !populated) {
    if (settings.ARXIV_MAILTO) {
      setArxivEmail(settings.ARXIV_MAILTO);
    }
    setPopulated(true);
  }

  return (
    <div>
      <SettingGroupLabel>arXiv</SettingGroupLabel>
      <SettingGroup>
        <SettingRow
          label="Contact email"
          description={
            <>
              Sent in the <code className="text-accent">User-Agent</code> header so
              arXiv can identify this app's requests.
            </>
          }
        >
          <Input
            type="email"
            value={arxivEmail}
            onChange={(e) => setArxivEmail(e.target.value)}
            placeholder="you@example.com"
            aria-label="arXiv contact email"
            style={{ maxWidth: 320 }}
          />
          <Button
            size="sm"
            onClick={() => updateEnv("ARXIV_MAILTO", arxivEmail).catch(console.error)}
          >
            Save
          </Button>
        </SettingRow>
      </SettingGroup>
    </div>
  );
}
