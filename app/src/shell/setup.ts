/**
 * The first-run setup panel's decisions, kept pure: what `dex doctor` said,
 * which failures have a button, and whether to show the panel at all.
 */

export type CheckStatus = "ok" | "fail" | "skip";

export interface Check {
  name: string;
  status: CheckStatus;
  detail: string;
  /** The `dex` command that fixes it, when that takes arguments: `wsl setup Ubuntu`. */
  fix?: string;
}

/** `dex doctor --json`, as the app receives it. */
export interface DoctorReport {
  ok: boolean;
  checks: Check[];
}

/** A setup step the app can run for the owner; `wsl:<distro>` sets up a WSL distro. */
export type Step = "hooks" | "mcp" | "skill" | `wsl:${string}`;

const STEPS: readonly Step[] = ["hooks", "mcp", "skill"];

const WSL_FIX = "wsl setup ";

/**
 * Which checks the panel can fix itself; the rest it can only explain. A WSL
 * distro gets its button even before Dex runs anything there (a skipped
 * check): setting it up first is how an owner gets it ready.
 */
export function stepFor(check: Check): Step | null {
  if (check.status !== "ok" && check.fix?.startsWith(WSL_FIX)) {
    return `wsl:${check.fix.slice(WSL_FIX.length)}`;
  }
  if (check.status !== "fail") return null;
  return STEPS.find((step) => step === check.name) ?? null;
}

const STEP_LABEL: Record<string, string> = {
  hooks: "Install hooks",
  mcp: "Register MCP server",
  skill: "Install skill",
};

/** What a step's button says. */
export function stepLabel(step: Step): string {
  return step.startsWith("wsl:") ? `Set up ${step.slice(4)}` : (STEP_LABEL[step] ?? step);
}

/** Failing checks, in the order doctor reported them. */
export function failures(report: DoctorReport): Check[] {
  return report.checks.filter((check) => check.status === "fail");
}

/**
 * A short string naming what is wrong, so a dismissal can be remembered
 * against *this* set of problems: dismissing "hooks missing" must not also
 * dismiss "hooks broke" six months later.
 */
export function fingerprint(report: DoctorReport): string {
  return failures(report)
    .map((check) => check.name)
    .sort()
    .join(",");
}

/**
 * Whether to open the panel unasked. Only when something is wrong, and only
 * when the owner has not already dismissed exactly this set of problems.
 * An owner can always open it from the palette.
 */
export function shouldOffer(report: DoctorReport, dismissed: string | null): boolean {
  const now = fingerprint(report);
  return now !== "" && now !== dismissed;
}

/** What settings says about the chosen distro when agents cannot run there yet. */
export interface DistroToSetUp {
  distro: string;
  /** Doctor's own words: what is missing. */
  detail: string;
  /** Whether the setup panel can fix it (`dex wsl setup`), or the owner must (Claude Code missing). */
  fixable: boolean;
}

/**
 * The chosen terminal's distro, when doctor looked at it and found it not
 * ready for agents; null for PowerShell, a distro that is ready, or one
 * doctor has not looked at yet (skipped: the re-check after a choice will).
 */
export function distroToSetUp(report: DoctorReport | null, terminal: string): DistroToSetUp | null {
  if (!terminal.startsWith("wsl:")) return null;
  const check = report?.checks.find((c) => c.name === terminal);
  if (check?.status !== "fail") return null;
  return { distro: terminal.slice(4), detail: check.detail, fixable: stepFor(check) !== null };
}

/** What the header line should say. */
export function headline(report: DoctorReport): string {
  const failed = failures(report);
  if (failed.length === 0) return "Dex is set up";
  const fixable = failed.filter((check) => stepFor(check) !== null).length;
  if (fixable === failed.length) {
    return failed.length === 1 ? "One step left to set up Dex" : `${failed.length} steps left to set up Dex`;
  }
  return failed.length === 1 ? "One thing needs your attention" : `${failed.length} things need your attention`;
}
