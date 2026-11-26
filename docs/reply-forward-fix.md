# Sửa phản hồi và chuyển tiếp — 26/11/2026

Áp dụng trên giao diện hiện tại trong `SpaceLess-ui-old`, nhánh `codex/backend-current-ui`.

## Hành vi

- Trả lời mở thanh “Đang phản hồi [người gửi]” phía trên ô soạn. Giữ nguyên bản nháp, chuyển focus về ô nhập, có nút hủy. Gửi thành công mới xóa ngữ cảnh; lỗi gửi giữ nguyên. Đổi cuộc trò chuyện xóa ngữ cảnh.
- Tin gửi hiển thị trích dẫn tin gốc trong DM và phòng TEXT. Ngữ cảnh lưu trên PostgreSQL, đồng bộ trong lịch sử và WebSocket.
- Chuyển tiếp mở hộp chọn nơi nhận với tìm kiếm, checkbox chọn nhiều nơi, nội dung xem trước, lời nhắn tùy chọn và nút Gửi. Đóng bằng Escape, nút đóng hoặc bấm nền; khóa đóng lúc đang gửi.
- Danh sách nhận gồm phòng TEXT có quyền ghi và bạn bè (tạo/mở DM qua API); không gồm VOICE hay DM chỉ đọc. Backend vẫn kiểm tra quyền ở thời điểm gửi.
- Nếu gửi một phần thất bại, giữ hộp thoại, đánh dấu nơi đã gửi và chỉ thử lại các nơi còn lại. Từng nơi dùng `client_id` cố định để không tạo tin trùng khi mất phản hồi.
- Tệp đính kèm chưa hỗ trợ chuyển tiếp: nút gửi bị khóa và có thông báo. Không chuyển quyền tải tệp của phòng gốc sang phòng khác.

## API bổ sung

Migration `0007_message_context.sql` thêm hai cột JSONB; không sửa migration cũ. POST tin nhắn nhận `reply_to` (ID tin cùng phòng), hoặc `forward_from: {room_id,message_id}`. Server đọc nguồn và chụp tên/nội dung, không nhận trích dẫn do client tự khai. Hai loại ngữ cảnh không dùng cùng lúc. Response thêm `reply_context` và `forward_context`.

Snapshot lưu nội dung lúc gửi; chỉnh sửa/xóa tin gốc sau đó không sửa snapshot đã gửi. Mã băm chống trùng bao gồm tham chiếu nguồn. Phản hồi/chuyển tiếp lại giữ nội dung trích dẫn của tin nguồn đã được chuyển tiếp.

## Bằng chứng

- Build TypeScript/Vite thành công; Rust unit test 6/6.
- Bộ API hồi quy mới **25/25**, 373 requests, 53/53 route/alias, không route nào bỏ sót. Có ca mới trong `server/test/messaging.mjs`: lưu ngữ cảnh, gửi lại idempotent, từ chối đổi nguồn cùng client_id, phản hồi sai phòng, chuyển tiếp không lời nhắn, chuyển tiếp lại và nguồn không có quyền. [Kết quả](qa/reply-forward/api-suite/result.json).
- Playwright trên API thật: DM và phòng chat đều phản hồi/chuyển tiếp được; tìm kiếm, chọn hai nơi, hủy phản hồi giữ bản nháp, Escape, lịch sử sau đăng nhập lại và màn hình 390×844/1440×1000.
- Mô phỏng server đã lưu nhưng mất response ở một nơi: gửi lại thành công. PostgreSQL xác nhận đúng một tin tại mỗi nơi nhận cho nội dung fixture `QA mất phản hồi chuyển tiếp`.
- Database hồi quy tách biệt đã dọn; `spaceless_current_ui` và API ứng dụng vẫn chạy. Các lỗi console HTTP 403/404/409 trong kiểm tra quyền và lỗi mạng giả lập là có chủ đích; không dùng chúng làm bằng chứng lỗi giao diện.

![Thanh phản hồi](qa/backend-init/reply-banner.png)
![Hộp chuyển tiếp](qa/backend-init/forward-dialog.png)
![Chuyển tiếp trên màn hình hẹp](qa/backend-init/forward-narrow.png)
![Phản hồi trong phòng chat](qa/backend-init/channel-reply.png)

Chưa chạy bản native Tauri. Các giới hạn SMTP/storage/LiveKit trong `backend-current-ui.md` vẫn áp dụng.
