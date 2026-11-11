# Orbit sign-up

## Sub-features

- Six labeled fields, required-value feedback, email format, birth date and minimum password checks.
- Preview status for valid input and return to login via both available buttons.

## How to get to it (user POV)

Choose `Tạo tài khoản` below `Đăng nhập` on the startup screen or in the comparison preview.

## Driving it with Playwright

Run `node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop sign-up`. The helper checks the required name, invalid email, future birth date and short password. It corrects the inputs, asserts `tài khoản chưa được tạo` and checks that the form made no request. For a targeted navigation check, use both `Quay lại đăng nhập` and the `Đăng nhập` button inside `Đã có tài khoản?`, reopening sign-up between them. Expect `Kết nối lại nhé.` each time.

## Gotchas

The `Tạo tài khoản` entry button and form submit share a name and appear in different modes. Date inputs take `YYYY-MM-DD`. Test account creation separately through the API.
