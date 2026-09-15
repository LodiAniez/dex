import { open } from "@tauri-apps/plugin-dialog";
import { useState, type FormEvent } from "react";
import { showError } from "../../platform/notices";
import { PALETTE, nextPaletteColor } from "./palette";
import { createWorkspace, useWorkspaces } from "./workspaceStore";

/** Inline form for a new workspace: name, root folder, color. */
export function NewWorkspaceForm({ onDone }: { onDone: () => void }) {
  const list = useWorkspaces();
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [color, setColor] = useState(() => nextPaletteColor(list?.workspaces.length ?? 0));
  const [busy, setBusy] = useState(false);

  const browse = async () => {
    const picked = await open({ directory: true, multiple: false, title: "Workspace folder" });
    if (typeof picked === "string") setRootPath(picked);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      await createWorkspace({ name: name.trim() || undefined, rootPath: rootPath || undefined, color });
      onDone();
    } catch (err) {
      showError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="new-workspace"
      onSubmit={(event) => void submit(event)}
      onKeyDown={(event) => {
        if (event.key === "Escape") onDone();
      }}
    >
      <input autoFocus placeholder="Name (optional)" value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
      <div className="folder-row">
        <input readOnly value={rootPath} placeholder="Home folder" title={rootPath || "Home folder"} />
        <button type="button" onClick={() => void browse().catch(showError)}>
          Browse…
        </button>
      </div>
      <div className="swatches">
        {PALETTE.map((swatch) => (
          <button
            key={swatch}
            type="button"
            className={`swatch${swatch === color ? " selected" : ""}`}
            style={{ background: swatch }}
            aria-label={`Use ${swatch}`}
            onClick={() => setColor(swatch)}
          />
        ))}
      </div>
      <label className="picker-row">
        <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
        Custom color…
      </label>
      <div className="form-actions">
        <button type="button" onClick={onDone}>
          Cancel
        </button>
        <button type="submit" className="primary" disabled={busy}>
          Create
        </button>
      </div>
    </form>
  );
}
