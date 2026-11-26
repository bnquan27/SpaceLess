# Workspace preview

## Open the workspace

Open the Windows app or web `/`. The lobby shows **Dữ liệu mẫu** and greets the sample user **Nguyễn Minh**. Choose **Nhóm Đồ Án - UI/UX** to open the sample server. Use **Máy chủ & Tin nhắn** to reach direct messages or return to the lobby.

## Drive the workspace

```powershell
node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs web workspace
```

The helper opens the lobby notifications, the sample server, and its member roles. It sends a message in **chung** and a direct message to **Hoàng Đạt**. It toggles the microphone and camera in **Phòng Họp Nhóm** and focuses a sample shared screen. Check the screenshots and the empty `network` array in `result.json`.

For targeted checks, use **Mời cộng tác**, **Tạo kênh**, and **Tạo phòng** from the server overview. The lobby's invite form and the **Thử nhận tin mới** button also use sample data.

The workspace uses sample data and local state. For server-side messaging, permissions, or voice state, use [the API journey](server-api.md).
