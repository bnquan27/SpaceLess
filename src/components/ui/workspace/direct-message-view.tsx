import React, { useState } from "react";
import {
  PhoneCall,
  Video,
  Pin,
  Search,
  Download,
  Eye,
  Mic,
  Send,
  Code,
  Smile,
  Link2,
  UserPlus,
  Clock,
  X,
} from "lucide-react";
import { ViewProps } from "./types";
import { PreviewAvatar } from "./preview-images";

interface FriendContact {
  id: string;
  init: string;
  name: string;
  username: string;
  status: string;
  online: boolean;
  dot: string;
}

export const DirectMessageView: React.FC<ViewProps> = ({
  currentUser,
  onNavigate,
}) => {
  const [dmFilter, setDmFilter] = useState<
    "all" | "online" | "pending" | "add"
  >("all");
  const [selectedFriendId, setSelectedFriendId] = useState<string>("f1");
  const [dmInput, setDmInput] = useState("");
  const [addUsernameInput, setAddUsernameInput] = useState("");

  // Danh sách bạn bè hiện tại
  const [friends] = useState<FriendContact[]>([
    {
      id: "f1",
      init: "HĐ",
      name: "Hoàng Đạt",
      username: "@hoangdat",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f2",
      init: "TH",
      name: "Thu Hà",
      username: "@thuha",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f3",
      init: "QB",
      name: "Quốc Bảo",
      username: "@quocbao",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f4",
      init: "MA",
      name: "Minh Anh",
      username: "@minhanh",
      status: "Ngoại tuyến (2 giờ trước)",
      online: false,
      dot: "#6b7280",
    },
    {
      id: "f5",
      init: "ĐK",
      name: "Đăng Khoa",
      username: "@dangkhoa",
      status: "Ngoại tuyến",
      online: false,
      dot: "#6b7280",
    },
  ]);

  // Danh sách người mình đã bấm Kết bạn nhưng đang chờ họ phản hồi
  const [pendingRequests, setPendingRequests] = useState<
    { id: string; init: string; name: string; username: string; time: string }[]
  >([
    {
      id: "p1",
      init: "TN",
      name: "Tuấn Nghĩa",
      username: "@tuannghia.ui",
      time: "Đã gửi 10 phút trước",
    },
    {
      id: "p2",
      init: "KL",
      name: "Khánh Linh",
      username: "@khanhlinh.design",
      time: "Đã gửi hôm qua",
    },
  ]);

  // Danh sách gợi ý kết bạn trong tab "Thêm bạn"
  const [suggestions] = useState([
    {
      id: "s1",
      init: "VH",
      name: "Việt Hoàng",
      username: "@viethoang.fe",
      mutual: "4 bạn chung",
    },
    {
      id: "s2",
      init: "PL",
      name: "Phương Thảo",
      username: "@phuongthao.ux",
      mutual: "2 bạn chung",
    },
    {
      id: "s3",
      init: "ĐM",
      name: "Đức Mạnh",
      username: "@ducmanh.dev",
      mutual: "Cùng nhóm Đồ Án",
    },
  ]);

  const [dmMessages, setDmMessages] = useState([
    {
      id: 1,
      sender: "Hoàng Đạt",
      time: "09:40 SA",
      isMe: false,
      text: "Chào Minh! Mình gửi tài liệu tổng hợp giao diện nhé, bạn xem qua rồi phản hồi giúp mình.",
      file: {
        name: "Tai_Lieu_Giao_Dien_v2.pdf",
        size: "2.4 MB",
      },
    },
    {
      id: 2,
      sender: `Bạn (${currentUser})`,
      time: "09:45 SA",
      isMe: true,
      text: "Cảm ơn Đạt nhé! Mình đã nhận được file rồi, giao diện nhìn rất gọn và đẹp.",
    },
    {
      id: 3,
      sender: "Hoàng Đạt",
      time: "09:46 SA",
      isMe: false,
      text: "Khi nào rảnh anh em mình bấm gọi thoại trao đổi nhanh một chút nhé!",
      callPrompt: true,
    },
  ]);

  const activeFriend =
    friends.find((f) => f.id === selectedFriendId) || friends[0];

  const displayedFriends =
    dmFilter === "online" ? friends.filter((f) => f.online) : friends;

  const handleSendDm = (e: React.FormEvent) => {
    e.preventDefault();
    if (!dmInput.trim()) return;
    setDmMessages([
      ...dmMessages,
      {
        id: Date.now(),
        sender: `Bạn (${currentUser})`,
        time: "Vừa xong",
        isMe: true,
        text: dmInput,
      },
    ]);
    setDmInput("");
  };

  const handleSendFriendRequest = (person: {
    id: string;
    init: string;
    name: string;
    username: string;
  }) => {
    if (pendingRequests.some((p) => p.username === person.username)) return;
    setPendingRequests((prev) => [
      {
        id: `p_${Date.now()}`,
        init: person.init,
        name: person.name,
        username: person.username,
        time: "Vừa gửi lời mời",
      },
      ...prev,
    ]);
  };

  const handleAddByInput = (e: React.FormEvent) => {
    e.preventDefault();
    if (!addUsernameInput.trim()) return;
    const raw = addUsernameInput.trim();
    const initials = raw.slice(0, 2).toUpperCase();
    setPendingRequests((prev) => [
      {
        id: `p_${Date.now()}`,
        init: initials,
        name: raw,
        username: raw.startsWith("@")
          ? raw
          : `@${raw.toLowerCase().replace(/\s+/g, "")}`,
        time: "Vừa gửi lời mời",
      },
      ...prev,
    ]);
    setAddUsernameInput("");
    setDmFilter("pending");
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col sm:flex-row overflow-hidden">
      {/* Cột Danh bạ & Bộ lọc bên trái */}
      <aside aria-label="Danh bạ" className="w-full max-sm:h-52 sm:w-60 lg:w-80 bg-[#120f19] border-b sm:border-b-0 sm:border-r border-white/10 p-3 sm:p-5 flex flex-col shrink-0">
        <div className="space-y-4 flex-1 overflow-y-auto p-1">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
              TIN NHẮN TRỰC TIẾP
            </span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-[#a2d9d3]/15 text-[#a2d9d3] font-semibold">
              {friends.filter((f) => f.online).length} trực tuyến
            </span>
          </div>

          {/* 4 Nút Tab: Tất cả | Trực tuyến | Chờ phản hồi | Thêm bạn */}
          <div className="grid grid-cols-2 gap-1.5 bg-[#1c172c] p-1.5 rounded-xl border border-white/10 text-xs">
            <button
              onClick={() => setDmFilter("all")}
              aria-pressed={dmFilter === "all"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition ${
                dmFilter === "all"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              Tất cả ({friends.length})
            </button>
            <button
              onClick={() => setDmFilter("online")}
              aria-pressed={dmFilter === "online"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition ${
                dmFilter === "online"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              Trực tuyến ({friends.filter((f) => f.online).length})
            </button>
            <button
              onClick={() => setDmFilter("pending")}
              aria-pressed={dmFilter === "pending"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition flex items-center justify-center gap-1 ${
                dmFilter === "pending"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              <span>Chờ phản hồi</span>
              {pendingRequests.length > 0 && (
                <span className="px-1.5 py-0.2 rounded-full bg-[#a2d9d3] text-[#15121d] text-xs font-bold">
                  {pendingRequests.length}
                </span>
              )}
            </button>
            <button
              onClick={() => setDmFilter("add")}
              aria-pressed={dmFilter === "add"}
              className={`workspace-action py-1.5 rounded-lg font-bold transition flex items-center justify-center gap-1 ${
                dmFilter === "add"
                  ? "bg-[#a2d9d3] text-[#15121d]"
                  : "text-[#a2d9d3] hover:bg-white/5"
              }`}
            >
              <UserPlus className="w-3 h-3 shrink-0" />
              <span>Thêm bạn</span>
            </button>
          </div>

          {/* NỘI DUNG TỪNG TAB */}
          {(dmFilter === "all" || dmFilter === "online") && (
            <div className="space-y-2 pt-1">
              {displayedFriends.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  onClick={() => setSelectedFriendId(f.id)}
                  aria-pressed={selectedFriendId === f.id}
                  className={`w-full text-left p-3 rounded-xl border transition flex items-center justify-between ${
                    selectedFriendId === f.id
                      ? "bg-[#1c172c] border-[#c8b2f1]/50 shadow-md"
                      : "border-transparent hover:bg-white/5"
                  }`}
                >
                  <span className="flex items-center gap-3 min-w-0">
                    <span className="w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center relative shrink-0">
                      <PreviewAvatar name={f.name} initials={f.init} />
                      <span
                        className="w-2.5 h-2.5 rounded-full border-2 border-[#120f19] absolute -bottom-0.5 -right-0.5"
                        style={{ backgroundColor: f.dot }}
                      />
                    </span>
                    <span className="min-w-0">
                      <span className="block text-sm font-semibold text-[#f4effa] truncate">
                        {f.name}
                      </span>
                      <span className="block text-xs text-[#b7aabe] truncate mt-0.5">
                        {f.status}
                      </span>
                    </span>
                  </span>
                </button>
              ))}
            </div>
          )}

          {/* TAB: CHỜ PHẢN HỒI (Những người mình đã bấm kết bạn nhưng chưa phản hồi) */}
          {dmFilter === "pending" && (
            <div className="space-y-2.5 pt-1">
              <div className="text-xs font-bold uppercase tracking-wider text-[#b7aabe] px-1">
                LỜI MỜI ĐÃ GỬI ({pendingRequests.length})
              </div>
              {pendingRequests.length === 0 ? (
                <p className="text-xs text-[#b7aabe] p-3 rounded-xl bg-[#1c172c]/50">
                  Chưa có lời mời kết bạn nào đang chờ phản hồi.
                </p>
              ) : (
                pendingRequests.map((req) => (
                  <div
                    key={req.id}
                    className="p-3 rounded-xl bg-[#1c172c]/80 border border-white/10 flex items-center justify-between gap-2"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <div className="relative w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center shrink-0">
                        <PreviewAvatar name={req.name} initials={req.init} />
                      </div>
                      <div className="min-w-0">
                        <div className="text-xs font-bold text-[#f4effa] truncate">
                          {req.name}
                        </div>
                        <div className="text-xs text-[#a2d9d3] flex items-center gap-1 mt-0.5">
                          <Clock className="w-3 h-3 shrink-0" />
                          <span className="truncate">{req.time}</span>
                        </div>
                      </div>
                    </div>

                    <button
                      onClick={() =>
                        setPendingRequests((prev) =>
                          prev.filter((p) => p.id !== req.id),
                        )
                      }
                      title="Hủy lời mời"
                      className="p-1.5 rounded-lg bg-white/5 hover:bg-red-500/20 text-[#b7aabe] hover:text-red-300 transition shrink-0"
                    >
                      <X className="w-3.5 h-3.5 shrink-0" />
                    </button>
                  </div>
                ))
              )}
            </div>
          )}

          {/* TAB: THÊM BẠN */}
          {dmFilter === "add" && (
            <div className="space-y-4 pt-1">
              <form onSubmit={handleAddByInput} className="space-y-2">
                <label htmlFor="dm-add-contact" className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  NHẬP TÊN HOẶC ID NGƯỜI DÙNG
                </label>
                <div className="flex gap-1.5">
                  <input
                    id="dm-add-contact"
                    type="text"
                    value={addUsernameInput}
                    onChange={(e) => setAddUsernameInput(e.target.value)}
                    placeholder="VD: Minh Khoa hoặc @minhkhoa"
                    className="flex-1 min-w-0 bg-[#1c172c] border border-white/10 rounded-xl px-3 py-2 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1]"
                  />
                  <button
                    type="submit"
                    className="px-3 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] text-xs font-bold shrink-0"
                  >
                    Kết bạn
                  </button>
                </div>
              </form>

              <div className="space-y-2">
                <div className="text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  GỢI Ý KẾT BẠN
                </div>
                {suggestions.map((sug) => {
                  const isSent = pendingRequests.some(
                    (p) => p.username === sug.username,
                  );
                  return (
                    <div
                      key={sug.id}
                      className="p-3 rounded-xl bg-[#1c172c]/70 border border-white/10 flex items-center justify-between gap-2"
                    >
                      <div className="flex items-center gap-2.5 min-w-0">
                        <div className="relative w-8 h-8 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center shrink-0">
                          <PreviewAvatar name={sug.name} initials={sug.init} />
                        </div>
                        <div className="min-w-0">
                          <div className="text-xs font-bold text-[#f4effa] truncate">
                            {sug.name}
                          </div>
                          <div className="text-xs text-[#b7aabe] truncate">
                            {sug.mutual}
                          </div>
                        </div>
                      </div>

                      <button
                        onClick={() => handleSendFriendRequest(sug)}
                        className={`px-2.5 py-1.5 rounded-lg text-xs font-bold transition shrink-0 ${
                          isSent
                            ? "bg-[#a2d9d3]/20 text-[#a2d9d3]"
                            : "bg-[#c8b2f1] text-[#271737] hover:bg-[#d8c7f7]"
                        }`}
                      >
                        {isSent ? "Chờ phản hồi" : "+ Kết bạn"}
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </aside>

      {/* Khu vực Trò chuyện 1-1 */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col bg-[#15121d] overflow-hidden">
        <div className="min-h-15 px-4 lg:px-8 py-2 border-b border-white/10 flex flex-wrap gap-2 items-center justify-between shrink-0">
          <div className="flex min-w-0 items-center gap-3.5">
            <div className="relative w-9 h-9 shrink-0 rounded-full bg-[#261e3a] border border-[#c8b2f1]/40 text-[#c8b2f1] text-xs font-bold grid place-items-center">
              <PreviewAvatar name={activeFriend.name} initials={activeFriend.init} />
            </div>
            <div>
              <div className="text-sm font-bold flex flex-wrap items-center gap-x-2 gap-y-0.5">
                {activeFriend.name}
                <span className="text-xs font-normal text-[#b7aabe]">
                  {activeFriend.username}
                </span>
                <span className="text-xs px-2 py-0.5 rounded-full bg-[#a2d9d3]/15 text-[#a2d9d3]">
                  • {activeFriend.status}
                </span>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => onNavigate("voice")}
              title="Gọi thoại"
              className="p-2.5 rounded-xl bg-[#1c172c] hover:bg-white/10 border border-white/10 text-[#a2d9d3] transition"
            >
              <PhoneCall size={15} />
            </button>
            <button
              onClick={() => onNavigate("screen")}
              title="Gọi Video"
              className="p-2.5 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] transition shadow"
            >
              <Video size={15} />
            </button>
            <button aria-label="Tin nhắn đã ghim" className="p-2.5 rounded-xl bg-[#1c172c] border border-white/10 text-[#b7aabe]">
              <Pin size={15} />
            </button>
            <button aria-label="Tìm trong hội thoại" className="p-2.5 rounded-xl bg-[#1c172c] border border-white/10 text-[#b7aabe]">
              <Search size={15} />
            </button>
          </div>
        </div>

        {/* Lịch sử Tin nhắn 1-1 */}
        <div className="flex-1 min-h-0 overflow-y-auto px-4 lg:px-8 py-4 sm:py-6">
          <div className="max-w-3xl mx-auto space-y-5">
            <div className="flex items-center gap-4 text-xs text-[#b7aabe] uppercase tracking-wider">
              <div className="h-[1px] flex-1 bg-white/10" />
              <span className="px-3.5 py-1 rounded-full bg-[#1c172c] border border-white/10 font-semibold">
                HÔM NAY
              </span>
              <div className="h-[1px] flex-1 bg-white/10" />
            </div>

            {dmMessages.map((dm) => (
              <div
                key={dm.id}
                className={`flex flex-col ${
                  dm.isMe ? "items-end" : "items-start"
                }`}
              >
                <div className="text-xs text-[#b7aabe] mb-1.5 px-1">
                  <strong className="text-[#f4effa]">{dm.sender}</strong> •{" "}
                  {dm.time}
                </div>
                <div
                  className={`w-fit max-w-full lg:max-w-xl p-4 rounded-2xl text-sm leading-relaxed wrap-anywhere ${
                    dm.isMe
                      ? "bg-[#c8b2f1] text-[#271737] font-medium rounded-br-none shadow-lg"
                      : "bg-[#1c172c] border border-white/10 text-[#f4effa] rounded-bl-none"
                  }`}
                >
                  {dm.text}

                  {dm.file && (
                    <div className="mt-3 p-3 rounded-xl bg-[#15121d] border border-white/10 flex flex-wrap items-center justify-between gap-3">
                      <div>
                        <div className="font-semibold text-[#f4effa]">
                          {dm.file.name}
                        </div>
                        <div className="text-xs text-[#a2d9d3] mt-0.5">
                          {dm.file.size}
                        </div>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <button className="px-2.5 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 text-xs flex items-center gap-1 text-[#f4effa]">
                          <Eye size={12} /> Xem
                        </button>
                        <button aria-label="Tải tệp đính kèm" className="workspace-icon-button rounded-lg bg-[#c8b2f1]/20 text-[#c8b2f1]">
                          <Download size={13} />
                        </button>
                      </div>
                    </div>
                  )}

                  {dm.callPrompt && (
                    <div className="mt-3 pt-3 border-t border-white/10 flex flex-wrap items-center justify-between gap-3">
                      <span className="text-xs text-[#a2d9d3] font-semibold">
                        ✦ Bắt đầu cuộc gọi với {activeFriend.name}
                      </span>
                      <div className="flex gap-2">
                        <button
                          onClick={() => onNavigate("voice")}
                          className="px-3.5 py-1.5 rounded-xl bg-white/10 hover:bg-white/15 text-xs flex items-center gap-1.5"
                        >
                          <PhoneCall size={12} /> Thoại
                        </button>
                        <button
                          onClick={() => onNavigate("screen")}
                          className="px-3.5 py-1.5 rounded-xl bg-[#c8b2f1] text-[#271737] font-bold text-xs flex items-center gap-1.5"
                        >
                          <Video size={12} /> Video
                        </button>
                      </div>
                    </div>
                  )}
                </div>
                {dm.isMe && (
                  <span className="text-xs text-[#a2d9d3] mt-1">
                    ✓ Đã gửi
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Khung Nhập tin nhắn 1-1 (Đã bỏ chữ P2P Stream) */}
        <form
          onSubmit={handleSendDm}
          className="px-3 lg:px-8 py-2 sm:py-4 border-t border-white/10 bg-[#120f19] shrink-0"
        >
          <div className="max-w-3xl mx-auto rounded-2xl bg-[#1c172c] border border-white/15 p-3 space-y-2 shadow-lg">
            <div className="flex flex-wrap items-center justify-between gap-x-2 text-xs text-[#b7aabe]">
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  aria-label="In đậm"
                  className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1] font-bold"
                >
                  B
                </button>
                <button type="button" aria-label="In nghiêng" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1] italic">
                  I
                </button>
                <button type="button" aria-label="Định dạng mã" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Code size={13} />
                </button>
                <button type="button" aria-label="Chèn liên kết" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Link2 size={13} />
                </button>
                <button type="button" aria-label="Chèn biểu cảm" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Smile size={13} />
                </button>
              </div>
              <span className="hidden sm:inline">Enter để gửi tin nhắn</span>
            </div>

            <input
              type="text"
              value={dmInput}
              aria-label={`Nhắn tin riêng cho ${activeFriend.name}`}
              onChange={(e) => setDmInput(e.target.value)}
              placeholder={`Nhắn tin riêng cho ${activeFriend.name}...`}
              className="w-full bg-[#15121d]/80 border border-white/10 rounded-xl px-4 py-2.5 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1] transition"
            />

            <div className="flex items-center justify-between pt-1">
              <div className="flex items-center gap-2.5">
                <button
                  type="button"
                  onClick={() => onNavigate("voice")}
                  className="workspace-action px-3 py-1.5 rounded-xl bg-white/5 hover:bg-white/10 text-[#a2d9d3] flex items-center gap-1.5 transition"
                >
                  <Mic size={12} /> Gửi ghi âm
                </button>
              </div>
              <button
                type="submit"
                className="workspace-action px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-bold flex items-center gap-1.5 shadow transition"
              >
                Gửi <Send size={13} />
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
};
