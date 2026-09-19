import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { showError } from "../platform/notices";
import { type DoctorReport, fingerprint, shouldOffer } from "./setup";
import { Settings } from "./SettingsPanel";
import { Setup } from "./SetupPanel";

function run(action: Promise<unknown>): void {
  void action.catch(showError);
}

/** The set of setup problems the owner last chose to put off. */
const SETUP_DISMISSED = "dex.setupDismissed";

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable: the panel will simply offer again next time.
  }
}

/**
 * The setup panel and the settings panel, which share `dex doctor`'s report:
 * settings shows whether the chosen distro is set up, and sends the owner to
 * setup to do it. `onClosed` gives the keyboard back when either closes.
 */
export function useSetupDialogs(onClosed: () => void) {
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  /** Runs `dex doctor`; opens the setup panel only when it should offer itself. */
  const check = async (offer: boolean) => {
    const next = await invoke<DoctorReport>("setup_check");
    setReport(next);
    if (offer && shouldOffer(next, readPref(SETUP_DISMISSED))) setSetupOpen(true);
  };
  const openSetup = () => {
    setSettingsOpen(false);
    setSetupOpen(true);
    run(check(false));
  };
  /** The gear: settings, with the checks read again for what they say about the terminal. */
  const openSettings = () => {
    setSettingsOpen(true);
    run(check(false));
  };

  const dialogs = (
    <>
      {settingsOpen && (
        <Settings
          report={report}
          onRecheck={() => check(false)}
          onOpenSetup={openSetup}
          onClose={() => {
            setSettingsOpen(false);
            onClosed();
          }}
        />
      )}
      {setupOpen && report && (
        <Setup
          report={report}
          onRecheck={() => check(false)}
          onClose={() => {
            // Remember what was put off, so the same problems do not reopen
            // the panel every start — but new ones still do.
            writePref(SETUP_DISMISSED, fingerprint(report));
            setSetupOpen(false);
            onClosed();
          }}
        />
      )}
    </>
  );
  return { check, openSetup, openSettings, dialogs };
}
