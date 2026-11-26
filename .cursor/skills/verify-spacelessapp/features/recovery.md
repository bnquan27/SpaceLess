# Password recovery

## Sub-features

- Email input and invalid-format feedback.
- Explicit unsent status for valid email and return to sign-in.

## How to get to it (user POV)

Choose **Quên mật khẩu?** from Orbit sign-in after leaving the sample workspace with **Đăng xuất**. The link also appears in the comparison preview.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop recovery`. The helper submits an invalid email, then a valid synthetic email. It checks the format alert and captures the unsent status in `preview.png`. It then chooses **Quay lại đăng nhập** and checks that the sign-in heading returns.

## Gotchas

Use `.test` email addresses for preview inputs. An SMTP delivery check requires a dedicated relay and an authorized test inbox.
