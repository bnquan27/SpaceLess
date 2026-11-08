# SpaceLessApp

<!-- impeccable:product-schema 1 -->

## Platform
web

React web UI hosted by Tauri in a desktop application. Responsive web previews support reviewing the UI; a native mobile app is not confirmed.

## Stack
Confirmed by user: React frontend; Rust + Tauri desktop/backend; LiveKit for real-time media. TypeScript, Vite and Tailwind are supporting frontend choices for this implementation.

## Product Purpose
Real-time messaging and voice/video communication, including friends, DM, Channels, TEXT/VOICE Rooms and screen sharing. MVP behavior comes from `docs/Dac_ta_he_thong_MVP_v1.docx`.

## Users
People using the desktop app to exchange messages and communicate in real time. A narrower audience and differentiating market position remain undecided.

## Capabilities and Constraints
Username/password login; email-based recovery. DM only between friends; voice/video and screen sharing in VOICE rooms. Authoritative REST/WebSocket server topology and hosting remain open decisions. The current implementation is the standalone Orbit sign-in page in Tauri; the comparison interface remains at `/design`. Authentication and messaging services are not yet connected.

## Brand Commitments
Orbit is the user-approved sign-in direction (2026-11-08), including a floating astronaut. Preserve its plum/lilac space composition in sign-in work.
SpaceLessApp name; Vietnamese interface. Sign-in directions derive from the supplied dark space sample, its astronaut raster, glass inputs and atmospheric horizon. Candidate variations may explore light surfaces and conversation-based expression.

## Evidence on Hand
- Original MVP specification in `docs/` and extracted text alongside it.
- Pasted React reference preserved in `docs/references/`.
- Astronaut extracted without modification from the supplied embedded PNG at `public/assets/astronaut.png`.
- No commercial claims or real user activity supplied.
