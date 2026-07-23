JpoProducer — chord stamps (SPEC-v2 §6.2)
=========================================

Place stamp files here for **seed only** (copied once into `<exe>/stamps` on first run).
User saves always go to:  <folder of jpo.exe>\stamps\*.jpostamp

Each file is one progression (.jpostamp JSON, UTF-8):

{
  "schema": 1,
  "name": "表示名",
  "bars_hint": 4,
  "blocks": [
    { "start": 0.0, "dur": 4.0, "degree": 1, "quality": "", "octave": 4, "syncopation_fill": false }
  ]
}

Rules:
- start/dur in beats, **relative** (earliest start normalized to 0 on save/load)
- degree: 1–7 (diatonic); no absolute key / bpm in the file
- bars_hint: 4 | 8 | 16 (optional hint; apply clips to loop length)
- Append (UI「追記」): always after last chord block (empty → beat 0). Playhead ignored.
- Same name save: confirm overwrite in UI (no silent auto-rename)
- Broken files are skipped on load (app does not crash)
- After seed, user edits under stamps/ are never overwritten by assets/

To try another stamp from scratch: clear progression, then append again.
