# rockhound

Fast Star Citizen signature scanner: reads the HUD signature value and tells the player what is out there (Rust crates, Tauri + React app in `app/`, website in `site/`).

- What it is and how it is laid out: `README.md`; how to contribute: `CONTRIBUTING.md`; history and release notes: `DEVLOG.md`.
- Decisions (binding when `accepted`): `docs/decisions/README.md` — read the ones your change touches.
- PRs: `.github/pull_request_template.md`, with the Door line (one-way / two-way).
- A version bump on `main` publishes a release: leave the version alone unless asked; user-visible changes get a `DEVLOG.md` entry.
- Check before "done": `sh scripts/check.sh` (fmt, clippy, tests of the Rust crates, frontend build and tests; not the Tauri app crate, e2e or audit).
- No decision on record = a question on the issue, not a guess.
- This file stays at 20 lines or fewer; details go into linked files.
