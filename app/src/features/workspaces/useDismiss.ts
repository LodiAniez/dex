import { useEffect, useRef } from "react";

/** Calls `onDismiss` on a pointer press outside the returned element, or on Escape. */
export function useDismiss<T extends HTMLElement>(onDismiss: () => void) {
  const ref = useRef<T>(null);
  useEffect(() => {
    const onPointer = (event: PointerEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onDismiss();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onDismiss();
    };
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [onDismiss]);
  return ref;
}
