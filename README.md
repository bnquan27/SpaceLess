# SpaceLessApp

Trang đăng nhập Orbit cho ứng dụng nhắn tin, đàm thoại thời gian thực. Giao diện tiếng Việt, stack **React + TypeScript / Rust + Tauri / LiveKit**. App mở trực tiếp vào đăng nhập với phi hành gia floating; xác thực thật chưa được kết nối.

## Chạy giao diện

```powershell
npm install
npm run dev
```

Mở [trang đăng nhập](http://127.0.0.1:1420). Bộ chọn thiết kế được giữ tại [phòng thiết kế](http://127.0.0.1:1420/design) để tham khảo; lựa chọn ở đó không thay đổi hướng Orbit đã chốt của app.

```powershell
npm run build
npm run preview
```

## Chạy desktop

Cài [điều kiện phát triển Tauri trên Windows](https://v2.tauri.app/start/prerequisites/) gồm Rust, C++ Build Tools và WebView2, rồi chạy:

```powershell
npm run desktop:dev
npm run desktop:build
```

`src-tauri/` là shell desktop; backend xác thực và LiveKit chưa kết nối. Vite build kiểm tra frontend, không thay cho biên dịch Rust/Tauri.

Sau `npm run desktop:build`, mở trực tiếp `src-tauri/target/release/spaceless-app.exe` để kiểm tra. Bộ cài Windows nằm tại `src-tauri/target/release/bundle/nsis/SpaceLessApp_0.1.0_x64-setup.exe` và `src-tauri/target/release/bundle/msi/SpaceLessApp_0.1.0_x64_en-US.msi`.

## Cấu trúc

- `src/components/ui/space-login.tsx`: component độc lập với prop `direction` và callback `onSubmit`.
- `src/components/sign-in-explorer.tsx`: giao diện chọn thiết kế, lưu lựa chọn, tải ghi chú.
- `src/index.css`: styles và tokens theo từng biến thể; font được self-host trong bundle.
- `src/lib/utils.ts`: helper `cn`; `components.json` cấu hình đường dẫn shadcn + Tailwind 4.
- `src-tauri/`: shell Rust/Tauri 2.
- `docs/`: đặc tả gốc và tài liệu dự án.
- `AGENTS.md`, `PRODUCT.md`, `DESIGN.md`: quy tắc agent, thông tin sản phẩm và định hướng thị giác.

Thêm component shadcn bằng `npx shadcn@latest add <component>` khi cần. Thư mục chuẩn của dự án là `src/components/ui/` qua alias `@/components/ui`, không cần tạo thêm thư mục `/components/ui` trùng lặp.

## Phạm vi

Trang đăng nhập và đăng ký Orbit trong app desktop cho phép kiểm tra giao diện và thông tin nhập. Các form chưa gửi dữ liệu mạng; khôi phục chưa gửi mail và đăng ký chưa tạo tài khoản. Component đăng nhập hỗ trợ callback bất đồng bộ, loading và lỗi để nối xác thực thật sau. LiveKit sẽ dùng cho phòng VOICE theo đặc tả; chưa có tính năng gọi trong bản này.

Tài liệu hệ thống: `docs/Dac_ta_he_thong_MVP_v1.docx`. Đăng nhập theo đặc tả dùng tên đăng nhập + mật khẩu, dù mẫu tham khảo ban đầu dùng email.
