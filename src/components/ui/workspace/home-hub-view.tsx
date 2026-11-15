import React, { useState } from "react";
import {
  Sparkles,
  Plus,
  MessageSquare,
  ArrowRight,
  FolderGit2,
  Link2,
  Check,
  Headphones,
  Radio,
} from "lucide-react";
import { PreviewAvatar, PreviewServerPhoto } from "./preview-images";

export interface HomeServerItem {
  id: string;
  name: string;
  shortName: string;
  category: string;
  desc?: string;
  membersCount: number;
  onlineCount: number;
  badge?: number;
  onlineDot?: boolean;
  icon: React.ElementType;
}

interface HomeHubViewProps {
  currentUser: string;
  servers: HomeServerItem[];
  onSelectServer: (serverId: string) => void;
  onOpenDm: () => void;
  onQuickJoinVoice: (serverId: string, roomName: string) => void;
}

export const HomeHubView: React.FC<HomeHubViewProps> = ({
  currentUser,
  servers,
  onSelectServer,
  onOpenDm,
  onQuickJoinVoice,
}) => {
  const [inviteCode, setInviteCode] = useState("");
  const [joinNotice, setJoinNotice] = useState("");

  const handleJoinByCode = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inviteCode.trim()) return;
    setJoinNotice(`Đang kết nối vào máy chủ "${inviteCode}"...`);
    setTimeout(() => {
      setJoinNotice("");
      setInviteCode("");
      onSelectServer("uiux");
    }, 800);
  };

  return (
    <div className="flex-1 min-h-0 w-full overflow-y-auto lg:overflow-hidden px-4 sm:px-6 md:px-10 xl:px-14 pt-5 pb-3 flex flex-col">
      <div className="max-w-[1400px] w-full mx-auto flex flex-col gap-5 lg:flex-1 lg:min-h-0">
        {/* 1. HERO WELCOME BANNER */}
        <section className="w-full bg-gradient-to-r from-[#1c172c] via-[#261e3d] to-[#1c172c] border border-white/10 rounded-2xl p-5 md:p-6 flex flex-col lg:flex-row items-start lg:items-center justify-between gap-5 shadow-xl shrink-0">
          <div className="space-y-2 max-w-2xl">
            <div className="flex items-center gap-2 text-xs font-semibold tracking-wider uppercase">
              <span className="px-2.5 py-1 rounded-full bg-[#c8b2f1]/15 border border-[#c8b2f1]/30 text-[#c8b2f1] flex items-center gap-1.5">
                <Sparkles className="w-3 h-3 shrink-0" /> SẢNH CHỜ
              </span>
              <span className="px-2.5 py-1 rounded-full bg-[#a2d9d3]/15 border border-[#a2d9d3]/30 text-[#a2d9d3]">
                Dữ liệu mẫu
              </span>
            </div>

            <h1 className="text-2xl md:text-3xl font-bold tracking-tight text-[#f4effa]">
              Chào mừng trở lại,{" "}
              <span className="text-[#c8b2f1]">{currentUser}</span>!
            </h1>
            <p className="text-sm leading-relaxed text-[#b7aabe]">
              Chọn một máy chủ bên dưới để bắt đầu trò chuyện hoặc nhắn tin trực
              tiếp với bạn bè.
            </p>
          </div>

          {/* Ô Nhập mã mời tham gia nhanh */}
          <form
            onSubmit={handleJoinByCode}
            className="w-full lg:w-auto lg:min-w-[320px] bg-[#15121d]/90 border border-white/10 rounded-2xl p-3.5 space-y-2.5 shadow-inner"
          >
            <label htmlFor="workspace-invite-code" className="text-sm font-bold text-[#f4effa] flex items-center gap-1.5">
              <Link2 className="w-3.5 h-3.5 text-[#a2d9d3] shrink-0" />
              Tham gia máy chủ bằng mã mời
            </label>

            <div className="flex flex-wrap items-center gap-2">
              <input
                id="workspace-invite-code"
                type="text"
                value={inviteCode}
                onChange={(e) => setInviteCode(e.target.value)}
                placeholder="Mã hoặc liên kết mời"
                className="flex-1 basis-32 min-w-0 min-h-11 bg-[#1c172c] border border-white/10 rounded-xl px-3 py-2 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1]"
              />
              <button
                type="submit"
                className="workspace-action px-3.5 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-semibold transition shrink-0"
              >
                Vào ngay
              </button>
            </div>

            {joinNotice && (
              <p className="text-xs text-[#a2d9d3] flex items-center gap-1">
                <Check className="w-3.5 h-3.5 shrink-0" /> {joinNotice}
              </p>
            )}
          </form>
        </section>

        {/* 2. HAI KHUNG BẰNG NHAU & KÉO SÁT XUỐNG GẦN THANH DOCK */}
        <div className="lg:flex-1 grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch lg:min-h-0">
          {/* CỘT TRÁI (8/12): MÁY CHỦ CỦA BẠN (Đã bỏ phần mô tả máy chủ) */}
          <div className="lg:col-span-8 bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col justify-between shadow-lg min-h-0 overflow-y-auto">
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa] flex items-center gap-2">
                    <FolderGit2 className="w-4 h-4 text-[#c8b2f1] shrink-0" />
                    MÁY CHỦ CỦA BẠN ({servers.length})
                  </h2>
                  <p className="text-xs text-[#b7aabe] mt-0.5">
                    Bấm vào máy chủ để mở danh sách kênh và phòng thoại
                  </p>
                </div>

                <button className="workspace-action px-3.5 py-2 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 font-semibold text-[#f4effa] flex items-center gap-1.5 transition">
                  <Plus className="w-3.5 h-3.5 text-[#c8b2f1] shrink-0" />
                  <span>Tạo máy chủ</span>
                </button>
              </div>

              {/* Lưới 2x2 Thẻ Máy chủ (Gọn gàng, không có mô tả) */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {servers.map((srv) => {
                  const Icon = srv.icon;
                  return (
                    <article
                      key={srv.id}
                      className="group relative p-5 rounded-2xl bg-[#15121d]/85 hover:bg-[#231c36]/90 border border-white/10 hover:border-[#c8b2f1]/50 transition flex flex-col justify-between gap-5 shadow-md"
                    >
                      <button
                        type="button"
                        aria-label={`Mở máy chủ ${srv.name}`}
                        onClick={() => onSelectServer(srv.id)}
                        className="absolute inset-0 rounded-2xl"
                      />
                      <div className="flex items-center justify-between gap-3">
                        <div className="flex items-center gap-3.5">
                          <div className="relative w-14 h-14 shrink-0 rounded-xl bg-[#1c172c] group-hover:bg-[#c8b2f1] text-[#c8b2f1] group-hover:text-[#271737] border border-white/10 grid place-items-center transition">
                            <Icon className="w-5 h-5 shrink-0" />
                            <PreviewServerPhoto id={srv.id} />
                          </div>
                          <div>
                            <span className="text-xs font-semibold uppercase tracking-wider text-[#a2d9d3]">
                              {srv.category}
                            </span>
                            <h3 className="text-base font-bold text-[#f4effa] group-hover:text-[#c8b2f1] transition mt-0.5">
                              {srv.name}
                            </h3>
                          </div>
                        </div>

                        {srv.badge && (
                          <span className="px-2.5 py-0.5 rounded-full bg-[#c8b2f1] text-[#271737] text-xs font-bold shrink-0">
                            {srv.badge} mới
                          </span>
                        )}
                      </div>

                      <div className="pt-3 border-t border-white/10 flex items-center justify-between">
                        <div className="flex items-center gap-2 text-xs text-[#b7aabe]">
                          <span className="text-[#a2d9d3] font-medium">
                            • {srv.onlineCount} trực tuyến
                          </span>
                          <span>/ {srv.membersCount} thành viên</span>
                        </div>

                        <span className="inline-flex items-center gap-1 text-xs font-bold text-[#c8b2f1] group-hover:translate-x-0.5 transition">
                          Vào <ArrowRight className="w-3.5 h-3.5 shrink-0" />
                        </span>
                      </div>
                    </article>
                  );
                })}
              </div>
            </div>

            {/* Thanh Phòng thoại đang hoạt động */}
            <div className="p-4 rounded-2xl bg-[#261e3a]/80 border border-[#a2d9d3]/35 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 mt-4">
              <div className="flex items-center gap-3.5">
                <div className="w-10 h-10 rounded-xl bg-[#a2d9d3]/20 text-[#a2d9d3] grid place-items-center">
                  <Radio className="w-5 h-5 animate-pulse shrink-0" />
                </div>
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-xs font-bold text-[#f4effa]">
                      Phòng Họp Nhóm — Nhóm Đồ Án - UI/UX
                    </span>
                    <span className="text-xs px-2 py-0.5 rounded-full bg-[#a2d9d3] text-[#15121d] font-bold">
                      4 NGƯỜI
                    </span>
                  </div>
                  <p className="text-xs text-[#b7aabe] mt-0.5">
                    Thu Hà, Quốc Bảo, Minh Anh, Hoàng Long đang ở trong phòng
                    thoại
                  </p>
                </div>
              </div>

              <button
                onClick={() => onQuickJoinVoice("uiux", "Phòng Họp Nhóm")}
                className="workspace-action px-4 py-2 rounded-xl bg-[#a2d9d3] hover:bg-[#b5e5df] text-[#15121d] font-bold flex items-center gap-1.5 transition shrink-0"
              >
                <Headphones className="w-3.5 h-3.5 shrink-0" />
                <span>Tham gia ngay</span>
              </button>
            </div>
          </div>

          {/* CỘT PHẢI (4/12): TRÒ CHUYỆN TRỰC TIẾP (Cao bằng khung trái) */}
          <div className="lg:col-span-4 bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col justify-between shadow-lg min-h-0 overflow-y-auto">
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-x-3">
                <div>
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa] flex items-center gap-2">
                    <MessageSquare className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                    TRÒ CHUYỆN TRỰC TIẾP
                  </h2>
                  <p className="text-xs text-[#b7aabe] mt-0.5">
                    Nhắn tin riêng với bạn bè của bạn
                  </p>
                </div>
                <button
                  onClick={onOpenDm}
                  className="workspace-action font-semibold text-[#c8b2f1] hover:underline"
                >
                  Mở tất cả
                </button>
              </div>

              <div className="space-y-2.5">
                {[
                  {
                    init: "HĐ",
                    name: "Hoàng Đạt",
                    status: "Vừa gửi một tệp đính kèm",
                    time: "2 phút",
                    unread: true,
                  },
                  {
                    init: "TH",
                    name: "Thu Hà",
                    status: "Đang chỉnh sửa giao diện",
                    time: "Trực tuyến",
                    unread: false,
                  },
                  {
                    init: "QB",
                    name: "Quốc Bảo",
                    status: "Đang làm việc",
                    time: "Trực tuyến",
                    unread: false,
                  },
                  {
                    init: "MA",
                    name: "Minh Anh",
                    status: "Sẵn sàng trao đổi",
                    time: "Trực tuyến",
                    unread: false,
                  },
                ].map((friend, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={onOpenDm}
                    className="w-full p-3.5 rounded-xl bg-[#15121d]/80 hover:bg-[#15121d] border border-white/10 hover:border-[#c8b2f1]/40 flex items-center justify-between gap-2 text-left transition"
                  >
                    <span className="flex items-center gap-3 min-w-0">
                      <span className="w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center relative shrink-0">
                        <PreviewAvatar name={friend.name} initials={friend.init} />
                        <span className="w-2.5 h-2.5 rounded-full bg-[#a2d9d3] border-2 border-[#15121d] absolute bottom-0 right-0" />
                      </span>
                      <span className="min-w-0">
                        <span className="block text-sm font-bold text-[#f4effa] truncate">
                          {friend.name}
                        </span>
                        <span className="block text-xs text-[#b7aabe] truncate mt-0.5">
                          {friend.status}
                        </span>
                      </span>
                    </span>

                    <span className="flex items-center gap-2 shrink-0">
                      {friend.unread ? (
                        <span className="px-2 py-0.5 rounded-full bg-[#c8b2f1] text-[#271737] text-xs font-bold">
                          Mới
                        </span>
                      ) : (
                        <span className="text-xs text-[#a2d9d3]">
                          {friend.time}
                        </span>
                      )}
                      <ArrowRight aria-hidden="true" className="w-4 h-4 text-[#c8b2f1]" />
                    </span>
                  </button>
                ))}
              </div>
            </div>

            <button
              onClick={onOpenDm}
              className="workspace-action w-full px-3 py-2.5 rounded-xl bg-[#15121d] hover:bg-[#261e3a] border border-white/10 font-bold text-[#c8b2f1] transition mt-4"
            >
              Đi tới danh sách Bạn bè & Tin nhắn →
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
