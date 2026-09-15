import { useEffect, useRef } from "react";
import { PALETTE } from "./palette";
import { useDismiss } from "./useDismiss";

interface Props {
  value: string | null;
  anchor: { x: number; y: number };
  onPick: (color: string | null) => void;
  onClose: () => void;
}

/** Palette swatches, a custom color, or no color. */
export function ColorPicker({ value, anchor, onPick, onClose }: Props) {
  const ref = useDismiss<HTMLDivElement>(onClose);
  const customRef = useRef<HTMLInputElement>(null);

  // React's onChange on a color input fires on every drag step; the native
  // `change` event fires once, when the system picker closes. Commit on that.
  useEffect(() => {
    const input = customRef.current;
    if (!input) return;
    const commit = () => onPick(input.value);
    input.addEventListener("change", commit);
    return () => input.removeEventListener("change", commit);
  }, [onPick]);

  return (
    <div
      ref={ref}
      className="popover color-picker"
      style={{ left: anchor.x, top: anchor.y }}
      role="dialog"
      aria-label="Workspace color"
      onClick={(event) => event.stopPropagation()}
    >
      <div className="swatches">
        {PALETTE.map((color) => (
          <button
            key={color}
            type="button"
            className={`swatch${color === value ? " selected" : ""}`}
            style={{ background: color }}
            aria-label={`Use ${color}`}
            onClick={() => onPick(color)}
          />
        ))}
      </div>
      <label className="picker-row">
        <input ref={customRef} type="color" defaultValue={value ?? PALETTE[0]} />
        Custom color…
      </label>
      <button type="button" className="picker-row link" onClick={() => onPick(null)}>
        No color
      </button>
    </div>
  );
}
