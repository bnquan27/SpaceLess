# Báo cáo khảo sát và chỉnh sửa giao diện SpaceLessApp

Khảo sát ngày 15/11/2026.  
Chromium qua Playwright MCP trên Windows. So sánh giao diện hiện trạng trên Vite dev với bản build Vite Preview sau khi sửa, tại 1280 × 820, 760 × 700 và 390 × 844 CSS px. Workspace chứa dữ liệu mẫu. Biểu mẫu Orbit mở qua nút **Đăng xuất**.

## Kết quả khảo sát

| Mức độ | Màn hình và cách tái hiện | Hiện trạng | Kết quả kiểm lại |
| --- | --- | --- | --- |
| P1 | Sảnh và chat. Mở sảnh rồi vào máy chủ **Nhóm Đồ Án - UI/UX** → kênh **chung**. | Reset CSS toàn cục thắng lớp tiện ích Tailwind. Chữ sáng trên nút lilac đạt 1,67:1, trên nút mint đạt 1,39:1, còn chữ `NM` trùng màu nền và đạt 1:1. | **Vào ngay** và **Gửi** đạt 8,78:1. **Tham gia ngay** đạt 11,80:1 và **Vào ngay** khi hover đạt 10,63:1. Mục **Thành viên & Phân quyền** đang chọn cũng đạt 8,78:1. Chữ trên các nút chính đo được là 14 px. Ảnh hồ sơ được hiển thị dưới dạng ảnh đại diện với chữ viết tắt dự phòng. [Sảnh trước](qa/ui-audit-2026-11-15/before-sanh-1280.png) · [Sảnh sau](qa/ui-audit-2026-11-15/after-sanh-1280.png) · [Chat trước](qa/ui-audit-2026-11-15/before-chat-1280.png) · [Chat sau](qa/ui-audit-2026-11-15/after-chat-1280.png) |
| P1 | Điều hướng bằng bàn phím. Từ nút **Tạo máy chủ**, nhấn Tab. Thử tương tự trong danh sách phòng trò chuyện và danh bạ DM. | Thẻ máy chủ, hàng kênh và hàng hội thoại dùng phần tử không nhận focus. Tab bỏ qua đường vào trực tiếp. Nút định dạng đậm ở chat chỉ khoảng 7 × 17 px. | Tab đến thẻ máy chủ rồi Enter mở tổng quan. Tab đến hàng kênh rồi Space mở chat. Tab trong danh bạ rồi Space đổi người nhận sang Thu Hà. Nút **In đậm** có vùng bấm 32 × 32 px và tên tiếng Việt. [Trước](qa/ui-audit-2026-11-15/before-chat-1280.png) · [Sau và viền focus](qa/ui-audit-2026-11-15/after-dm-focus-390.png) |
| P1 | DM ở 390 × 844. Từ sảnh chọn **Mở tất cả**. | Cột danh bạ rộng 320 px. Nút **Gửi** nằm trong khoảng x = 433–510 px, ngoài viewport. Nút danh sách thành viên ở chat kênh cũng nằm ngoài viewport. | Danh bạ thành hàng cuộn phía trên. Ô nhập DM nằm ở x = 25–365 px, nút **Gửi** ở x = 290–365 px, cao 44 px. Bấm **Gửi** hiển thị tin nhắn mẫu. Trong chat, nút **Gửi** cũng cao 44 px và có thể mở rồi đóng danh sách thành viên. [DM trước](qa/ui-audit-2026-11-15/before-dm-390.png) · [DM sau](qa/ui-audit-2026-11-15/after-dm-390.png) · [Chat sau](qa/ui-audit-2026-11-15/after-chat-390.png) |
| P2 | Sảnh ở 390 × 844. Xem ô mã mời và danh sách máy chủ. | Nút **Vào ngay** vượt mép thẻ mã mời. Các danh sách cuộn trong hai vùng ngắn và nút tạo máy chủ bị tách chữ thành nhiều dòng. | Nút **Vào ngay** nằm trong thẻ, cao 44 px. Sảnh cuộn theo chiều dọc, đủ chỗ cho tên nút và ảnh. Đo chiều rộng tài liệu 390 px ở lượt kiểm DM. [Trước](qa/ui-audit-2026-11-15/before-sanh-390.png) · [Sau 390 px](qa/ui-audit-2026-11-15/after-sanh-390.png) · [Sau 760 px](qa/ui-audit-2026-11-15/after-sanh-760.png) |
| P2 | Phòng thoại. Mở **Phòng Họp Nhóm** và kiểm tra cụm nút cuối màn hình. | Sáu nút biểu tượng 40 × 40 px thiếu tên trong cây trợ năng. | Có tên cho mic, tai nghe, camera, mở màn hình chia sẻ, cài đặt âm thanh và rời phòng. Space đổi `aria-pressed` của mic, tai nghe và camera. Biểu tượng cũng phản ánh trạng thái. Đã mở màn hình chia sẻ mẫu ở cả khung rộng và khung 390 px. [Trước](qa/ui-audit-2026-11-15/before-thoai-1280.png) · [Sau](qa/ui-audit-2026-11-15/after-thoai-1280.png) |
| P2 | Orbit. Đăng xuất rồi chuyển giữa đăng nhập, đăng ký và khôi phục mật khẩu. | **Quay lại đăng nhập** dùng chữ 9 px và vùng bấm cao 15 px. Nút điều khiển chuyển động dùng chữ 9 px. | Nút quay lại dùng chữ 14 px và vùng bấm 155 × 44 px tại 390 px. Đăng nhập trống báo hai lỗi và chuyển focus về tên đăng nhập. Đã kiểm tra hiện/ẩn mật khẩu, lỗi email, sáu lỗi đăng ký trống và hai thông báo bản xem thử. Chế độ giảm chuyển động mở tĩnh, cho phép chạy rồi dừng phi hành gia. [Đăng ký trước](qa/ui-audit-2026-11-15/before-dang-ky-390.png) · [Đăng ký sau](qa/ui-audit-2026-11-15/after-dang-ky-390.png) · [Lỗi đăng nhập](qa/ui-audit-2026-11-15/after-orbit-loi-390.png) · [Khôi phục](qa/ui-audit-2026-11-15/after-khoi-phuc-390.png) · [Orbit rộng trước](qa/ui-audit-2026-11-15/before-orbit-1280.png) · [Orbit rộng sau](qa/ui-audit-2026-11-15/after-orbit-1280.png) |

Tỷ lệ tương phản được tính từ màu chữ và màu nền đặc do Chromium trả về. Ngưỡng WCAG 2.2 AA cho chữ thường là 4,5:1. Mốc 32 và 44 px là mục tiêu vùng bấm của đợt sửa. Tiêu chí AA về kích thước vùng bấm dùng mức 24 CSS px và có các ngoại lệ. [Tương phản chữ](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) · [Kích thước vùng bấm](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) · [Thao tác bàn phím](https://www.w3.org/WAI/WCAG22/Understanding/keyboard.html)

## Ảnh mẫu trong giao diện

ImageGen tạo bốn hình theo chủ đề thiết kế, lập trình, an toàn thông tin và chơi game, cùng chân dung hư cấu cho 15 người dùng mẫu. Ảnh máy chủ xuất hiện trên thẻ sảnh và dock. Ảnh đại diện xuất hiện ở thanh trên, DM, danh sách thành viên, phòng thoại, màn hình chia sẻ và phân quyền. Hai ảnh JPEG 640 × 640 được tải từ tài nguyên cục bộ, tổng khoảng 188 KiB. Sảnh rộng đã tải và hiển thị đủ bốn ảnh máy chủ, bốn ảnh bạn bè và ảnh người dùng hiện tại. [Tệp ảnh máy chủ](../public/assets/workspace-servers.jpg) · [Tệp ảnh đại diện](../public/assets/workspace-avatars.jpg) · [Phân quyền ở 760 px](qa/ui-audit-2026-11-15/after-phan-quyen-760.png)

![Sảnh ở 1280 × 820 với bốn ảnh máy chủ và ảnh đại diện của người dùng mẫu](qa/ui-audit-2026-11-15/after-sanh-1280.png)

## Kiểm chứng và giới hạn

- `npm run build -- --configLoader runner` hoàn tất TypeScript và build Vite. Lệnh `npm run build` mặc định qua bước TypeScript, sau đó dừng vì esbuild bị từ chối đọc thư mục cha khi nạp cấu hình Vite trong sandbox Windows.
- Playwright đã gửi tin nhắn mẫu bằng Enter trong chat và bằng nút **Gửi** trong DM. Kiểm tra biểu mẫu với dữ liệu tổng hợp cho thấy **0 yêu cầu API xác thực**. Bản xem thử dùng chung lịch sử mẫu trong DM khi đổi người nhận. Hàng hội thoại ở sảnh dùng mũi tên mở DM thay cho biểu tượng gọi nhanh cũ, vì cả hàng dẫn tới danh sách hội thoại. Các nút như **Tạo máy chủ**, định dạng và đính kèm tin, tải tệp, và cài đặt âm thanh hiện có giao diện mẫu.
- Phạm vi kiểm chứng là Chromium trên bản web, dùng kích thước viewport mô phỏng. Trình nạp và bộ quét Impeccable thiếu engine trong môi trường này. Số đo và ảnh trong báo cáo đến từ Playwright. Việc đóng gói Tauri, xác thực, gửi email và media LiveKit cần môi trường tích hợp riêng để kiểm chứng.

<details>
<summary>Prompt đã dùng với ImageGen tích hợp</summary>

**Ảnh máy chủ**

```text
Use case: product-mockup. Asset type: a single square 2×2 image atlas for the four server thumbnails in a dark Vietnamese desktop chat app. Generate four crisp editorial photographs, one image per exactly equal quadrant, seams exactly at the horizontal and vertical center, edge to edge with absolutely no gutters, borders, frames or text. Each recognizable subject centered within its own quadrant, photographed with rich natural texture and moody illumination. Top left: a design studio desk with a neatly laid out purple color swatch fan, tablet with abstract shapes and pencil, lilac light. Top right: a software engineer’s workstation, mechanical keyboard and a monitor with abstract cyan code-like lines, cool blue light, no legible characters. Bottom left: close-up of tidy black server racks with teal status lights, cybersecurity laboratory. Bottom right: black game controller centered under magenta and violet light, arcade atmosphere. Keep composition simple and icons readable as cropped squares at 56px. No logos, people, captions or watermark.
```

**Ảnh đại diện**

```text
Use case: photorealistic-natural. Asset type: one square 4×4 contact sheet of 16 fictitious Vietnamese adult user avatars, one individual in each exactly equal square grid cell, with seams exactly at the quarter, half and three-quarter positions of both axes. No gutters, borders, captions, ornaments or outlines. Head-and-shoulders photographs, each face centered within its own cell so a circular crop keeps the whole face. Friendly natural expressions, clean soft studio lighting, subtly different muted dark lavender, slate blue, teal and warm neutral backgrounds, distinct recognizable faces. Ordered rows: row 1 young male with dark wavy hair, young male with round glasses, young woman with bangs, young male with short side part. Row 2 young woman with wavy hair, young man with a textured crop, male with square glasses, male with a loose fringe. Row 3 female with a bob, young male with longer curled hair, woman with thin glasses, man with crew cut. Row 4 one young man with oval glasses, man with straight short hair, man with parted medium hair, woman with cropped hair. No duplicates, text, brands or watermarks. Straight aligned exact 4-by-4 grid.
```

</details>

