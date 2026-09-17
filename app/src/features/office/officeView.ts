/** The two ways of looking at the office. */
export type OfficeViewKind = "cards" | "office";

const VIEWS: readonly string[] = ["cards", "office"];
const STORAGE_KEY = "dex.office.view";

function isView(value: string | null | undefined): value is OfficeViewKind {
  return typeof value === "string" && VIEWS.includes(value);
}

/**
 * Which view to open with: what the owner last clicked, else `[ui] office_view`
 * from their config, else cards. The click is remembered here in the window and
 * never written to their config file, which stays theirs.
 */
export function chooseView(stored: string | null, configured: string | undefined): OfficeViewKind {
  if (isView(stored)) return stored;
  return isView(configured) ? configured : "cards";
}

/** What the owner last clicked, if this window can remember anything. */
export function storedView(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function rememberView(view: OfficeViewKind): void {
  try {
    localStorage.setItem(STORAGE_KEY, view);
  } catch {
    // Forgetting a preference is not worth telling anyone about.
  }
}
