# Security

## Supported versions

Only the latest release gets security fixes. An installer copy keeps itself
up to date while *Automatic updates* is on; winget, Chocolatey, Scoop and
the Microsoft Store update theirs. If you run an older version, update
before reporting: the issue may be fixed already.

## Reporting a vulnerability

Please do not open a public issue, discussion or pull request for a
vulnerability. Report it privately on GitHub instead: the repository's
**Security** tab, **Report a vulnerability**
(https://github.com/napxlexn/ailimits/security/advisories/new). Only the
maintainer sees it.

Say what is affected (the widget, `ailimits-auth`, the installer, the
updater, the release files), the version, and how to reproduce it. Never
include a real token, key or password, yours or anyone's: a placeholder
such as `sk-ant-...` is enough. A log line from `%APPDATA%\AiLimits` is
welcome; the app never writes a token there, but look it over first.

AI Limits is maintained by one person. You will get a reply as soon as
possible; please give the fix a reasonable time before you disclose. The
fix ships in a release, the CHANGELOG says what it fixes, and you are
credited there unless you ask not to be.

## What is in scope

- Anything that could expose a token or key the app stores or reads, or
  send one to a host other than the provider's own (see "What is read, and
  when" in [docs/en/CONFIG.md](docs/en/CONFIG.md), and the hosts in
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md)).
- The updater: anything that would make it install a file that is not a
  signed release (see "Updates and releases" in
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md)).
- The installer and the uninstaller, and the files the app writes in
  `%APPDATA%\AiLimits` and `%TEMP%`.
- Reads of other tools' files before the user allowed them, or writes to
  them at all.

Out of scope: the providers' own services and endpoints, and the tools
whose sign-in the app reads (report those to their makers); Windows
SmartScreen's "Windows protected your PC" on an installer that is not
code-signed (known, see the README); antivirus detections on a release
(report them as a regular issue).

## Checking a release

Every release carries the SHA-256 GitHub shows for each file, and from
0.7.1 an Ed25519 signature (`.sig`) for the installer and the portable zip.
The signing key's public half is built into the app
(`RELEASE_KEYS` in [src/updater/signature.rs](src/updater/signature.rs),
key id `1507bd92b1a4bb7b`); its private half is kept offline by the
maintainer and in the release pipeline's secret. The app's updater installs
nothing without a valid signature. To check a download by hand, see
[Install](README.md#install) in the README.
