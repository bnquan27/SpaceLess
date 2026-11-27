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

## Nâng cấp cơ sở dữ liệu lên MVP v1.1

1. Sao lưu cơ sở dữ liệu và thử nâng cấp trên bản sao.
2. Dừng API và cổng tải đang dùng schema v1.
3. Khởi động API bản mới với `DATABASE_URL` của cơ sở dữ liệu cần nâng cấp. API áp dụng các migration `0003` đến `0006` trước khi báo sẵn sàng.
4. Khởi động lại cổng tải bằng cùng bản build sau khi API báo `SpaceLess server ready`.

Giữ nguyên `APP_JWT_SECRET` để các JWT v1 còn hạn tiếp tục dùng được. Lần refresh tiếp theo cấp JWT mới và thu hồi JWT cũ. Hạn mức khôi phục bắt đầu lại sau khi nâng cấp. Với tin đã sửa hoặc đã xóa trong schema v1, migration giữ khóa gửi còn lưu và dùng trạng thái hiện có khi xử lý retry vì dữ liệu gửi gốc có thể đã mất.

## Triển khai API staging trên Cloudflare

[Worker staging](https://spaceless-staging-api.whgaming200.workers.dev) dùng Hyperdrive để kết nối PostgreSQL của Supabase. Vai trò PostgreSQL của Worker chỉ có quyền cần cho các route staging. Worker dùng schema hiện có và không tự chạy migration.

Trước khi triển khai Worker có giới hạn gửi tin, áp dụng migration `server/migrations/0008_message_send_windows.sql` và cập nhật quyền bằng `server/cloudflare-api/worker-role.sql`.

Bản staging phục vụ đăng nhập bằng mật khẩu và Google, làm mới phiên, đăng xuất, hồ sơ, bootstrap, kênh, bạn bè, nhắn tin, WebSocket, tệp trên R2 và phòng thoại LiveKit Cloud. Các route của staging yêu cầu header `X-SpaceLess-Staging-Token`. Webhook LiveKit xác thực bằng chữ ký và hash của nội dung. Giữ giá trị header staging trong môi trường kiểm thử đáng tin cậy. Ứng dụng desktop và frontend hiện dùng API riêng.

1. Cài `cf`, `worker-build`, Rust target `wasm32-unknown-unknown`, và PowerShell 7. Đăng nhập `cf` vào tài khoản Workers Free.
2. Nạp `STAGING_GATE_TOKEN` và `APP_JWT_SECRET` từ nơi lưu bí mật. Giữ nguyên hai giá trị khi cập nhật Worker staging hiện có.
3. Tại thư mục gốc của repo, chạy `./server/cloudflare-api/deploy.ps1 -HyperdriveId <id>` với cấu hình Hyperdrive staging. Script chạy dry run trước khi tải bản build lên.
4. Dùng `verify-auth.mjs` và `verify-messages.mjs` trong `server/cloudflare-api/` để kiểm tra API đã triển khai. Hai script tạo và dọn dữ liệu thử riêng. Nếu route vừa thêm trả `404` ngay sau khi cập nhật, đợi Durable Object nhận phiên bản mới rồi chạy lại. [Cách Durable Object nhận bản cập nhật](https://developers.cloudflare.com/durable-objects/deployments/durable-objects-code-updates/)

`verify-r2-staging.mjs` kiểm tra tải tệp thật, đọc theo `Range` và thu hồi quyền ngay cả khi URL tải đã được cấp. `verify-livekit-cloud.mjs` dùng hai máy khách RTC để kiểm tra âm thanh hai chiều, video chia sẻ màn hình và sự kiện bắt đầu, dừng chia sẻ qua WebSocket. Chạy hai script này với dữ liệu thử trên staging.

Dành Snowflake worker ID `1023` cho Worker staging. Worker thêm dấu `cloudflare_staging` vào sự kiện tin nhắn do nó tạo. Durable Object gửi sự kiện đó qua WebSocket ngay sau khi giao dịch hoàn tất. Cron chạy mỗi phút để thử lại sự kiện còn chờ. Bản Rust native trong repo bỏ qua các sự kiện có dấu này khi đọc cùng cơ sở dữ liệu.

Workers Free có hạn mức riêng cho lượt gọi Worker, Durable Objects và câu lệnh qua Hyperdrive. Khi vượt hạn mức, thao tác tương ứng có thể bị từ chối. [Giới hạn Workers](https://developers.cloudflare.com/workers/platform/limits/) · [Giá Hyperdrive](https://developers.cloudflare.com/hyperdrive/platform/pricing/) · [Giá Durable Objects](https://developers.cloudflare.com/durable-objects/platform/pricing/)

### Kiểm thử bản Worker cục bộ

Build Worker bằng `worker-build --release` trong `server/cloudflare-api`. Dùng Node hỗ trợ `crypto.argon2Sync`. Đặt `SPACELESS_TEST_TOOLS` thành đường dẫn tuyệt đối tới thư mục `node_modules` có `miniflare`, `pg` và `ws`. Đặt `SPACELESS_TEST_POSTGRES` thành URL tới cơ sở dữ liệu `postgres` trên một cụm PostgreSQL thử nghiệm ở `127.0.0.1`.

Tại thư mục gốc, chạy `node server/cloudflare-api/verify-local.mjs`. Script tạo một cơ sở dữ liệu `spaceless_test_worker_*`, nạp schema, rồi dọn cơ sở dữ liệu thử khi kết thúc. Miniflare chạy cùng WebAssembly dùng để deploy. Bộ thử dùng PostgreSQL thật tại máy, R2 mô phỏng và nhà cung cấp OAuth và LiveKit mô phỏng.

### Cấu hình nhà cung cấp cho bản Worker mới

- **Google.** Nạp `GOOGLE_OAUTH_CLIENT_ID` và `GOOGLE_OAUTH_CLIENT_SECRET` của ứng dụng desktop staging từ nơi lưu bí mật.
- **R2.** Chọn bucket Standard riêng tư. Nạp `R2_ACCOUNT_ID`, `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` và `FILE_GATEWAY_SECRET`. Chỉ bật subscription R2 sau khi duyệt cách tính phí vượt hạn mức. [Giá R2](https://developers.cloudflare.com/r2/pricing/)
- **LiveKit Cloud.** Nạp `LIVEKIT_URL`, `LIVEKIT_API_KEY` và `LIVEKIT_API_SECRET` của dự án staging. Đặt webhook của dự án trỏ tới `/api/v1/webhooks/livekit`. Worker kiểm tra chữ ký và hash của nội dung webhook bằng khóa LiveKit.

Dùng `-DryRun` với `deploy.ps1` để kiểm tra gói triển khai. Trước khi cập nhật staging, dùng `server/cloudflare-api/worker-role.sql` với `psql -v worker_role=<vai-trò-Hyperdrive-staging>` trên đúng cơ sở dữ liệu Supabase staging. Script cấp quyền cho vai trò đã có và giữ nguyên schema.

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

Client gửi Bearer khi dùng URL tải do API cấp. Cổng xác minh lại phiên và quyền ở mỗi GET hoặc Range request. URL PUT ký trước ghi vào khóa staging. API chốt tệp vào khóa riêng và xác minh bản chốt trước khi gắn. Tài khoản truy cập kho của API cần quyền đọc, ghi và xóa các object dưới tiền tố upload.

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
