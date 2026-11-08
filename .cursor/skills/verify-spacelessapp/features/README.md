# SpaceLessApp feature map

| Feature | Entry points | Proof |
| --- | --- | --- |
| [Orbit sign-in](sign-in.md) | Tauri startup, web `/`, embedded `/design` preview | Field errors, focus, password visibility, preview notice and zero auth requests |
| [Orbit sign-up](sign-up.md) | `Tạo tài khoản` on sign-in, including the embedded preview | Six fields, validation and explicit account preview notice |
| [Password recovery](recovery.md) | `Quên mật khẩu?` on sign-in, including the embedded preview | Email validation and explicit unsent notice |
| [Design comparison](design.md) | Web `/design` | Select, switch viewport, save, reload and download notes. `/` still loads Orbit. |
| [Server API](server-api.md) | REST `/api/v1`, authenticated WebSocket | Session, room permissions, messages, media links and voice state through the service |

The UI helper covers the standalone forms and the comparison controls. Drive the embedded form entry points separately when a change affects `/design`.
