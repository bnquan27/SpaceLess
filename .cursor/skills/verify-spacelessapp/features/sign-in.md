# Orbit sign-in

## Sub-features

- Empty form validation and focus on the username.
- Password visibility, remember checkbox, and submit by click or Enter.
- Local workspace transition after valid input in the app, or disconnected preview status in the embedded form.

## How to get to it (user POV)

Open the Windows app or web `/` and choose **Đăng xuất** from the sample workspace. On `/design`, select **Orbit** and use the form in **Xem trước trực tiếp**.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop sign-in`. Use `web sign-in 390` for the narrow layout. The helper opens Orbit, checks both empty-field errors and username focus, fills the two fields, toggles password visibility, and submits with Enter. The workspace greets the entered username. Confirm `network` is empty in `result.json`.

Run `web design` with the same helper to check the embedded Orbit form's disconnected sign-in status.

## Gotchas

Use **Tên đăng nhập** and a synthetic password. The app changes its active demo username in memory without checking credentials. To verify authentication, use synthetic accounts in [the API journey](server-api.md).
