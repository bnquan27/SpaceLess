# Design comparison

## Sub-features

- Four direction buttons and previous/next arrows.
- Desktop/mobile controls and expanded preview.
- Save a direction locally and export Markdown notes.

## How to get to it (user POV)

Open web `/design`. The public startup route `/` opens Orbit sign-in.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs web design`. The helper chooses `Signal`, switches to `Xem trên điện thoại`, saves via `Chọn hướng này` and downloads `spaceless-signal.md`. It reloads `/design` to prove persistence, then checks `/` still renders Orbit. When changing navigation, also drive `Hướng thiết kế trước`, `Hướng thiết kế tiếp theo`, `Mở rộng xem trước` and `Thu nhỏ xem trước`. Verify the selected button's `aria-pressed` state and the preview's direction after each action.

## Gotchas

Saving affects the current browser profile's comparison preference. Use a fresh context to test first-load behavior. A 390-pixel browser viewport and the embedded mobile control test different layouts.
