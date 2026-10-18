# Contributing to Rockhound

Thanks for helping. Bug reports, recognizer test data, and pull requests are
all welcome.

## Licensing of contributions

Rockhound is licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE). By opening a pull request or
otherwise submitting a contribution, you agree that:

- your contribution is your own work, or you have the right to submit it;
- it is licensed to everyone under the project license; and
- you grant the maintainer (Alan, Averus89) a perpetual, worldwide,
  royalty-free license to use, modify, sublicense and relicense your
  contribution, including under other terms, so the project can change its
  license later without tracking down every contributor.

Do not submit code or data copied from projects whose license conflicts with
this, and say so in the PR when you include third-party material so it can be
listed in `THIRD_PARTY_NOTICES.md`.

## Reporting bugs

Open an issue with the app version, your screen resolution, and what you saw
versus what you expected. For misreads, turn on **Save debug images** in the
app and attach the contents of
`%APPDATA%\io.github.averus89.rockhound\debug`; those captures beat videos.

Security problems do not go in public issues: see [SECURITY.md](SECURITY.md).

## Pull requests

1. Fork the repo and branch from `main`.
2. Keep each PR focused on one change, and describe what a user would notice.
3. Make sure the checks CI runs pass locally:
   ```sh
   cargo fmt --all --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   cd app && npm ci && npm run build && npm test
   ```
4. Add a `DEVLOG.md` entry for anything user-visible. Leave the version alone
   unless the maintainer asks: a version bump on `main` publishes a release.

The maintainer reviews every PR and may ask for changes or decline ones that
don't fit the project's direction.
