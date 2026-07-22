# Future task: Grok Co-Producer (Chrome extension)

**Status:** Deferred until JpoProducer core/ship is solid.  
**Not started.** Do not block v1.0 on this.

## Product positioning

| Layer | Promise |
|-------|---------|
| **Jpo alone** | Download + FluidR3 → works. Job text can be pasted into **any** LLM chat (Grok recommended; others untested officially). |
| **Chrome extension (official)** | **Grok-only** convenience: make co-production with Grok easier (insert job / bring answer back). |
| **Other LLMs** | Same job format works in principle. Customize yourself — source on GitHub. |

Name intent: **Grok as co-production producer** (working title: **Grok Co-Producer** for JpoProducer).

## Why not multi-LLM official extension

- Job pipeline is already LLM-agnostic (text in / notes out).  
- Official depth is Grok only → less DOM maintenance, clearer story.  
- “Prefer Grok; other LLMs DIY” stays consistent.

## Technical sketch (when we build it)

1. **Phase 0 (simplest, fewest bugs):** Extension UI on Grok pages + **clipboard** only. Jpo keeps S1 copy / import. No IPC required.  
2. **Phase 1 (optional):** Jpo serves **localhost HTTP** (`GET /job`, `POST /result`). Extension fetch. Fallback to clipboard if Jpo is down.  
3. **Avoid first:** Full auto-send/DOM puppeting of Grok; Native Messaging (heavier setup).

## Dev / distribute

- Sideload via Chrome **Developer mode** → “Load unpacked” (no Store required for early users).  
- Later: Chrome Web Store optional.  
- Grok Build can author the extension sources; human loads them in Chrome for test.

## Repo layout (suggested when starting)

```text
extensions/grok-co-producer/
  manifest.json
  README.md
  …
```

## Related

- Job builder in app: `build_grok_part_job()` (survey → structured prompt)  
- S1 clipboard / S2 API already in Edit Grok lane  
- Portable app pack is separate (`pack.ps1`); extension is not inside that zip unless we choose later  
