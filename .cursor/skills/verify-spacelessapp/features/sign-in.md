# Orbit sign-in

## Sub-features

- Empty form validation and focus on the username.
- Password visibility, remember checkbox, and submit by click or Enter.
- Preview status after valid input without a request or persisted credentials.

## How to get to it (user POV)

Open the Windows app or web `/`. On `/design`, select Orbit and use the form in `Xem trước trực tiếp`.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop sign-in`. Run the same command with `web sign-in 390` to check the narrow layout. The helper clicks `Đăng nhập`, observes both field errors and username focus, fills the two labeled fields, toggles password visibility and submits with Enter. The final `role=status` says login is disconnected. Confirm `network` is empty in `result.json`. To check the embedded entry, scope those controls to the preview's `Đăng nhập SpaceLessApp` region and perform the same sequence.

## Gotchas

Use `Tên đăng nhập`, which is distinct from the recovery `Email` field. Treat the preview notice as proof of form behavior. Use synthetic test accounts for [the API journey](server-api.md).
