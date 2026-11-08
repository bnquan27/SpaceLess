# Hợp đồng API máy chủ Rust MVP v1

Đường dẫn REST dưới đây dùng tiền tố `/api/v1`. Bảng chính giữ method và path của §4 trong [đặc tả MVP v1](Dac_ta_he_thong_MVP_v1.docx). Các route chi tiết của category và room khai triển cách ghi rút gọn trong đặc tả. Payload, mã lỗi và các route được ghi là đề xuất dùng để thống nhất với desktop trước khi triển khai. Quyền, giao dịch và worker được mô tả trong [thiết kế máy chủ Rust](rust-server-design.md).

## Quy ước giao thức

- Gửi JWT ứng dụng qua `Authorization: Bearer <token>` cho route bảo vệ. Actor và quyền lấy từ phiên đã xác minh và dữ liệu hiện tại. Các ID user trong URL hoặc body chỉ là đối tượng của thao tác.
- Snowflake là chuỗi thập phân dương vừa `BIGINT`, kể cả trong path, query và JWT `sub`. `room_sequence` và `last_read_sequence` là bộ đếm theo Phòng. Đề xuất mã hóa cả sequence dưới dạng chuỗi thập phân để giữ đủ độ chính xác. Ngày dùng `YYYY-MM-DD`, thời điểm dùng RFC 3339 UTC.
- `GET` dùng path/query. `POST`, `PUT`, `PATCH` dùng JSON. `DELETE` dùng path/query. Từ chối trường lạ. Với `PATCH`, trường vắng giữ nguyên và `null` xóa giá trị nullable, ví dụ `category_id`.
- DTO hồ sơ công khai chỉ gồm `user_id`, `username`, `full_name`, `avatar_url` và trạng thái quan hệ phù hợp với người xem. Chỉ route hồ sơ chính chủ trả dữ liệu riêng. DTO và event không chứa hash, `storage_key`, `session_jti` hay chiều chặn của người khác.
- Đề xuất trả `201` khi tạo tài nguyên, `200` khi trả dữ liệu hoặc trả lại tài nguyên đã có, `204` khi thành công không có body. Lỗi JSON có `error.code`, `error.message` và `request_id`. `400` cho schema sai, `401` cho phiên không hợp lệ, `403` cho thao tác thiếu quyền trên tài nguyên đã thấy, `404` cho tài nguyên không tồn tại hoặc bị ẩn, `409` cho xung đột trạng thái, `429` kèm `Retry-After` khi bị giới hạn và `503` khi phụ thuộc bắt buộc tạm lỗi. Lỗi kết bạn, mời hoặc mở DM không tiết lộ bên chặn.

## REST theo đặc tả

### Tài khoản và hồ sơ (§4.1)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `POST /auth/register` | `username`, `password`, `full_name`, `date_of_birth`, `email`, `phone` | Công khai. Tạo hồ sơ chính chủ, ngày sinh không ở tương lai. |
| `POST /auth/login` | `username`, `password` | Công khai. Trả `access_token`, `expires_at`. Sai tài khoản hoặc mật khẩu dùng chung thông báo. Chỉ báo khóa sau khi xác minh mật khẩu. |
| `POST /auth/refresh` | Bearer của phiên còn hiệu lực | Trả JWT và hạn mới của cùng phiên logic. |
| `POST /auth/logout` | Bearer của phiên | Thu hồi đúng phiên, đóng WS và media mà phiên sở hữu. |
| `POST /auth/forgot-password` | `email` | Công khai, rate-limit. Trả thông báo trung tính. |
| `POST /auth/reset-password` | `token`, `new_password` | Công khai với reset token còn hiệu lực. Đổi mật khẩu và thu hồi toàn bộ phiên. |
| `GET /users/me` | Bearer | Hồ sơ chính chủ. |
| `PATCH /users/me` | `full_name`, `date_of_birth`, `phone`, `email`, `address`, `avatar_upload_id` | Hồ sơ sau cập nhật. Username cố định. Đổi email vô hiệu hóa token reset chưa dùng. |

### Bạn bè, chặn và DM (§4.2)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `GET /users/search` | `q`, `cursor?`, `limit?` | Trang hồ sơ công khai theo người xem, không trả dữ liệu riêng. |
| `POST /friends/requests` | `target_user_id` | Tạo yêu cầu `PENDING` nếu hai tài khoản đủ điều kiện. |
| `PATCH /friends/requests/{request_id}` | `decision: ACCEPT` hoặc `DECLINE` | Chỉ người nhận xử lý yêu cầu `PENDING`. |
| `DELETE /friends/{target_user_id}` | Path | Hủy kết bạn idempotent, hủy lời mời Kênh liên quan và chuyển DM sang chỉ đọc. |
| `POST /blocks` | `target_user_id` | Chặn theo chiều actor, xóa quan hệ và hủy lời mời liên quan. Gọi lại cùng chiều trả thành công. |
| `DELETE /blocks/{target_user_id}` | Path | Gỡ chiều actor. Quan hệ bạn bè phải được thiết lập lại qua luồng kết bạn. |
| `POST /dms` | `target_user_id` | Tạo hoặc trả DM cũ của cặp đang là bạn và không chặn nhau. DM chỉ đọc vẫn xuất hiện trong bootstrap. |

### Kênh, lời mời và thành viên (§4.3)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `POST /channels` | `channel_name`, `invitee_ids?` | Tạo Kênh `INVITE_ONLY`, actor là Creator. Trả Kênh và kết quả từng lời mời. |
| `PATCH /channels/{channel_id}` | `channel_name`, `icon_upload_id`, `background_upload_id`, `description`, `join_mode` | Creator/Moderator. Giữ nguyên mã mời, chuyển chế độ không hủy lời mời đang chờ. |
| `DELETE /channels/{channel_id}` | Path | Creator xóa mềm, hủy lời mời chờ và thu hồi voice. |
| `POST /channels/{channel_id}/invitations` | `target_user_id` | Creator/Moderator mời bạn đủ điều kiện. Mỗi người nhận có tối đa một lời mời `PENDING` trong Kênh. |
| `PATCH /invitations/{invitation_id}` | `decision: ACCEPT` hoặc `DECLINE` | Người nhận phản hồi. `ACCEPT` kiểm tra lại quan hệ, quyền của người mời và Kênh. Nếu điều kiện mất, commit `CANCELLED` và trả xung đột. |
| `GET /channel-links/{invite_code}` | Path | Xem trước Kênh khả dụng ở `PUBLIC_LINK`. Đề xuất yêu cầu đăng nhập và chỉ trả dữ liệu xem trước. |
| `POST /channel-links/{invite_code}/join` | Path | Gia nhập Kênh đang bật `PUBLIC_LINK`. Nếu đã là thành viên thì trả thành công idempotent. |
| `DELETE /channels/{channel_id}/members/{user_id}` | Path | Creator khai trừ Moderator hoặc Member. Moderator chỉ khai trừ Member. |
| `DELETE /channels/{channel_id}/members/me` | Path | Tự rời. Creator phải nhượng quyền trước. |
| `PATCH /channels/{channel_id}/members/{user_id}/role` | `role: MODERATOR` hoặc `MEMBER` | Chỉ Creator đổi quyền người khác trong Kênh, hủy lời mời còn chờ khi hạ Moderator. |
| `POST /channels/{channel_id}/transfer-ownership` | `target_user_id` | Creator chuyển cho thành viên hoạt động, Creator cũ thành Moderator trong cùng giao dịch. |

Mã mời công khai chỉ hiện với Creator/Moderator trong payload Kênh phù hợp. Khi rời hoặc bị khai trừ, server xóa membership Kênh/Phòng riêng, hủy lời mời còn chờ do người đó gửi và ngắt voice thuộc Kênh. Vào lại nhận vai trò `MEMBER` mới và mốc đọc ít nhất bằng sequence hiện tại. Việc chặn một người không ảnh hưởng tới việc gia nhập qua liên kết công khai hoặc đọc tin nhắn ở Phòng chung.

### Nhóm Phòng và Phòng (§4.3)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `POST /channels/{channel_id}/categories` | `category_name`, `position?` | Creator/Moderator tạo nhóm. |
| `PATCH /channels/{channel_id}/categories/{category_id}` | `category_name`, `position` | Creator/Moderator đổi tên hoặc vị trí. |
| `DELETE /channels/{channel_id}/categories/{category_id}` | Path | Creator/Moderator xóa cứng nhóm, chuyển Phòng về cuối danh sách không nhóm. |
| `PUT /channels/{channel_id}/categories/order` | `category_ids` | Creator/Moderator gửi danh sách đầy đủ, mỗi ID hợp lệ xuất hiện đúng một lần. |
| `POST /channels/{channel_id}/rooms` | `room_name`, `type: TEXT` hoặc `VOICE`, `is_private`, `category_id?`, `position?`, `member_ids?` | Creator/Moderator tạo Phòng. Phòng TEXT khởi tạo read state cho người có quyền xem. |
| `PATCH /channels/{channel_id}/rooms/{room_id}` | `room_name`, `is_private`, `category_id`, `position`, `member_ids`, `room_mod_ids` | Creator/Moderator sửa tất cả trường trong bảng. Mod Phòng riêng chỉ sửa tên và danh sách Member thường của đúng Phòng. |
| `DELETE /channels/{channel_id}/rooms/{room_id}` | Path | Creator/Moderator xóa mềm và thu hồi quyền/media. |
| `PUT /channels/{channel_id}/rooms/order` | Đề xuất `groups: [{category_id, room_ids}]`, gồm nhóm `category_id: null` | Creator/Moderator sắp xếp Phòng khả dụng. Mỗi Phòng xuất hiện một lần trong nhóm hiện tại. Dùng PATCH để đổi nhóm. |

Phòng công khai dành cho thành viên Kênh. Phòng riêng dành cho Creator, Moderator và thành viên được gán. Khi chuyển sang công khai, xóa các `Room_member` của Phòng. Khi chuyển sang riêng tư, yêu cầu danh sách thành viên mới. Mod Phòng riêng không tự xóa quyền của mình hoặc xóa Mod khác. Đổi vai trò, membership hoặc tính riêng tư phải tính chênh lệch quyền trước và sau giao dịch để gửi `ROOM_CREATE` hoặc `ROOM_DELETE` đúng người.

### Tin nhắn, tệp, reaction và mốc đọc (§4.4)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `POST /rooms/{room_id}/messages` | `client_id`, `content`, `upload_ids?` | Quyền ghi TEXT/DM. Trả tin chính thức. Retry cùng khóa dedup trả tin đã tạo. |
| `GET /rooms/{room_id}/messages` | `before_sequence?` hoặc `after_sequence?`, `limit?`, `through_sequence?` | Quyền đọc, gồm DM chỉ đọc. Trang tin, tổng hợp reaction, reaction của actor và mốc sequence. `through_sequence` là mốc chụp đề xuất cho các trang tiếp theo khi bắt kịp. |
| `PATCH /rooms/{room_id}/messages/{message_id}` | `content` | Chỉ tác giả có quyền ghi. Giữ sequence, trả nội dung và `time_edited`. |
| `DELETE /rooms/{room_id}/messages/{message_id}` | Path | Tác giả có quyền ghi hoặc người quản trị đúng phạm vi. Trong DM chỉ tác giả có quyền ghi. |
| `POST /rooms/{room_id}/uploads/presign` | `file_name`, `mime_type`, `file_size_bytes` | Quyền ghi. Trả `upload_id`, `put_url`, `required_headers`, hạn URL và hạn STAGED. |
| `GET /attachments/{attachment_id}/url` | Path | Quyền đọc hiện tại. Trả `url`, `expires_at` cho tệp của tin còn tồn tại. |
| `PUT /rooms/{room_id}/messages/{message_id}/reactions/me` | `reaction_type` | Quyền ghi/tương tác. Thêm hoặc thay reaction của actor. |
| `DELETE /rooms/{room_id}/messages/{message_id}/reactions/me` | Path | Quyền ghi/tương tác. Gỡ reaction của actor nếu có. |
| `PUT /rooms/{room_id}/read-state` | `last_read_sequence` | Quyền đọc, gồm DM chỉ đọc. Trả mốc đơn điệu và `last_read_at`. |

Upload tin nhắn chấp nhận PNG, JPEG, WebP, PDF, TXT, CSV, DOCX, XLSX, PPTX và ZIP theo giới hạn trong thiết kế. Tệp phải thuộc người gửi và đúng Phòng, còn hạn, đã PUT thành công và còn STAGED. DM chỉ đọc vẫn cho xem lịch sử, xin quyền tải tệp và đánh dấu đã đọc. Client gửi Bearer khi dùng URL của cổng tải tệp. VOICE không có các thao tác tin nhắn trong MVP.

Ví dụ response của `POST /rooms/{room_id}/messages`. ID và sequence minh họa cách mã hóa trên wire.

```json
{
  "message_id": "189304910293840194",
  "room_id": "189304910293840193",
  "sender_id": "189304910293840192",
  "client_id": "local-01",
  "room_sequence": "42",
  "content": "Mình đã gửi bản cập nhật.",
  "time_created": "2026-11-08T08:30:00Z",
  "time_edited": null,
  "attachments": [],
  "reaction_counts": {},
  "my_reaction": null
}
```

Với `after_sequence`, trả `items` theo thứ tự tăng, `last_message_sequence`, `through_sequence`, `next_cursor` và `has_more`. Trang đầu chụp giới hạn trên. Trang tiếp theo giữ giới hạn này và đi từ sequence của tin cuối đã trả. Khi hết trang, `next_cursor` tiến tới giới hạn trên, kể cả khi không còn tin nào vì đã xóa. `before_sequence` trả trang lùi với cursor là sequence cũ nhất. Hai chế độ không dùng cùng nhau.

### Đàm thoại và chia sẻ màn hình (§4.5)

| Method và đường dẫn | Đầu vào | Kết quả và quyền |
| --- | --- | --- |
| `POST /rooms/{room_id}/voice/join` | Bearer và path | Phòng VOICE và quyền vào hiện tại. Trả `voice_session_id`, `livekit_url`, `livekit_token`, `expires_at`. |
| `DELETE /rooms/{room_id}/voice/leave` | Query `voice_session_id` | Chỉ phiên ứng dụng sở hữu đúng thế hệ mới kết thúc phiên. Retry với thế hệ đã kết thúc không ảnh hưởng phiên mới. |

LiveKit track là giao thức chia sẻ màn hình. Các sự kiện `track_published`, `track_unpublished`, `participant_joined` và `participant_left` đi qua webhook đã xác minh. Không cần route REST riêng để bật/tắt chia sẻ.

## Đường dẫn bổ sung cho hành vi chưa có path trong đặc tả

| Method và đường dẫn | Xác thực và nội dung |
| --- | --- |
| `GET /bootstrap` | JWT. Trả hồ sơ chính chủ, Kênh, quyền/membership cần hiển thị, nhóm, Phòng được xem, DM, bạn bè, lời mời Kênh, yêu cầu kết bạn, chiều chặn của chính actor, unread và trạng thái thoại/chia sẻ theo quyền. Snapshot có revision theo luồng đồng bộ. |
| `GET /rooms/{room_id}/messages/{message_id}/reactions` | JWT và quyền đọc. `cursor?`, `limit?`, `reaction_type?`. Trả trang hồ sơ công khai của người thả và loại reaction. |
| `POST /users/me/avatar/uploads/presign` | JWT. Metadata tệp ảnh, mục đích avatar gắn với actor. Gắn ảnh bằng PATCH hồ sơ. |
| `POST /channels/{channel_id}/images/uploads/presign` | JWT của Creator/Moderator. Metadata và `purpose: ICON` hoặc `BACKGROUND`. Gắn ảnh bằng PATCH Kênh. |
| `GET /ws` | WebSocket upgrade với Bearer. URL đầy đủ đề xuất `/api/v1/ws`. Chỉ server gửi sự kiện nghiệp vụ. |
| `POST /webhooks/livekit` | Chữ ký LiveKit trên raw body. URL đầy đủ đề xuất `/api/v1/webhooks/livekit`. Xử lý idempotent theo event ID. |

Bootstrap lọc cả Phòng và dữ liệu con theo quyền. `read_state` cho Phòng đang mở cần có mốc của các thành viên được hiển thị dấu đã xem. Đề xuất trả trong trang lịch sử đầu khi mở Phòng và cập nhật qua `MESSAGE_READ_UPDATE`. Ảnh hồ sơ hoặc Kênh trả URL hiển thị theo phạm vi tài nguyên và mục đích đã xác minh.

Các trường URL ảnh trỏ tới `GET /api/v1/images/{upload_id}/url`. Endpoint này xác minh Bearer và quyền xem ảnh trước khi trả `url` và `expires_at`. URL đã ký của ảnh và tệp dùng `GET /files/{upload_id}?token=…` trên cổng tải riêng. Client gửi Bearer tại cả hai bước.

## Sự kiện WebSocket (§7)

Envelope đề xuất có `event_id`, `type`, `occurred_at`, `stream`, `revision`, `ordinal`, `data`. `stream` là phạm vi revision của snapshot, ví dụ Phòng, Kênh hoặc quan hệ theo người xem. `revision` dùng chuỗi thập phân. `ordinal` sắp các event cùng revision. Sau commit, dispatcher kiểm tra quyền xem trước khi phát nội dung. Các event thu hồi cho người vừa mất quyền chỉ mang ID cần gỡ. Payload theo bảng là phần tối thiểu cần thống nhất khi viết schema.

| Tên sự kiện | Dữ liệu và người nhận |
| --- | --- |
| `CHANNEL_UPDATE` | Tóm tắt Kênh theo quyền của từng thành viên hiện tại. |
| `CHANNEL_DELETE` | `channel_id` tới thành viên tại thời điểm xóa. |
| `INVITE_CREATE`, `INVITE_UPDATE` | Lời mời và trạng thái cho người mời, người nhận. |
| `MEMBER_ADD`, `MEMBER_REMOVE` | `channel_id`, `user_id` và vai trò khi thêm. Thành viên Kênh và người vừa rời khi xóa. |
| `ROLE_UPDATE` | `channel_id`, các vai trò đổi cho thành viên Kênh. |
| `ROOM_CATEGORY_CREATE`, `ROOM_CATEGORY_UPDATE`, `ROOM_CATEGORY_DELETE` | `channel_id`, `category_id`, dữ liệu nhóm hoặc vị trí khi còn tồn tại, gửi thành viên Kênh. |
| `ROOM_CREATE`, `ROOM_UPDATE`, `ROOM_DELETE` | View Phòng theo quyền mới/hiện tại. Khi mất quyền chỉ gửi ID Phòng và Kênh nếu có. DM gửi hai thành viên. |
| `MESSAGE_CREATE`, `MESSAGE_UPDATE`, `MESSAGE_DELETE` | View tin hoặc ID/sequence khi xóa. Gửi người còn quyền đọc. |
| `MESSAGE_REACTION_UPDATE` | `room_id`, `message_id`, tổng hợp reaction và `my_reaction` theo từng người nhận. |
| `MESSAGE_READ_UPDATE` | `room_id`, `user_id`, `last_read_sequence`, `last_read_at`, gửi người còn quyền đọc. |
| `FRIEND_REQUEST`, `FRIEND_REQUEST_UPDATE` | Yêu cầu mới tới người nhận. Trạng thái từ chối/hủy tới hai bên. |
| `FRIEND_ADD`, `FRIEND_REMOVE` | Trạng thái quan hệ của cặp cho hai bên. |
| `BLOCK_LIST_UPDATE` | Thay đổi trong danh sách chặn của chính actor, gửi các phiên của actor. |
| `DM_STATE_UPDATE` | `room_id`, `is_read_only`, gửi cả hai thành viên. |
| `VOICE_STATE_UPDATE` | `room_id`, `user_id`, `voice_session_id`, trạng thái. Người có quyền xem Phòng nhận state chung. Chỉ phiên cũ nhận cờ chuyển thiết bị. |
| `SCREEN_SHARE_START`, `SCREEN_SHARE_STOP` | `room_id`, `user_id`, `voice_session_id`, track SID phù hợp, gửi người có quyền xem Phòng. |

Client ghép tin lạc quan với response/event qua `(sender_id, client_id)` và sắp theo `room_sequence`. Khi thấy gap, client bắt kịp bằng `GET .../messages?after_sequence={local_seq}`. `event_id` chống xử lý lặp. Revision của từng luồng giúp phân biệt sự kiện đã nằm trong bootstrap và sự kiện phát sinh sau snapshot. Khi nhận thông báo gỡ quyền, desktop loại tài nguyên khỏi giao diện và bỏ mọi response đang bay của thế hệ quyền cũ.
