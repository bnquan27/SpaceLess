# Thử đăng nhập Google trên Windows

## Cấu hình Google và máy chủ Rust

1. Trong Google Auth Platform, cấu hình màn hình đồng ý. Nếu ứng dụng ở chế độ **Testing**, thêm tài khoản Google dùng để thử vào danh sách **Test users**.
2. Tạo OAuth client loại **Desktop app**. Windows mở trình duyệt mặc định và nhận phản hồi tại cổng tạm trên `127.0.0.1`.
3. Đặt `GOOGLE_OAUTH_CLIENT_ID` và `GOOGLE_OAUTH_CLIENT_SECRET` trong môi trường của máy chủ Rust. Giữ client secret trên máy chủ. Dùng một cơ sở dữ liệu thử riêng và khởi động API để SQLx áp dụng migration.

[Google OAuth 2.0 cho ứng dụng desktop](https://developers.google.com/identity/protocols/oauth2/native-app) · [Cách chạy máy chủ Rust](rust-server-run.md)

## Chạy ứng dụng

Trong một terminal khác, đặt địa chỉ API và client ID công khai của cùng OAuth client trước khi chạy Tauri.

```powershell
$env:VITE_SPACELESS_API_URL = 'http://127.0.0.1:8080/api/v1'
$env:VITE_GOOGLE_CLIENT_ID = '<Google Desktop client ID>'
npm run tauri -- dev
```

Với máy chủ từ xa, dùng URL HTTPS kết thúc bằng `/api/v1`. Đặt cùng hai biến khi chạy `npm run tauri -- build --no-bundle` để thử bản build Windows. Bản xem trước thiết kế ở `/design` dành cho kiểm tra giao diện.

Chọn **Đăng nhập bằng Google** trên màn hình Orbit và hoàn tất xác thực trong trình duyệt. Sau khi API xác nhận hồ sơ, SpaceLessApp yêu cầu hệ điều hành đưa cửa sổ đang mở ra trước. Nếu vẫn ở trình duyệt, chọn **Mở SpaceLessApp** trên trang kết quả. Đăng xuất và thử lại với cùng tài khoản Google để kiểm tra API cấp phiên mới cho cùng một tài khoản.

Lần đầu, máy chủ dùng email đã xác minh và tên Google khi có, rồi sinh tên đăng nhập SpaceLess. Ngày sinh và số điện thoại để trống. Tài khoản chỉ dùng Google tiếp tục đăng nhập qua Google. Nếu email đã thuộc tài khoản SpaceLess khác, hãy dùng phương thức đăng nhập hiện có của tài khoản đó.

## Dùng Supabase cho môi trường staging

Supabase phù hợp khi cần một PostgreSQL dùng chung cho staging. Giữ Rust làm máy chủ API và nơi cấp phiên SpaceLess. Dùng một project staging riêng và tắt **Data API** trước khi áp dụng migration.

1. Lấy chuỗi kết nối PostgreSQL từ màn hình **Connect**. Dùng kết nối trực tiếp nếu máy chủ hỗ trợ IPv6, hoặc **Session pooler** cổng `5432` nếu máy chủ chỉ có IPv4.
2. Kiểm tra PostgreSQL có collation `und-x-icu` và vai trò chạy migration có quyền tạo bảng, chỉ mục, và hàm. Giữ TLS khi kết nối từ máy chủ đến Supabase.
3. Đặt chuỗi kết nối làm `DATABASE_URL` trên máy chủ Rust. Nếu dùng Supabase MCP để kiểm tra schema, giới hạn MCP vào project staging và bật chế độ chỉ đọc.

Máy chủ Rust giữ một PostgreSQL advisory lock trong suốt vòng đời của kết nối. Dùng kết nối trực tiếp hoặc **Session pooler** để giữ khóa đó qua các giao dịch.

[Kết nối PostgreSQL trên Supabase](https://supabase.com/docs/guides/database/connecting-to-postgres) · [Tắt Data API](https://supabase.com/docs/guides/api/securing-your-api) · [Supabase MCP](https://supabase.com/docs/guides/ai-tools/mcp)
