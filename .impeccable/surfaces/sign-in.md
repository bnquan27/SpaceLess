# Sign-in direction comparison

Mode: Operate.

Approved direction: Orbit (user selection, 2026-11-08). Focal motion: astronaut in zero gravity; 8-second floating loop and quiet companion beacon. Pause/resume control, offscreen/hidden-tab suspension, reduced-motion static alternative. Budget: CSS transform/opacity only for loops; no per-frame React updates or animated blur. Form and navigation remain stationary.

User task: sign in from the standalone Orbit page at `/` in the Tauri desktop application. The comparison tool remains at `/design` for reference.

Pinned brief: derive from the provided space sign-in; Vietnamese; React frontend, Rust + Tauri, LiveKit. Specification uses username/password rather than sample email login.

Success: app launches directly into Orbit, floating and pause/resume work, form remains usable at desktop and narrow widths, no misleading authentication, and a release Tauri executable can be built for review.

Scope: coded design comp and desktop shell only. Registration and recovery are explicitly non-network previews. Authentication service, LiveKit token service and signed desktop packaging remain future implementation work.
