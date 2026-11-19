---
id: 0002
title: Match the HUD font to templates instead of general OCR
status: accepted
date: 2026-10-14
issue: —
supersedes: —
superseded-by: —
signed-by: —
---

## Context
Rockhound reads the signature number from the game HUD, which uses one fixed digit font. Written down on 2026-11-19 from what the repo already shows: `DEVLOG.md` entry "2026-10-14: v0.1.0, project start" ("Decisions: Template matching instead of general OCR"), the first commit `9f2acbb`, and `README.md` ("Instead of a general OCR engine it matches the fixed HUD digit font against stored templates"). The code is `crates/rockhound-core` (`recognize.rs`, `resolve.rs`).

## Options
1. A general OCR engine (EasyOCR was measured at about 180 ms per read and a 275 MB download; Windows OCR gave no confidence score) — kind: full
2. Match glyphs to stored templates of the HUD font, check the result against the signature table — kind: smaller

Recommendation: 2, because it reads in a few milliseconds, keeps the app small and gives per-glyph scores that the correction step uses.

## Decision
Option 2, taken at project start (2026-10-14) per the DEVLOG. This record only documents it; Alan signs by merging the PR that adds it.

## Consequences
Templates must exist for each HUD size: they are bundled under `data/glyphs/` and players can contribute more (glyph packs). Windows OCR is used only to teach templates on a fresh install, never to show a value on its own once templates read it. A new HUD font needs new templates.

## Revisit if
templates cannot be collected for common HUD sizes, or `TODO.md` item "templates good enough, or add ONNX digit model" is decided for a model.
