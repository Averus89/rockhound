# sigextract

Refreshes `data/signatures.json` from the Star Citizen install after a patch.
It reads `Data/Game2.dcb` out of `Data.p4k` (no unp4k or other tools needed),
finds every entity's radar signature and updates the table's `base` values.
Names, tiers and `max_count` are not in the game data, so they stay as they
are in the table.

## After a patch

1. Let the launcher finish updating the game. The archive usually lives at
   `C:\Program Files\Roberts Space Industries\StarCitizen\LIVE\Data.p4k`
   (`PTU` instead of `LIVE` for the test server).
2. Get the tool: download `sigextract.exe` from the `rockhound-windows`
   artifact of the latest CI run on main, or build it from a checkout with
   `cargo build --release -p rockhound-sigextract`
   (it lands in `target\release\sigextract.exe`).
3. From the root of a rockhound checkout on an up-to-date `main`, run:

   ```powershell
   sigextract update
   ```

   Add `--game "D:\Games\StarCitizen\LIVE"` if the game is installed
   elsewhere. `--dry-run` shows the report without writing anything.
   Reading the archive and decompressing Game2.dcb takes a moment.
4. Read the report (see below). If values changed, commit and push the file:

   ```powershell
   git checkout -b signatures-4.x.y
   git commit -am "Update signatures for SC 4.x.y"
   git push -u origin HEAD
   ```

   and open a PR. Once merged, apps pick the new table up at their next
   start (when the file is publicly readable; see the main README). No
   version bump is needed.

`update` sets `sc_version` from the launcher's `build_manifest.id` next to
`Data.p4k` (override with `--sc-version 4.8.0`) and stamps `data_version`
with today's date, but only when something changed.

## Reading the report

- **Changed**: rows whose value in the game differs from the table. These
  are written to the file.
- **Added**: minerals the game has but the table doesn't. They get their
  tier from the entity name and `max_count` 100; check both.
- **No entity matched these rows**: the tool could not tell which game
  entities belong to the row, so the old value is kept. Usually means CIG
  renamed the entities; `list` shows the new names.
- **Rows whose entities disagree**: the row's entities have more than one
  value; the most common one is used. Worth a look in game.
- **Entities no rule covers**: mining or salvage entities none of the rules
  below recognize, grouped by value, such as debris from a new wreck type.
  Add a rule in `src/table.rs` if one belongs in the table.

Rows are matched by entity name (SC 4.7 naming):
`MineableRock_Asteroid<Tier>_<Mineral>` and `MineableRock_Surface<Tier>_<Mineral>`
for ship mining, `MineableRock_FPS_*` and `MineableRock_GroundVehicle_*` for
small and large ground deposits, `SalvageScrap*`, `SalvageLock*` and
`SalvageableRepairable_*` for panels, and `SalvageableDebris_<Ship>`
(Avenger Titan, Ares Inferno, C2, 890 Jump) for wreck debris. Older rock
entities (`AsteroidCTypeMineableRock`, `GraniteMineableRock_*`), tests and
templates are ignored.

## After a game update changes things

The tool is built to keep working when CIG reshuffles the data:

- **Bigger records.** The record size is worked out from the file rather
  than assumed, and the field order is checked against every record.
- **Moved signatures.** If the usual path to the signature array is gone,
  it searches the signature component for it and prints a note saying so.
- **New minerals or tiers.** Any `MineableRock_Asteroid<Tier>_<Mineral>`
  becomes a row, including tiers it hasn't seen before.
- **Renamed entities.** Rows nothing matches keep their old value and are
  listed in the report, and unfamiliar `MineableRock_*` or salvage entities
  are listed with their values, so a rename shows up instead of silently
  freezing the table.

When CIG changes the format beyond that, the parser stops with an error
rather than writing wrong values. Developers can check a new patch with
this test, which fails if any table row is no longer found:

```powershell
$env:SIGEXTRACT_GAME = "G:\StarCitizen\LIVE\Data.p4k"
cargo test --release -p rockhound-sigextract -- --ignored
```

## When something looks wrong

- `sigextract list --filter mineable > entities.tsv` writes every entity with
  a signature (value, record name, file, all channels). Sending that file is
  the quickest way to get the matching rules fixed.
- `sigextract dump MineableRock_Gold` prints matching records as JSON, to
  see where the signature moved if a patch changed the data layout.
- "unsupported Game2.dcb layout" means CIG changed the file format; the
  parser in `src/datacore.rs` needs updating. `sigextract export` saves the
  decompressed `Game2.dcb` (about 330 MB) so it can be studied without the
  whole archive; zip it and send it along with the error.

## How it works

The value the mining HUD shows is channel 4 (cross section) of
`EntityClassDefinition.Components[] -> SSCSignatureSystemParams.radarProperties
-> baseSignatureParams.signatures[]`. The chain and the Game2.dcb layout
follow the notes Mallachi published with SC Signature Scanner
(`SignatureValue.md`), which were based on StarBreaker. `Data.p4k` is a
ZIP64 archive with CIG's own local header signature and Zstandard entries.
