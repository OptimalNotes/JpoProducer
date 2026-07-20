JpoProducer — chord stamps
==========================

Place stamp files here (dev) or next to jpo.exe in a "stamps" folder.

Each file is one progression (.jpostamp or .json):

{
  "name": "表示名",
  "blocks": [
    { "start": 0.0, "dur": 4.0, "degree": 1, "quality": "", "octave": 4, "syncopation_fill": false }
  ]
}

- start/dur are in beats, relative (first block usually starts at 0)
- degree: 1–7 (diatonic)
- quality: "", "m", "7", "m7", "maj7", "dim", "sus4", ...
- On paste: playhead or end of timeline; parts past loop length are clipped

User saves go to:  <folder of jpo.exe>\stamps\*.jpostamp
