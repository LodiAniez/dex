import { useEffect, useRef, useState } from "react";
import { terminalChoices, useTerminal } from "../platform/runtimes";
import "./setup.css";
import { type DoctorReport, distroToSetUp } from "./setup";
import { IS_MAC } from "./keybindings";
import { TerminalChoice } from "./TerminalChoice";

export interface SettingsPanelProps {
  /** The last setup checks; null until they have run. */
  report: DoctorReport | null;
  /** Re-runs the checks; the parent owns the report. */
  onRecheck: () => Promise<void>;
  /** Opens the setup panel, where a distro is set up for agents. */
  onOpenSetup: () => void;
  onClose: () => void;
}

/**
 * The title bar's gear: the settings Dex keeps itself, which today is the
 * terminal it opens (PowerShell or a WSL distro). Everything else is in
 * `config.toml`, which Dex only reads.
 */
export function SettingsPanel({ report, onRecheck, onOpenSetup, onClose }: SettingsPanelProps) {
  const terminal = useTerminal(report);
  const { view, error } = terminal;
  // While a restart offer waits, nothing here may lead away from it.
  const [offerPending, setOfferPending] = useState(false);

  // Take the keyboard on open, so Escape closes the panel and not xterm's.
  const panel = useRef<HTMLElement>(null);
  useEffect(() => panel.current?.focus(), []);

  const distro = view && !offerPending ? distroToSetUp(report, view.terminal) : null;
  return (
    <div className="setup-backdrop" onMouseDown={onClose}>
      <section
        ref={panel}
        tabIndex={-1}
        className="setup settings"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <header className="setup-head">
          <h2>Settings</h2>
        </header>
        {view === null ? (
          <p className="setup-sub">{error ? `Dex could not read its terminal: ${error}` : "Loading..."}</p>
        ) : terminalChoices(view).length === 0 ? (
          <p className="setup-sub">
            {IS_MAC
              ? "Dex runs its panes and agents in your login shell."
              : "Dex runs its panes and agents in PowerShell. Install WSL to be able to choose a Linux distro instead."}
          </p>
        ) : (
          <TerminalChoice terminal={terminal} onChosen={onRecheck} onOfferChange={setOfferPending} />
        )}
        {distro && (
          <div className="setup-check settings-setup">
            <span className="setup-detail">
              {distro.distro} is not ready for agents yet.
              <span className="setup-explain">{distro.detail}</span>
            </span>
            {distro.fixable && (
              <button type="button" className="setup-button" onClick={onOpenSetup}>
                Open setup
              </button>
            )}
          </div>
        )}
        <footer className="setup-foot">
          <button type="button" className="setup-button primary" onClick={onClose}>
            Done
          </button>
        </footer>
      </section>
    </div>
  );
}
