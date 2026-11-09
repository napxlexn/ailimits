# Contributing to AI Limits

AI Limits is written and maintained by one person, napxlexn. Every line of
code in this repository has the same author, and it stays that way: pull
requests are not merged, however good they are.

Why: AI Limits updates itself silently and reads the login tokens of Claude
Code, Codex CLI, gh and Antigravity. Whatever it ships runs on people's
machines with no review step on their side, so every line goes through one
pair of hands. One author also keeps the code, the design and the licensing
in one place.

## What helps

- **Bug reports.** Open an issue with the AI Limits version (the last line of
  the tray menu), the Windows build, how you installed it (installer, winget,
  Chocolatey, Scoop, Microsoft Store, portable zip), which providers are on,
  and what happened versus what you expected. A debug log helps most: set
  `AILIMITS_LOG=ailimits=debug`, reproduce, and attach
  `%APPDATA%\AiLimits\ailimits.log`. The log never contains tokens.
- **Ideas and new providers.** Open an issue. For a provider, say which plan
  you are on and where the provider itself shows your usage (a web page, a CLI
  command).

When an idea from an issue ships, the changelog credits the issue it came
from.

## Pull requests

A pull request is closed with a thank-you. If you have already written code,
describe the change in an issue instead: the idea is what counts, and it is
implemented here from the issue, not from the diff.

## Security

Report vulnerabilities privately through GitHub (Security tab, "Report a
vulnerability"), not in a public issue.

## Forks

The GPL allows them. A modified build needs its own name and logo; see
[TRADEMARKS.md](TRADEMARKS.md).
