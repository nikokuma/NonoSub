# Invisible Product Shell

Status: in implementation (July 18, 2026)

NonoSub's normal post-onboarding experience is a menu-bar utility, not a dashboard. The workbench remains available as **Settings & Transcript**, but watching is handled by compact, purpose-built surfaces:

- a borderless local-video viewer;
- a transparent live-caption overlay;
- a transparent floating Nono lesson stage;
- a temporary launcher/drop zone for file and live-session startup.

## Implementation gates

1. Generalize the surface and preference contracts for the launcher and per-monitor lesson placement.
2. Extract file/live startup into shared frontend services used by both the launcher and workbench.
3. Centralize native window specifications and tray/context-menu action dispatch in Rust.
4. Implement the launcher, transparent lesson chrome, collapsible Ask Nono drawer, and minimal viewer chrome.
5. Verify migration, startup, placement, drawer escape behavior, right-click suppression, file pause ownership, live continuity, and all existing pipelines.

## Fixed boundaries

- Local files continue to use the internal viewer for correct media timing.
- Live captions continue to use system-audio capture and the compact overlay.
- The workbench is shown automatically only for first-run setup or an error that needs user action.
- Transparent windows use compact rectangular bounds; per-pixel hit testing is excluded.
- Nono voice, model reconstruction, and tail retiming remain separate deliverables.
- No transcript, media, API-key, or lesson content is added to persistent preferences.
