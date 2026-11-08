# Password recovery

## Sub-features

- Email input and invalid-format feedback.
- Explicit unsent status for valid email and return to sign-in.

## How to get to it (user POV)

Choose `Quên mật khẩu?` from startup sign-in or its comparison preview.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop recovery`. The helper fills `Email` with invalid and then synthetic valid values and presses `Gửi yêu cầu khôi phục` for both. Expect the format alert, then a status saying the recovery request was not sent. For return navigation, choose `Quay lại đăng nhập` and verify the `Tên đăng nhập` field is back.

## Gotchas

Use `.test` email addresses for preview inputs. An SMTP delivery check requires a dedicated relay and an authorized test inbox.
