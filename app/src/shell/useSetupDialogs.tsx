import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { showError } from "../platform/notices";
import { type DoctorReport, fingerprint, shouldOffer } from "./setup";
import { SettingsPanel } from "./SettingsPanel";
import { SetupPanel } from "./SetupPanel";

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
 * setup to do it. One is open at a time. `onClosed` gives the keyboard back.
 */
export function useSetupDialogs(onClosed: () => void) {
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [open, setOpen] = useState<"setup" | "settings" | null>(null);

  /** Runs `dex doctor`; offers the setup panel only when it should, and never over another dialog. */
  const check = async (offer: boolean) => {
    const next = await invoke<DoctorReport>("setup_check");
    setReport(next);
    if (offer && shouldOffer(next, readPref(SETUP_DISMISSED))) setOpen((now) => now ?? "setup");
  };
  const openSetup = () => {
    setOpen("setup");
    void check(false).catch((err) => {
      // No report to show, and none coming: close again rather than open later by surprise.
      if (report === null) setOpen((now) => (now === "setup" ? null : now));
      showError(err);
    });
  };
  /** The gear: settings, with the checks read again for what they say about the terminal. */
  const openSettings = () => {
    setOpen("settings");
    // Settings works without them; a failure is not worth a notice.
    void check(false).catch((err) => console.warn("setup check failed", err));
  };
  const close = () => {
    setOpen(null);
    onClosed();
  };

  const dialogs = (
    <>
      {open === "settings" && (
        <SettingsPanel report={report} onRecheck={() => check(false)} onOpenSetup={openSetup} onClose={close} />
      )}
      {open === "setup" && report && (
        <SetupPanel
          report={report}
          onRecheck={() => check(false)}
          onClose={() => {
            // Remember what was put off, so the same problems do not reopen
            // the panel every start — but new ones still do.
            writePref(SETUP_DISMISSED, fingerprint(report));
            close();
          }}
        />
      )}
    </>
  );
  return { check, openSetup, openSettings, dialogs };
}
