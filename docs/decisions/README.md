# Decisions

Format: `TEMPLATE.md`.

- `proposed` — evidence of thinking, does not bind; cite it.
- `accepted` — binds; contradicting it needs the evidence its "Revisit if" names.
- `superseded` — kept for history; the newer record sets `supersedes:`, this one gets `superseded-by:`.

Write a record for anything hard to reverse: data shape, public contracts, dependencies, auth, CI, deploy, costs.

| id | title | status | touches |
|---|---|---|---|
| 0001 | [Record decisions in docs/decisions](0001-record-decisions.md) | accepted | all |
| 0002 | [Match the HUD font to templates instead of general OCR](0002-template-matching-not-ocr.md) | accepted | crates/rockhound-core |
