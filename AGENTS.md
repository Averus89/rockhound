# rockhound

Rockhound reads the Star Citizen HUD signature number and tells players what it means (ore, rock count, salvage): Rust crates, a Tauri + React app in `app/`, a static site in `site/`.

- Contributing and licence of contributions: `CONTRIBUTING.md`; security reports: `SECURITY.md`.
- Decisions (binding when `accepted`): `docs/decisions/README.md` — read the ones your change touches.
- Principles: `docs/principles/`.
- Primitives (what the system is made of): `docs/primitives.yaml` — compose before you add.
- PRs: `.github/pull_request_template.md`, with the Door line (one-way / two-way).
- Check before "done": `sh scripts/check.sh` (needs cargo and node/npm; CI also runs the Tauri build, coverage, e2e, audit, Windows).
- Releases: a version bump in `Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json` plus a `DEVLOG.md` entry (README).
- No decision on record = a question on the issue, not a guess.
- Plans become issues via `.agents/skills/split-plan/`; a worker closes an issue with the comment
  `done: <what> / worker: <harness · model · effort> / commit: <sha>` (one line each).
- This file stays at 20 lines or fewer; details go into linked files.
