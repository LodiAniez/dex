import { useDismiss } from "./useDismiss";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
}

interface Props {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

/** A right-click menu at the cursor. */
export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useDismiss<HTMLDivElement>(onClose);
  return (
    <div
      ref={ref}
      className="popover context-menu"
      style={{ left: x, top: y }}
      role="menu"
      onClick={(event) => event.stopPropagation()}
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          className={item.danger ? "danger" : undefined}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
