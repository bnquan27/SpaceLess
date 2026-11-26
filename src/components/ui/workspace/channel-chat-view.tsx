import React, { useState } from "react";
import { MessageActions } from "./message-actions";
import { PreviewAvatar } from "./preview-images";
import {
  Hash,
  Pin,
  Users,
  FolderGit2,
  Download,
  Code,
  Paperclip,
  Smile,
  Send,
  AtSign,
  List,
  Link2,
  PanelRightClose,
  PanelRightOpen,
  ArrowLeft,
} from "lucide-react";
import { ViewProps } from "./types";

const CHANNEL_METADATA: Record<
  string,
  { tag: string; desc: string; notice: string }
> = {
  chung: {
    tag: "Phòng chính",
    desc: "Không gian thảo luận chung và cập nhật tiến độ của cả nhóm",
    notice: "Họp tổng kết thiết kế giao diện vào 14:00 chiều nay.",
  },
  "thiet-ke-ui-ux": {
    tag: "Thiết kế",
    desc: "Trao đổi về bố cục giao diện, màu sắc và trải nghiệm người dùng",
    notice: "Đã đồng bộ xong giao diện Sảnh chờ và khung chat.",
  },
  "bao-cao-tien-do": {
    tag: "Báo cáo",
    desc: "Cập nhật công việc hoàn thành hàng ngày của các thành viên",
    notice: "Nhắc nhở: Các thành viên gửi báo cáo tiến độ trước 11:30.",
  },
  "tai-nguyen-assets": {
    tag: "Tài nguyên",
    desc: "Kho lưu trữ biểu tượng, phông chữ và tài liệu thiết kế",
    notice: "Đã tải lên bộ tài nguyên hình ảnh mới nhất.",
  },
};

export const ChannelChatView: React.FC<ViewProps> = ({
  currentUser,
  onNavigate,
  selectedChatChannel = "chung",
  onSelectChatChannel,
}) => {
  const [showRightPanel, setShowRightPanel] = useState(true);
  const [showCompactMembers, setShowCompactMembers] = useState(false);
  const [msgInput, setMsgInput] = useState("");

  const activeChannel = CHANNEL_METADATA[selectedChatChannel]
    ? selectedChatChannel
    : "chung";
  const channelInfo = CHANNEL_METADATA[activeChannel];

  const [messagesByChannel, setMessagesByChannel] = useState<
    Record<string, any[]>
  >({
    chung: [
      {
        id: 1,
        sender: "Minh Anh",
        time: "10:14 SA",
        isMe: false,
        text: "Chào mọi người, mình gửi file đóng gói toàn bộ giao diện bản hoàn chỉnh nhé. Các bạn kiểm tra trước buổi họp chiều nay.",
        file: {
          name: "Ban_Thiet_Ke_UIUX_Final.zip",
          size: "18.4 MB",
        },
      },
      {
        id: 2,
        sender: `Bạn (${currentUser})`,
        time: "10:18 SA",
        isMe: true,
        text: "Cảm ơn Minh Anh! Mình đã đồng bộ luôn phần tin nhắn trong nhóm giống với phần tin nhắn trực tiếp (DM) cho dễ nhìn rồi nhé.",
      },
      {
        id: 3,
        sender: "Quốc Bảo",
        time: "10:22 SA",
        isMe: false,
        text: "Giao diện nhìn thoáng và gọn hơn hẳn rồi đấy. Nhớ kiểm tra thêm phần thông báo góc phải nhé!",
      },
    ],
    "thiet-ke-ui-ux": [
      {
        id: 101,
        sender: "Thu Hà",
        time: "09:50 SA",
        isMe: false,
        text: "Em vừa chỉnh lại thanh Dock ẩn xuống dưới đáy, khi nào bấm nút mũi tên mới hiện lên để không che thanh chat ạ.",
      },
      {
        id: 102,
        sender: `Bạn (${currentUser})`,
        time: "09:55 SA",
        isMe: true,
        text: "Chuẩn luôn Hà ơi, nhìn gọn gàng hơn rất nhiều!",
      },
    ],
    "bao-cao-tien-do": [
      {
        id: 201,
        sender: "Hoàng Long",
        time: "09:00 SA",
        isMe: false,
        text: "Đã hoàn thành tính năng kết bạn và danh sách chờ phản hồi trong trang DM.",
      },
    ],
    "tai-nguyen-assets": [
      {
        id: 301,
        sender: "Minh Anh",
        time: "Hôm qua",
        isMe: false,
        text: "Gửi mọi người bộ Icon & Hình nền phi hành gia Space Orbit.",
        file: {
          name: "Space_Orbit_Assets.rar",
          size: "64.2 MB",
        },
      },
    ],
  });

  const currentMessages = messagesByChannel[activeChannel] || [];

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!msgInput.trim()) return;
    const newMsg = {
      id: Date.now(),
      sender: `Bạn (${currentUser})`,
      time: "Vừa xong",
      isMe: true,
      text: msgInput,
    };
    setMessagesByChannel((prev) => ({
      ...prev,
      [activeChannel]: [...(prev[activeChannel] || []), newMsg],
    }));
    setMsgInput("");
  };

  return (
    <div className="relative flex-1 min-h-0 flex overflow-hidden">
      <div className="flex-1 min-w-0 flex flex-col bg-[#15121d] overflow-hidden">
        {/* Subheader của Kênh */}
        <div className="min-h-15 px-4 xl:px-6 py-2 border-b border-white/10 bg-[#15121d]/90 flex flex-wrap items-center justify-between gap-2 shrink-0">
          <div className="flex flex-1 min-w-0 items-center gap-2.5">
            <button
              onClick={() => onNavigate("overview")}
              aria-label="Tổng quan"
              className="workspace-action shrink-0 px-3 py-1.5 rounded-xl bg-[#1c172c] hover:bg-white/10 border border-white/10 text-[#b7aabe] hover:text-[#f4effa] flex items-center gap-1.5 transition"
            >
              <ArrowLeft className="w-3.5 h-3.5 shrink-0" />
              <span className="hidden sm:inline">Tổng quan</span>
            </button>

            <div className="w-8 h-8 shrink-0 rounded-xl bg-[#1c172c] border border-white/10 grid place-items-center text-[#c8b2f1]">
              <Hash className="w-4 h-4 shrink-0" />
            </div>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-x-2">
                <span className="font-bold text-sm text-[#f4effa] wrap-anywhere">
                  {activeChannel}
                </span>
                <span className="text-xs px-2 py-0.5 rounded-full bg-[#c8b2f1]/15 text-[#c8b2f1] font-semibold">
                  {channelInfo.tag}
                </span>
              </div>
              <p className="text-xs text-[#b7aabe] truncate max-w-lg">
                {channelInfo.desc}
              </p>
            </div>
          </div>

          <div className="flex min-w-0 items-center gap-2">
            <div className="hidden lg:flex items-center overflow-x-auto gap-1 bg-[#1c172c] p-1 rounded-xl border border-white/10">
              {Object.keys(CHANNEL_METADATA).map((chKey) => (
                <button
                  key={chKey}
                  onClick={() => onSelectChatChannel?.(chKey)}
                  aria-pressed={activeChannel === chKey}
                  className={`workspace-action shrink-0 px-2 py-1 rounded-lg font-semibold transition flex items-center gap-1 ${
                    activeChannel === chKey
                      ? "bg-[#c8b2f1] text-[#271737]"
                      : "text-[#b7aabe] hover:text-[#f4effa]"
                  }`}
                >
                  <Hash className="w-3 h-3 shrink-0" />
                  <span>{chKey}</span>
                </button>
              ))}
            </div>

            <button
              onClick={() => setShowRightPanel(!showRightPanel)}
              aria-expanded={showRightPanel}
              aria-label="Danh sách thành viên"
              title="Danh sách thành viên"
              className="hidden xl:grid p-2 rounded-xl bg-[#1c172c] hover:bg-[#261e3a] border border-white/10 text-[#c8b2f1] transition"
            >
              {showRightPanel ? (
                <PanelRightClose className="w-4 h-4 shrink-0" />
              ) : (
                <PanelRightOpen className="w-4 h-4 shrink-0" />
              )}
            </button>
            <button
              onClick={() => setShowCompactMembers(true)}
              aria-expanded={showCompactMembers}
              aria-label="Danh sách thành viên"
              title="Danh sách thành viên"
              className="xl:hidden p-2 rounded-xl bg-[#1c172c] hover:bg-[#261e3a] border border-white/10 text-[#c8b2f1] transition"
            >
              <PanelRightOpen className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Thanh Thông báo Ghim */}
        <div className="bg-[#1c172c]/90 border-b border-white/10 px-4 lg:px-8 py-2 flex items-center justify-between text-xs shrink-0">
          <span className="text-[#f4effa] flex items-center gap-2">
            <Pin className="w-3.5 h-3.5 text-[#c8b2f1] shrink-0" />
            <strong>Ghim:</strong> {channelInfo.notice}
          </span>
        </div>

        {/* Danh sách Tin nhắn (Đồng bộ kiểu bong bóng trái/phải giống hệt trang DM) */}
        <div className="flex-1 min-h-0 overflow-y-auto px-4 lg:px-8 py-4 sm:py-6">
          <div className="max-w-3xl mx-auto space-y-5">
            <div className="flex items-center gap-4 text-xs text-[#b7aabe] uppercase tracking-wider">
              <div className="h-[1px] flex-1 bg-white/10" />
              <span className="px-3.5 py-1 rounded-full bg-[#1c172c] border border-white/10 font-semibold">
                PHÒNG #{activeChannel.toUpperCase()}
              </span>
              <div className="h-[1px] flex-1 bg-white/10" />
            </div>

            {currentMessages.map((m) => (
              <MessageActions
                key={m.id}
                messageId={String(m.id)}
                isMe={m.isMe}
                onDelete={() => setMessagesByChannel(previous => ({ ...previous, [activeChannel]: previous[activeChannel].filter(message => message.id !== m.id) }))}
                text={m.text} onReply={text => setMsgInput(`> ${text}\n`)}
              >
                <div className="text-xs text-[#b7aabe] mb-1.5 px-1 flex items-center gap-1.5">
                  <strong className="text-[#f4effa]">{m.sender}</strong>
                  <span>• {m.time}</span>
                </div>

                <div
                  className={`w-fit max-w-full lg:max-w-xl p-4 rounded-2xl text-sm leading-relaxed wrap-anywhere ${
                    m.isMe
                      ? "bg-[#c8b2f1] text-[#271737] font-medium rounded-br-none shadow-lg"
                      : "bg-[#1c172c] border border-white/10 text-[#f4effa] rounded-bl-none"
                  }`}
                >
                  {m.text}

                  {m.file && (
                    <div className="mt-3 p-3 rounded-xl bg-[#15121d] border border-white/10 flex items-center justify-between gap-4">
                      <div className="flex items-center gap-2.5">
                        <FolderGit2 className="w-4 h-4 text-[#c8b2f1] shrink-0" />
                        <div>
                          <div className="font-semibold text-[#f4effa]">
                            {m.file.name}
                          </div>
                          <div className="text-xs text-[#a2d9d3]">
                            {m.file.size}
                          </div>
                        </div>
                      </div>
                      <button className="workspace-action shrink-0 px-2.5 py-1.5 rounded-lg bg-[#c8b2f1] text-[#271737] font-bold flex items-center gap-1">
                        <Download className="w-3 h-3 shrink-0" /> Tải về
                      </button>
                    </div>
                  )}
                </div>

                {m.isMe && (
                  <span className="text-xs text-[#a2d9d3] mt-1">
                    ✓ Đã gửi
                  </span>
                )}
              </MessageActions>
            ))}
          </div>
        </div>

        {/* Khung Soạn thảo Tin nhắn (Đã bỏ chữ P2P Stream / kỹ thuật thừa) */}
        <form
          onSubmit={handleSend}
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
                <button type="button" aria-label="Chèn liên kết" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Link2 className="w-3.5 h-3.5 shrink-0" />
                </button>
                <button type="button" aria-label="Định dạng mã" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Code className="w-3.5 h-3.5 shrink-0" />
                </button>
                <button type="button" aria-label="Tạo danh sách" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <List className="w-3.5 h-3.5 shrink-0" />
                </button>
              </div>
              <span className="hidden sm:inline">Enter để gửi tin nhắn</span>
            </div>

            <input
              type="text"
              value={msgInput}
              aria-label={`Nhắn tin tới #${activeChannel}`}
              onChange={(e) => setMsgInput(e.target.value)}
              placeholder={`Nhắn tin tới #${activeChannel}...`}
              className="w-full bg-[#15121d]/80 border border-white/10 rounded-xl px-4 py-2.5 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1] transition"
            />

            <div className="flex items-center justify-between pt-1">
              <div className="flex items-center gap-1 text-[#b7aabe]">
                <button type="button" aria-label="Đính kèm tệp" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Paperclip className="w-4 h-4 shrink-0" />
                </button>
                <button type="button" aria-label="Chèn biểu cảm" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Smile className="w-4 h-4 shrink-0" />
                </button>
                <button type="button" aria-label="Nhắc đến thành viên" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <AtSign className="w-4 h-4 shrink-0" />
                </button>
              </div>
              <button
                type="submit"
                className="workspace-action px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-bold flex items-center gap-1.5 transition shadow"
              >
                Gửi <Send className="w-3.5 h-3.5 shrink-0" />
              </button>
            </div>
          </div>
        </form>
      </div>

      {/* Cột Sidebar bên phải */}
      {(showRightPanel || showCompactMembers) && (
        <aside aria-label="Thành viên kênh" className={`absolute inset-y-0 right-0 z-10 xl:static w-76 max-w-full bg-[#120f19] border-l border-white/10 p-5 flex-col justify-between shrink-0 overflow-y-auto ${showCompactMembers ? "flex" : "hidden"} ${showRightPanel ? "xl:flex" : "xl:hidden"}`}>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-bold uppercase tracking-wider text-[#a2d9d3]">
                TRỰC TUYẾN (5)
              </h4>
              <Users className="hidden xl:block w-3.5 h-3.5 text-[#b7aabe] shrink-0" />
              <button onClick={() => setShowCompactMembers(false)} aria-label="Đóng danh sách thành viên" className="xl:hidden p-2 rounded-lg hover:bg-white/10">
                <PanelRightClose className="w-4 h-4" />
              </button>
            </div>

            <div className="space-y-2.5">
              {[
                { init: "TH", name: "Thu Hà" },
                { init: "QB", name: "Quốc Bảo" },
                { init: "HL", name: "Hoàng Long" },
                { init: "MA", name: "Minh Anh" },
                {
                  init: "NM",
                  name: `${currentUser} (Bạn)`,
                  highlight: true,
                },
              ].map((u, i) => (
                <div
                  key={i}
                  className={`p-3 rounded-xl border flex items-center justify-between ${
                    u.highlight
                      ? "bg-[#261e3a]/80 border-[#c8b2f1]/45"
                      : "bg-[#1c172c]/60 border-white/10"
                  }`}
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <div className="w-8 h-8 rounded-full bg-[#2a223d] text-[#c8b2f1] text-xs font-bold grid place-items-center shrink-0 relative">
                      <PreviewAvatar name={u.name} initials={u.init} />
                      <span className="w-2 h-2 rounded-full bg-[#a2d9d3] absolute bottom-0 right-0" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-xs font-semibold text-[#f4effa] truncate">
                        {u.name}
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>

        </aside>
      )}
    </div>
  );
};
