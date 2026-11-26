# SpaceLessApp feature map

| Feature | Entry points | Proof |
| --- | --- | --- |
| [Workspace preview](workspace.md) | Tauri startup, web `/` | Sample server, roles, local messages, voice controls, and screen-share layout |
| [Orbit sign-in](sign-in.md) | `Đăng xuất` from the workspace, embedded `/design` preview | Field errors, focus, password visibility, local transition or embedded preview status, and zero auth requests |
| [Orbit sign-up](sign-up.md) | `Tạo tài khoản` on Orbit sign-in, including the embedded preview | Six fields, validation, and account preview status |
| [Password recovery](recovery.md) | `Quên mật khẩu?` on Orbit sign-in, including the embedded preview | Email validation, unsent status, and return to sign-in |
| [Design comparison](design.md) | Web `/design` | Select, switch viewport, save, reload, and download notes. The form reached from the workspace still uses Orbit. |
| [Server API](server-api.md) | REST `/api/v1`, authenticated WebSocket | Session, room permissions, messages, media links and voice state through the service |

The UI helper covers the workspace, the standalone forms, the comparison controls, and embedded sign-in. Drive embedded sign-up and recovery separately when a change affects `/design`.
