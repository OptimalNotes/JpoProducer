# JpoProducer — agent instructions

## Project

Rust + egui J-Pop/J-Rock MIDI sketch tool.  
**Spec of truth:** `SPEC-v2.md` (five pillars + reshape contracts).  
**History:** `SPEC-v1.md` (superseded). Gaps: `HANDOVER.md`.

## Always

1. Read **`SPEC-v2.md`** then `HANDOVER.md` before non-trivial edits.
2. Follow skill **`jpo-producer`** and its references (`invariants`, Domino lessons, etc.).
3. Develop on **WSL** `~/JpoProducer` or Windows `C:\Users\user\JpoProducer`. GUI listen: Windows preferred.
4. After logic changes: `cargo test`.
5. **Input isolation** via `InputFocus` (SPEC-v2 §4.2), not flag soup:
   - ChordStrip: blocks only
   - PianoRoll: Select/Draw/Erase, Ctrl+C/X/V/D, Q/W/E, Undo
   - GrokText: text only (do not steal roll shortcuts)
   - Arrange: bank/slots only
6. **Unique `NoteId` always** on generate/import/paste. Single `NoteSelection` model (SPEC-v2 §5.4).
7. Generate pipeline ends with **sync coverage + cleanup** (SPEC-v2 §6.3)—not raw pattern dump.
8. Beat-grid times are **BPM-independent**.
9. **Loop SoT:** `loop_bank[active]` + flush before switch/save (SPEC-v2 §5.3).
10. Do not automate Domino GUI; patterns may be hand-edited there.
11. Spec changes → update **`SPEC-v2.md` first**; then code; then `HANDOVER.md`.
12. Do not expand scope past SPEC-v2 DoD without explicit user approval.
13. **Reshape, don't rewrite** (SPEC-v2 D7). No full rewrite / no revive `archive/jpo-v2` as mainline.
14. Do not mix large module splits with behavior fixes.

## Five pillars (do not invert priority)

1. Dense J-Pop chord progressions (sub-bar, syncopation with **full window coverage**)
2. Simple accompaniment bed (not full arrange)
3. Normal MIDI editing (including multi-select move)
4. 4/8/16 loops → song skeleton
5. Grok co-create: S1 clipboard required, S2 API optional first-class

## Do not

- Treat Domino feature-parity as a goal.
- Full rewrite from empty tree.
- “Fix” golden files to match a buggy generator without a SPEC change.
- Ship generated notes with duplicate ids, uncovered sync windows, or unchecked bass range / same-pitch overlap.
- Bury new work in README roadmap tables; use SPEC-v2 + HANDOVER.
