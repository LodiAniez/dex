// Builds `dex` and `dex-mcp` for the architecture Tauri is bundling, and
// stages them in src-tauri/macos/, where tauri.macos.conf.json copies them
// into Dex.app/Contents/MacOS. Tauri names the target in TAURI_ENV_TARGET_TRIPLE
// for its before-build command, so `tauri build --target x86_64-apple-darwin`
// on an Apple Silicon Mac stages Intel binaries.
//
// Each is signed ad hoc: the app bundle is signed ad hoc too (Dex ships
// unsigned for now, #36), and codesign refuses a bundle holding unsigned code.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = dirname(dirname(fileURLToPath(import.meta.url)));
const repo = dirname(app);
const triple = process.env.TAURI_ENV_TARGET_TRIPLE;
if (!triple) {
  throw new Error("TAURI_ENV_TARGET_TRIPLE is not set; run this through `tauri build`.");
}

const run = (program, args) => execFileSync(program, args, { cwd: repo, stdio: "inherit" });

run("cargo", ["build", "--release", "--target", triple, "-p", "dex-cli", "-p", "dex-mcp"]);
const staged = join(app, "src-tauri", "macos");
mkdirSync(staged, { recursive: true });
for (const name of ["dex", "dex-mcp"]) {
  const to = join(staged, name);
  copyFileSync(join(repo, "target", triple, "release", name), to);
  run("codesign", ["--force", "--sign", "-", to]);
}
