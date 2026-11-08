# Kiến trúc SpaceLessApp

Stack người dùng xác nhận: React frontend, Rust + Tauri desktop/backend và LiveKit cho media thời gian thực.

## Cấu trúc hiện tại

`src/` chứa giao diện React + TypeScript. `src/components/ui/space-login.tsx` là component đăng nhập có bốn hướng thiết kế; `src/components/sign-in-explorer.tsx` là bộ chọn. `src-tauri/` chứa shell Rust + Tauri 2, cấu hình cửa sổ và quyền tối thiểu; chưa có nghiệp vụ backend.

Form nhận callback `onSubmit({ username, password, rememberMe })`. Không cung cấp callback thì chỉ kiểm tra form và hiện thông báo xem thử, không giả lập phiên đăng nhập. Không lưu thông tin xác thực. Bộ chọn chỉ lưu ID hướng thiết kế trong localStorage, có xử lý trường hợp lưu trữ bị chặn.

`server/` chứa dịch vụ Axum và PostgreSQL, tách khỏi tiến trình desktop. Xem [cách chạy máy chủ](rust-server-run.md) và [hợp đồng API](rust-server-api.md).

## Ranh giới giữa desktop và dịch vụ

- React: form, view, state hiển thị, trạng thái mạng và media.
- Rust + Tauri: native integration và logic backend được phân công; không đưa khóa máy chủ vào ứng dụng phân phối.
- Dịch vụ REST `/api/v1` và WebSocket dùng chung xử lý danh tính, quyền, dữ liệu, giao dịch và sự kiện. Một tiến trình API sở hữu outbox của cơ sở dữ liệu.
- LiveKit: WebRTC âm thanh/video/chia sẻ màn hình trong phòng VOICE. Chỉ kết nối sau khi máy chủ kiểm tra quyền và cấp token ngắn hạn. Token signing cần nằm trên máy chủ tin cậy. Chưa cài SDK media vào bản comp đăng nhập vì chưa có phần gọi.
- Kho đối tượng nhận tải lên qua URL ký trước. Cổng tải riêng kiểm tra lại quyền cho mỗi yêu cầu tải.

Sơ đồ luồng dự kiến: React → Tauri/Rust + REST service; service → WebSocket → client. Media client ↔ LiveKit, với token do service cấp. Điểm phân chia HTTP/WebSocket giữa React và Rust cần quyết định trước khi triển khai chức năng.

## Nguồn kỹ thuật

[Tauri 2 overview](https://v2.tauri.app/start/) mô tả frontend web kết hợp Rust/native backend. [LiveKit authentication](https://docs.livekit.io/home/get-started/authentication/) mô tả access token cho media. Áp dụng theo đặc tả MVP, không mặc nhiên thêm tính năng voice agent/AI.
