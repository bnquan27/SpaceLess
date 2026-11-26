# Backend cho giao diện hiện tại — 26/11/2026

## Trạng thái

Backend đã được khởi tạo và nối vào **chính giao diện phát triển từ `0f31753`**, gồm sửa đổi ẩn thanh kênh khi mở DM từ sảnh. Không thay bằng bố cục `ConnectedWorkspace` của bản thử trước.

Thư mục: `C:\Users\Admin\Documents\ttcs\SpaceLess\SpaceLess-ui-old`. Nhánh: `codex/backend-current-ui`. Commit UI gốc trên GitHub vẫn được giữ nguyên. Phần sửa phản hồi/chuyển tiếp và kết quả hồi quy API mới nhất được ghi trong [reply-forward-fix.md](reply-forward-fix.md).

| Thành phần | Địa chỉ / dữ liệu | Kiểm tra |
| --- | --- | --- |
| Giao diện React/Vite | http://127.0.0.1:1421/ | Build thành công; desktop và 390px |
| Rust REST/WebSocket | http://127.0.0.1:18080/api/v1 | Auth, CORS 1421, socket thật |
| PostgreSQL 17.12 | 127.0.0.1:55432, DB `spaceless_current_ui` | ICU `und-x-icu`, 6 migration đã áp dụng |
| Workspace mẫu | http://127.0.0.1:1421/preview | Giữ dữ liệu mẫu, không giả đăng nhập API |
| SMTP | Chưa cấu hình trong môi trường chạy local | Chưa chứng minh email được giao nhận |
| S3 / gateway | Chưa cấu hình; gateway chưa chạy | Suite kiểm tra bằng fixture giao thức cục bộ |
| LiveKit Cloud | Chưa cấu hình | Join trả dịch vụ không khả dụng; UI không giả đã kết nối |

## Chạy từ terminal VS Code

Trong thư mục `SpaceLess-ui-old`, terminal thứ nhất:

```powershell
npm run backend
```

Terminal thứ hai:

```powershell
npm run ui
```

Mở **http://127.0.0.1:1421/**. Backend `18080` phục vụ JSON, không phục vụ trang HTML. Nếu các tiến trình đang chạy, chỉ cần mở URL. Ctrl+C dừng tiến trình của terminal đó.

Kiểm tra điều kiện local:

```powershell
npm run backend:check
```

Build và chạy unit test Rust khi sửa mã server:

```powershell
npm run backend:build
```

Script build dùng Cargo có sẵn, hoặc bộ Rust GNU/MinGW portable đã chuẩn bị trong `%TEMP%/spaceless-api-tools`. Script chạy API tự mở lại PostgreSQL portable, tạo DB `spaceless_current_ui` nếu chưa tồn tại và áp dụng migration qua server. Nó không xóa dữ liệu hoặc đổi DB của bản thử trước. Chữ ký phiên local được tạo ngẫu nhiên trong bộ nhớ nếu chưa có `APP_JWT_SECRET`; khởi động lại với khóa mới yêu cầu đăng nhập lại.

`.env.local` chỉ chứa địa chỉ public của API/gateway. Nếu chưa có, script backend tạo cấu hình local; mẫu có trong `.env.example`. Không đặt JWT signing key, mật khẩu DB, khóa S3 hoặc LiveKit secret vào biến `VITE_*`.

## Các luồng đã nối trên giao diện

- Orbit đăng ký/đăng nhập qua Rust; không tự vào workspace bằng tên mẫu. JWT chỉ giữ trong bộ nhớ; refresh và logout dùng server.
- Bootstrap tải kênh, phòng, bạn bè, DM, lời mời và voice state. ID/sequence giữ nguyên chuỗi thập phân; so sánh bằng BigInt.
- Tạo kênh và phòng công khai TEXT/VOICE, gửi và chấp nhận lời mời kênh, gia nhập qua mã công khai.
- Tìm chính xác tên đăng nhập, gửi và chấp nhận lời mời kết bạn, mở DM giữa bạn bè. DM tự chuyển chỉ đọc theo trạng thái server.
- Lịch sử tin nhắn phân trang, gửi với `client_id`, xóa tin của mình, reaction, mốc đọc và WebSocket/reconnect. Không polling toàn bộ khi socket khỏe.
- Gửi lại sau lỗi mất phản hồi giữ khóa chống trùng. Tệp được xin presigned URL, gửi lên kho và gắn bằng upload ID; cần cấu hình S3/gateway. Tải xuống gửi Bearer tới gateway đã cấu hình.
- Hồ sơ và avatar dùng dữ liệu thật. Sửa hồ sơ qua PATCH. Không dùng ảnh người mẫu hoặc trạng thái online giả thay cho dữ liệu thiếu của tài khoản thật.
- Voice dùng grant từ Rust và SDK LiveKit, giữ đúng `voice_session_id`/thế hệ khi leave hoặc callback cũ. Mic/camera/chia sẻ chỉ bật khi có kết nối thật. Chưa nghiệm thu media với nhà cung cấp thật.

## Bằng chứng kiểm thử

| Kiểm tra | Kết quả | Bằng chứng |
| --- | --- | --- |
| Build web | Đạt TypeScript + Vite | [web-build.log](qa/backend-init/web-build.log) |
| Rust unit tests | 6/6 | [rust-tests.log](qa/backend-init/rust-tests.log) |
| REST/WS integration | 24/24, 352 request, phủ 52 endpoint + 1 alias | [result.json](qa/backend-init/api-suite/result.json) |
| Điều kiện chạy local | REST auth, CORS, PostgreSQL/ICU/migration đạt | [doctor.log](qa/backend-init/doctor.log) |
| Đăng ký, tạo kênh/phòng, gửi tin bằng UI | Đạt qua API thật | [Phòng chat](qa/backend-init/channel-real-api.png) |
| Kết bạn, lời mời kênh, DM/reaction hai phiên | Đạt, không reload để nhận tin | [DM hai phiên](qa/backend-init/dm-two-sessions.png) |
| Mất phản hồi sau commit | Retry còn đúng 1 tin trên UI | [browser-results.json](qa/backend-init/browser-results.json) |
| Hủy/kết bạn lại | DM khóa/mở nhập đúng, lịch sử giữ nguyên | [DM chỉ đọc](qa/backend-init/dm-read-only.png) |
| Thiếu LiveKit | Báo lỗi, mic không bật, không có participant giả | [Trạng thái voice](qa/backend-init/voice-unconfigured.png) |
| Responsive | Desktop 1440×1000, narrow 390×844, không tràn ngang | [Desktop](qa/backend-init/final-dm-desktop.png), [390px](qa/backend-init/final-dm-narrow.png) |

Playwright sử dụng hai tài khoản kiểm thử riêng, không phải tài khoản cá nhân của người dùng. Kiểm tra mất phản hồi chỉ can thiệp phản hồi HTTP của trình duyệt **sau khi backend thật đã ghi tin**; không thay backend bằng mock. Kiểm tra DM chỉ đọc dùng REST hủy quan hệ hai tài khoản fixture, xác minh UI rồi khôi phục bằng lời mời/accept thật. Không lưu token/mật khẩu vào báo cáo.

Suite server dùng DB rỗng `spaceless_test_current_ui_suite`, tách DB app. S3 và LiveKit của suite là fixture giao thức cục bộ; không tương đương xác minh Cloud, chất lượng media, mail delivery, thử tải hoặc chạy dài ngày. Các tiến trình API/gateway/fixture của suite đã tự đóng; API và DB app được giữ chạy để xem giao diện.

## Phần chưa sẵn sàng

- Cần cấu hình SMTP và kiểm tra hộp thư thật cho recovery. Phản hồi trung tính không chứng minh đã gửi email.
- Cần S3 compatible storage, credential phía server, gateway và CORS kho tệp phù hợp để nghiệm thu upload/download thực.
- Cần `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `LIVEKIT_CLOUD_REVOCATION=true` và webhook tới API; nghiệm thu ít nhất hai client thật. Chưa kiểm tra mic/camera/screen capture/thu hồi token Cloud.
- Trang quản trị phân quyền đầy đủ, sửa phòng riêng/danh sách thành viên, và một số thao tác nâng cao của UI chưa được nối. Các điểm mở trang quản trị mẫu được chặn bằng thông báo thay vì giả thay đổi quyền. Endpoint backend tương ứng đã nằm trong bộ kiểm thử.
- Ghim/lưu/nhắc nhở tin không có endpoint trong hợp đồng hiện tại; không giả đồng bộ các thao tác đó. Tìm tin trong DM hiện chỉ lọc phần lịch sử đã tải.
- Chưa build/kiểm tra Tauri native/MSVC; kết quả Vite/Rust server không phải kết quả desktop bundle.

Đây là môi trường phát triển local. Khi triển khai nhiều người dùng cần DB/auth/network/TLS và nhà cung cấp thực được cấu hình riêng.
