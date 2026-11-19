# Design comparison

## Sub-features

- Four direction buttons and previous/next arrows.
- Desktop/mobile controls and expanded preview.
- Save a direction locally and export Markdown notes.

## How to get to it (user POV)

Open web `/design`. On `/`, choose **Đăng xuất** from the sample workspace to see the app's Orbit sign-in form.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs web design`. The helper chooses **Signal**, switches to **Xem trên điện thoại**, saves with **Chọn hướng này**, and downloads `spaceless-signal.md`. It checks that Signal survives reload and that the embedded Orbit form shows disconnected sign-in status. It returns to `/`, leaves the sample workspace, and checks that the app form still uses Orbit.

When changing navigation, also use **Hướng thiết kế trước**, **Hướng thiết kế tiếp theo**, **Mở rộng xem trước**, and **Thu nhỏ xem trước**. Check the selected direction button's `aria-pressed` state and the visible preview.

## Gotchas

Saving affects the current browser profile's comparison preference. Use a fresh context to test first-load behavior. A 390-pixel browser viewport and the embedded mobile control test different layouts.
