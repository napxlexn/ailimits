# winget manifest for AI Limits

The [winget](https://learn.microsoft.com/windows/package-manager/) manifests
for `napxlexn.AILimits` live in the **`winget/`** directory next to this file,
so users can install with:

```
winget install napxlexn.AILimits
```

> This notes file deliberately lives OUTSIDE `winget/`: `winget validate`
> parses *every* file in the manifest directory as a manifest, so a stray
> README/`.md` there makes validation fail. Keep `winget/` to the three YAML
> files only.

Three files make up one package version (manifest schema **1.6.0**):

| File | Role |
|---|---|
| `winget/napxlexn.AILimits.yaml` | version manifest (points at the default locale) |
| `winget/napxlexn.AILimits.locale.en-US.yaml` | name, description, license, tags |
| `winget/napxlexn.AILimits.installer.yaml` | download URL, SHA256, install type |

The installer is the **Inno Setup** per-user `.exe` produced by
`installer/ailimits.iss` (`InstallerType: inno`, `Scope: user`) and attached
to each GitHub Release by `.github/workflows/release.yml`.

## Releasing a new version (the SHA256 chicken-and-egg)

The manifest must carry the **SHA256 of the exact published installer**, which
does not exist until CI has built the release. So the order is:

1. Bump the version in `Cargo.toml` and `installer/ailimits.iss`, update docs,
   and bump `PackageVersion` + the `InstallerUrl` here (and `ReleaseDate`).
2. Tag and push `vX.Y.Z` — CI builds `AiLimits-Setup-X.Y.Z.exe` and creates the
   GitHub Release.
3. Compute the installer's hash and paste it into `InstallerSha256` here:

   ```powershell
   $url = "https://github.com/napxlexn/ailimits/releases/download/vX.Y.Z/AiLimits-Setup-X.Y.Z.exe"
   Invoke-WebRequest $url -OutFile setup.exe
   (Get-FileHash setup.exe -Algorithm SHA256).Hash
   ```

   > The committed `InstallerSha256` is a `000…0` placeholder until this step.

4. Validate, then submit to the community repo
   [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs) under
   `manifests/n/napxlexn/AILimits/X.Y.Z/`:

   ```powershell
   winget validate --manifest .\installer\winget
   # optional local install test (needs the real SHA256 in place):
   winget install --manifest .\installer\winget
   ```

   Easiest submission path is [`wingetcreate`](https://github.com/microsoft/winget-create):

   ```powershell
   wingetcreate update napxlexn.AILimits `
     --version X.Y.Z `
     --urls https://github.com/napxlexn/ailimits/releases/download/vX.Y.Z/AiLimits-Setup-X.Y.Z.exe `
     --submit
   ```

   `wingetcreate` downloads the installer, fills the SHA256 for you, and opens
   the PR. The manifests here are the source of truth to keep in sync with what
   lands in `winget-pkgs`.

## Manifest text: avoid the automated policy triggers

`winget-pkgs` runs automated **content-policy label checks** on the manifest
text before any human looks at the PR. They are plain substring matches, not
content review — a label means a word in your text matched a list, nothing
more. Arguing the false positive in PR comments does not clear the label; only
editing the text does.

**Confirmed the hard way (PR #407754, v0.6.0):** the `Policy-Test-2.7` label
(adult content) fired on the word **`explicitly`** in `Description`, because it
contains the substring `explicit`. Two reasoned comments over ten days changed
nothing; rewording `credentials explicitly added by the user` →
`credentials added manually by the user` was the whole fix. Contributor
@DandelionSprout pointed this out on the PR.

**Rule: before every submission, run the gate. It is not advisory — a
non-zero exit means do not submit.**

```powershell
pwsh installer\check-manifests.ps1
```

It checks the five things that have gone wrong or nearly gone wrong:

1. **policy trigger substrings** (the rule above — the matcher is a substring
   match, so `explicitly` hits on `explicit`);
2. **the InstallerSha256 is real** — 64 hex digits, not the all-zero
   placeholder that sits in the manifests between a version bump and the
   release build;
3. **one PackageVersion** across all three manifests;
4. **Cargo.toml agrees** with that version;
5. **the InstallerUrl points at the matching release tag.**

The placeholder check is the reason this is a script and not a habit: the
zeroes are deliberate for the whole window between bumping the version and
publishing the build, so nothing but attention stood between them and a
submission.

Preferred replacements: `explicitly` → `manually` / `directly`;
`hack` → `troubleshoot`; `crack` → `fix`. Keep the substitution in
`installer/winget/` and in whatever lands in `winget-pkgs` identical.

Other manifest-hygiene items the reviewers have flagged:

- **No `DisplayVersion` in `AppsAndFeaturesEntries`** when it would duplicate
  `PackageVersion` — maintainer @stephengillie asked for its removal on the
  same PR. `ProductCode` alone is enough for correlation.
- **Comments in the YAML are English-only**, like the rest of the codebase.
  These files are read by outside reviewers.

## Notes

- **License:** AI Limits is distributed under `GPL-3.0-or-later`. Keep the
  root `LICENSE`, Cargo metadata, release notes, and the WinGet `License` /
  `LicenseUrl` fields aligned.
- **`ProductCode`** is the Inno per-user uninstall key (`{AppId}_is1`) so winget
  can detect an existing install and upgrade in place.
