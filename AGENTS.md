# JpoProducer — agent instructions

## Project

Rust + egui J-Pop/J-Rock MIDI sketch tool.  
**Spec of truth:** `SPEC-v1.md` (five pillars). Implementation gaps: `HANDOVER.md`.

## Always

1. Read **`SPEC-v1.md`** then `HANDOVER.md` before non-trivial edits.
2. Follow skill **`jpo-producer`** and its references (`invariants`, Domino lessons, etc.).
3. Develop on **WSL** `~/JpoProducer`. Windows tree is for release packaging.
4. After logic changes: `cargo test`.
5. **Input isolation** (by focus zone, not only tab count):
   - Chord strip: blocks only (no note Ctrl+C/V)
   - Piano roll (Edit / Sketch): Select/Draw/Erase, Ctrl+C/X/V/D, Undo
   - Arrange: no note edit
6. **Unique `NoteId` always** on generate/import/paste. Selection is id-based.
7. Generate pipeline ends with **cleanup** (ids, range, same-pitch overlap)—not raw pattern dump.
8. Beat-grid times are **BPM-independent**.
9. Do not automate Domino GUI; patterns may be hand-edited there.
10. Spec changes → update `SPEC-v1.md` first; then code; then `HANDOVER.md`.
11. Do not expand scope past SPEC v1.0 DoD without explicit user approval.

## Five pillars (do not invert priority)

1. Dense J-Pop chord progressions (sub-bar, syncopation)
2. Simple accompaniment bed (not full arrange)
3. Normal MIDI editing
4. 4/8/16 loops → song skeleton
5. Grok context → MIDI parts import

## Do not

- Treat Domino feature-parity as a goal.
- Full rewrite / revive `archive/jpo-v2` as mainline.
- “Fix” golden files to match a buggy generator without a SPEC change.
- Mix large refactors with behavior fixes.
- Ship generated notes with duplicate ids or unchecked bass range / same-pitch overlap.
- Bury new work in README roadmap tables; use SPEC + HANDOVER.
