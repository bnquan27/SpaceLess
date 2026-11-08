# Chạy máy chủ Rust SpaceLessApp

Crate `server/` cung cấp REST và WebSocket cho desktop. Xem [hợp đồng API](rust-server-api.md) để tra đường dẫn và [thiết kế](rust-server-design.md) để xem quyền truy cập cùng các giao dịch. PostgreSQL cần có collation ICU `und-x-icu` để xử lý tên tiếng Việt nhất quán.

## Khởi động API cục bộ

Tạo một cơ sở dữ liệu PostgreSQL riêng. Trong PowerShell, đặt `DATABASE_URL` thành URL kết nối của cơ sở dữ liệu đó và tạo bí mật ngẫu nhiên cho phiên local.

```powershell
$env:DATABASE_URL = 'postgres://spaceless@127.0.0.1:5432/spaceless'
$env:APP_JWT_SECRET = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(48))
cargo run --manifest-path server/Cargo.toml --locked
```

Máy chủ áp dụng migration khi khởi động. REST có tiền tố `http://127.0.0.1:8080/api/v1` và WebSocket dùng `ws://127.0.0.1:8080/api/v1/ws`. Dùng `SERVER_BIND` để đổi địa chỉ lắng nghe. API giữ một lease PostgreSQL để chặn replica thứ hai cùng sử dụng outbox. Đặt `SNOWFLAKE_WORKER_ID` từ `0` đến `1023` nếu cần cấu hình worker ID. Khi triển khai, đặt API và cổng tải sau TLS.

## Bật nhà cung cấp

| Phụ thuộc | Biến môi trường |
| --- | --- |
| Khôi phục mật khẩu | `SMTP_HOST`, `SMTP_FROM`, `PASSWORD_RESET_URL`. `SMTP_PORT` mặc định `465`, dùng `587` cho STARTTLS. Đặt cả `SMTP_USERNAME` và `SMTP_PASSWORD` nếu relay cần đăng nhập. |
| Kho đối tượng | `S3_BUCKET`, `AWS_REGION`, và thông tin xác thực qua chuỗi cấu hình chuẩn của AWS SDK. Đặt `S3_ENDPOINT` để dùng kho tương thích S3 có path-style và conditional PUT. |
| Cổng tải tệp | `FILE_GATEWAY_PUBLIC_URL`, `FILE_GATEWAY_SECRET` tối thiểu 32 byte. Cổng và API dùng cùng `DATABASE_URL`, `APP_JWT_SECRET` và `FILE_GATEWAY_SECRET`. |
| Voice | `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `LIVEKIT_CLOUD_REVOCATION=true`. Cấu hình webhook nhà cung cấp trỏ tới `POST /api/v1/webhooks/livekit` với cùng khóa ký. |

Chạy cổng tải sau khi API đã áp dụng migration. Mở một tiến trình riêng với các biến cơ sở dữ liệu và kho tệp ở trên, sau đó đặt `SERVER_MODE=gateway`.

```powershell
$env:SERVER_MODE = 'gateway'
$env:FILE_GATEWAY_BIND = '127.0.0.1:8081'
cargo run --manifest-path server/Cargo.toml --locked
```

Client gửi Bearer khi dùng URL tải do API cấp. Cổng xác minh lại phiên và quyền ở mỗi GET hoặc Range request. Tệp tải lên đi trực tiếp vào kho đối tượng qua URL PUT ký trước.

## Chạy kiểm thử

```powershell
cargo test --manifest-path server/Cargo.toml --locked
pwsh -File server/tests/api-smoke.ps1
```

Dùng `-BaseUrl` nếu API chạy trên địa chỉ khác. Smoke test tạo tài khoản và dữ liệu với tên ngẫu nhiên. Chạy trên cơ sở dữ liệu thử nghiệm. Thêm `-WithFiles` khi API và cổng tải đã cấu hình kho đối tượng. Phép thử tệp kiểm tra PUT có điều kiện, đọc theo Range và URL tải đã cấp bị từ chối sau khi mất quyền.

Thêm `-Psql psql` với `DATABASE_URL` của cơ sở dữ liệu thử nghiệm để kiểm tra reset mật khẩu. Phép thử chèn hash token cho tài khoản vừa tạo và dùng REST để đổi mật khẩu, kiểm tra token chỉ dùng một lần và hai phiên cũ bị thu hồi.

Khi dùng cả `-WithFiles` và `-Psql`, phép thử chờ một chu kỳ worker để xác nhận xóa upload hết hạn và tệp của tin đã xóa. Nó cũng tải lại tệp vẫn còn đính kèm.

Sau khi build, chạy phép thử giao thức voice với `DATABASE_URL` của cơ sở dữ liệu thử nghiệm và cổng API đang trống. Fixture LiveKit chỉ lắng nghe trên localhost. Nó nhận lệnh thu hồi và tạo webhook đã ký để kiểm tra logic phiên trong máy chủ Rust.

```powershell
node server/tests/voice-smoke.mjs server/target/debug/spaceless-server.exe
```

Kiểm thử nghiệm thu LiveKit cần hai client và dự án LiveKit Cloud. Kiểm tra riêng trường hợp token đã được SDK gia hạn, quyền bị thu hồi giữa cuộc gọi và hai track video màn hình được publish gần nhau. Xử lý track thừa qua webhook xảy ra sau lúc SFU báo publish, vì vậy giới hạn tuyệt đối trước lúc gửi track cần chính sách ở SFU. Cổng tải kiểm tra từng request và giữ stream đã mở đến khi truyền xong.
