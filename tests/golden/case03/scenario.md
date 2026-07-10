# case03 — Miditest_broken01 (2026-07-10)

User-reported state after C-3 generator fixes.

**Source:** `C:\Users\user\OneDrive\Desktop\Miditest_broken01.mid`  
**Project:** same as case01 (`MidiTest.jpo`)

## Confirmed OK

- **Syncopation fill** — adaptive window works; short G◆ block no longer leaves huge rest.

## Open issues

1. **Piano (Ch2)** — chord notes still overlap in time.
2. **Bass (Ch3)** — not confined to E1–D#2 (MIDI 28–51); fold/range not audible yet.
3. **Tab3 Edit** — selecting one note highlights all notes on screen (all `NoteId(0)` after Generate All).

## Files

| file | role |
|------|------|
| `broken.mid` | current broken output |
| `MidiTest.jpo` | chord blocks + patterns |
| `fixed.mid` | *(pending — user hand-edit goal)* |

## When fixed.mid arrives

Add asserts for:
- Ch3 pitch min ≥ 28, max ≤ 51
- Ch2 temporal overlap count ≤ goal
- (Tab3 is app-level; separate UI test)