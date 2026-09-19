# Releasing Dex

Both installers are built on GitHub, never on a developer's machine:
`windows.yml` builds the MSI and `macos.yml` the two disk images, and each
attaches what it built to the release that triggered it.

1. **Bump the version** on a `chore/release-x.y.z` branch, in every place it
   lives: `Cargo.toml` (workspace), `Cargo.lock`, `app/package.json`,
   `app/package-lock.json`, `app/src-tauri/tauri.conf.json`. Write
   `docs/release-notes/vx.y.z.md`. Open the PR; merge once CI is green.
2. **Create the release with no files** from the merged master:

   ```
   gh release create vx.y.z --target master --title "Dex x.y.z" --notes-file docs/release-notes/vx.y.z.md
   ```

   Publishing it starts both workflows. Each checks first that the tag is the
   app's version and fails before building if not.
3. **Watch both runs** (`gh run list --workflow windows.yml`, `--workflow
   macos.yml`) until they finish; the release then carries
   `Dex_x.y.z_x64_en-US.msi` and the two `.dmg` files.
4. **Check the assets**: `gh release view vx.y.z` lists all three.

If a run fails after the release is out, fix the cause and run the workflow
by hand for that tag (`gh workflow run windows.yml -f tag=vx.y.z`); it builds
the tag and replaces what is attached.
