# case02 — sync ON + short G◆ block (2.5 beats)

Regression for the three generator fixes:

1. **Adaptive sync window** — 2.5-beat block at 5.5 uses 1-beat sync window + 1.5-beat refill (not 2-beat window leaving 0.5-beat gap).
2. **Bass fold** — C#/D/D# up, E–B down (`map_pattern_pitch` at octave 2).
3. **Piano pitch-class trim** — same-onset duplicate pitch classes removed (loudest kept).

## Project (MidiTest.jpo)

- bpm: 128, key: C major
- gen range: 0–16 beats
- patterns: Piano01 / Bass8beat01 / Drum8beat_01
- **syncopation_fill: true** (global Tab2 toggle)

## Chord blocks (focus)

| start | dur | degree | sync |
|-------|-----|--------|------|
| 5.5 | 2.5 | 5 (G) | **ON** |

## Acceptance

- Sync window ends at **6.5** (not 7.5).
- Piano and bass have notes in **6.5–8.0** refill region.
- Bass at beat 0: C2 (36), not parallel-transpose drift.
- No duplicate pitch classes at same piano onset.

## Reference

- Domino on Desktop — UI/workflow reference for template editing (no automation).
- `bass0710.png` — fold contour diagram.