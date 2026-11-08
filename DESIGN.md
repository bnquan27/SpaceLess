# SpaceLessApp design candidates

The user approved **Orbit** on 2026-11-08, including a floating astronaut animation. Orbit is the committed sign-in direction; other variants remain comparison references. The Impeccable context/seed launcher was unavailable because its engine could not initialize in the session; no generated assignment is claimed.

## Shared visual language
Self-hosted Be Vietnam Pro, Vietnamese copy, Lucide outline icons, a geometric conversation/orbit brand mark, and clear task-focused forms. Use CSS geometry for stars, orbital paths and horizon. Use the supplied original astronaut raster, not a substitute illustration. The studio shell is charcoal and muted lilac so candidates retain their own palette.

## Candidate directions
| Direction | Ground and accent | Composition | Intent and tradeoff |
| --- | --- | --- | --- |
| Orbit | Plum #1c172c, lilac #c8b2f1 | Space illustration left, form right | Warm and atmospheric; illustration yields to form on narrow screens. |
| Deep Space | Navy #08111b, mint #a2d9d3, peach horizon | Centered glass form, astronaut off-axis | Closest to supplied reference; decorative space needs more room. |
| Signal | Forest #243d30, lime #d2ed9a | Form left, conversation and voice rhythm right | Makes product purpose explicit; less literal space imagery. |
| Daylight | Warm paper #faf8f3, clay #a14b35 | Light two-panel composition | Daytime comfort; less immersive than the original night sky. |

## Motion accessibility
When the system requests reduced motion, start static but keep the Play control visible. Explicit Play opts into astronaut, beacon and shooting-star animation for the current component session; Pause returns to static. Offscreen and hidden-tab suspension still applies. Do not hide the only way to enable motion.

## Reusable decisions
- Labels stay visible; username rather than email in login, email for recovery.
- Native checkbox and form validation, password visibility toggle, inline response text, disabled/loading and rejected-auth states.
- Candidate switching preserves form input and does not pretend to navigate into an authenticated application.
- Orbit's astronaut floats over an 8-second seamless loop (22px vertical travel, 6-degree tilt); its companion star pulses over 4 seconds. Keep form controls stationary. Provide a pause/resume button, stop loops offscreen or in hidden tabs, and show a static composition under prefers-reduced-motion. Narrow layouts retain the form-first composition without the astronaut.
- Component breakpoints use container queries so mobile preview works inside the desktop chooser.
- Use self-hosted assets; no remote fonts or images at runtime.
- A selection persists only its direction ID on the current device. Downloadable Markdown records the candidate for later implementation; it does not edit the project brand rules.
