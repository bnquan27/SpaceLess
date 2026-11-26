import React from "react";
import { PreviewAvatar } from "./preview-images";
import {
  Hash,
  Volume2,
  Plus,
  Settings,
  Info,
  ArrowRight,
  Sparkles,
  Lock,
  Users,
  MessageSquare,
  Headphones,
  Shield,
  Crown,
} from "lucide-react";
import { ViewProps } from "./types";
import { useBackend, initials } from "@/lib/workspace-backend";

export const OverviewView: React.FC<ViewProps> = ({
  onNavigate,
  isCreator = true,
  chatChannels = [],
  voiceRooms = [],
  onSelectChatChannel,
  onSelectVoiceRoom,
  onOpenInviteModal,
  onOpenSettingsModal,
  onOpenCreateModal,
  channelId,
}) => {
  const backend = useBackend();
  const channel = backend?.data.channels.find(channel => channel.channel_id === channelId);
  const openChannel = (channelName: string) => {
    if (onSelectChatChannel) {
      onSelectChatChannel(channelName);
    } else {
      onNavigate("chat");
    }
  };

  const openVoice = (roomName: string) => {
    if (onSelectVoiceRoom) {
      onSelectVoiceRoom(roomName);
    } else {
      onNavigate("voice");
    }
  };

  return (
    <div className="flex-1 w-full h-full overflow-y-auto px-6 md:px-10 xl:px-14 pt-6 pb-16">
      <div className="max-w-[1400px] mx-auto flex flex-col gap-6">
        {/* 1. HERO BANNER CARD */}
        <section className="w-full bg-gradient-to-r from-[#1c172c] via-[#231c36] to-[#1c172c] border border-white/10 rounded-2xl p-6 md:p-7 flex flex-col md:flex-row items-start md:items-center justify-between gap-6 shadow-xl">
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2 text-xs font-semibold tracking-wider uppercase">
              <span className="px-2.5 py-1 rounded-full bg-[#a2d9d3]/15 border border-[#a2d9d3]/30 text-[#a2d9d3]">
                {backend ? 'Kết nối API' : '• TRỰC TUYẾN'}
              </span>
              {isCreator && (
                <span className="px-2.5 py-1 rounded-full bg-[#c8b2f1]/15 border border-[#c8b2f1]/30 text-[#c8b2f1] flex items-center gap-1">
                  <Crown size={11} /> Chủ kênh (Creator)
                </span>
              )}
            </div>

            <div className="flex items-center gap-3">
              <h1 className="text-2xl md:text-[28px] font-bold tracking-tight text-[#f4effa]">
                {backend ? channel?.channel_name || 'Chưa có kênh' : 'Nhóm Đồ Án - UI/UX'}
              </h1>
              <span className="w-6 h-6 rounded-full bg-[#c8b2f1]/15 border border-[#c8b2f1]/30 text-[#c8b2f1] grid place-items-center text-xs">
                <Sparkles className="w-3.5 h-3.5" />
              </span>
            </div>

            <div className="flex flex-wrap items-center gap-2.5 pt-1">
              <button
                onClick={() => onOpenInviteModal?.()}
                className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] text-xs font-bold transition shadow-md"
              >
                <Plus className="w-3.5 h-3.5 stroke-[2.5]" />
                <span>Mời cộng tác</span>
              </button>

              <button
                onClick={() => onOpenSettingsModal?.(0)}
                className="flex items-center gap-1.5 px-3.5 py-2 rounded-xl bg-[#15121d]/80 hover:bg-white/10 text-xs text-[#f4effa] transition border border-white/10"
              >
                <Settings className="w-3.5 h-3.5 text-[#b7aabe]" />
                <span>Hồ sơ & Cài đặt</span>
              </button>

              {isCreator && (
                <button
                  onClick={() => onOpenSettingsModal?.(2)}
                  className="flex items-center gap-1.5 px-3.5 py-2 rounded-xl bg-[#15121d]/80 hover:bg-[#c8b2f1]/15 text-xs text-[#c8b2f1] transition border border-[#c8b2f1]/30"
                >
                  <Shield className="w-3.5 h-3.5" />
                  <span>Quản lý Phân quyền</span>
                </button>
              )}

              <button
                onClick={() => onOpenSettingsModal?.(0)}
                title="Thông tin Kênh"
                className="p-2 rounded-xl bg-[#15121d]/80 hover:bg-white/10 text-[#b7aabe] hover:text-[#f4effa] transition border border-white/10"
              >
                <Info className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          {/* Members Metric Badge */}
          <div className="bg-[#15121d]/90 border border-white/10 rounded-2xl p-4 min-w-[280px] shadow-inner">
            <div className="flex items-center justify-between mb-3">
              <span className="text-xs text-[#f4effa] flex items-center gap-1.5 font-semibold">
                <Users className="w-3.5 h-3.5 text-[#a2d9d3]" /> Thành viên
              </span>
              <span className="px-2.5 py-0.5 rounded-full text-xs bg-[#a2d9d3]/15 text-[#a2d9d3] font-semibold">
                {backend ? channel?.members.length || 0 : 12} Thành viên
              </span>
            </div>

            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center -space-x-1.5">
                {(backend ? (channel?.members || []).slice(0,4).map(member => [member.full_name, initials(member.full_name)]) : [["Quốc Bảo", "QB"], ["Minh Anh", "MA"], ["Thu Hà", "TH"], ["Hoàng Long", "HL"]]).map(([name, initials]) => (
                  <div
                    key={name}
                    className="w-8 h-8 rounded-full bg-[#261e3a] border-2 border-[#15121d] text-xs font-bold flex items-center justify-center text-[#f4effa] relative"
                  >
                    <PreviewAvatar name={name} initials={initials} />
                    <span hidden={!!backend} className="w-2 h-2 rounded-full bg-[#a2d9d3] absolute bottom-0 right-0 border border-[#15121d]" />
                  </div>
                ))}
                <div className="w-8 h-8 rounded-full bg-[#1c172c] border-2 border-[#15121d] text-xs font-bold flex items-center justify-center text-[#b7aabe]">
                  +{backend ? Math.max(0,(channel?.members.length || 0)-4) : 8}
                </div>
              </div>
              <button
                onClick={() => onOpenInviteModal?.()}
                className="px-3 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-medium text-[#f4effa] transition"
              >
                + Thêm
              </button>
            </div>

            <div className="flex items-center justify-between text-xs text-[#b7aabe] pt-2.5 border-t border-white/10">
              <span className="text-[#a2d9d3] font-medium">{backend ? 'Thành viên trong kênh' : '• 4 trực tuyến'}</span>
              <span>{backend ? 'Không công bố trạng thái hiện diện' : '• 8 ngoại tuyến'}</span>
            </div>
          </div>
        </section>

        {/* 2. BENTO WORKSPACE GRID */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start">
          {/* LEFT COLUMN: PHÒNG TRÒ CHUYỆN */}
          <div className="bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col gap-5 shadow-lg">
            <div className="flex items-center justify-between">
              <div>
                <div className="flex items-center gap-2">
                  <MessageSquare className="w-4 h-4 text-[#a2d9d3]" />
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa]">
                    PHÒNG TRÒ CHUYỆN
                  </h2>
                </div>
                <p className="text-xs text-[#b7aabe] mt-0.5">
                  {chatChannels.length} phòng văn bản đang hoạt động
                </p>
              </div>

              {isCreator ? (
                <button
                  onClick={() => onOpenCreateModal?.("chat")}
                  className="flex items-center gap-1.5 text-xs text-[#f4effa] px-3 py-1.5 rounded-xl bg-white/5 hover:bg-[#c8b2f1] hover:text-[#271737] border border-white/10 font-semibold transition"
                >
                  <Plus className="w-3.5 h-3.5 text-[#c8b2f1]" />
                  <span>Tạo phòng</span>
                </button>
              ) : (
                <span className="flex items-center gap-1 text-xs text-[#b7aabe]/60 px-2.5 py-1 rounded-lg bg-[#15121d]/60 border border-white/5">
                  <Lock className="w-3 h-3 shrink-0" /> Chỉ Chủ kênh
                </span>
              )}
            </div>

            <div className="space-y-3">
              {chatChannels.map((ch, idx) => {
                const isMain = idx === 0;
                return (
                  <button
                    key={ch.id}
                    type="button"
                    aria-label={`Mở phòng ${ch.name}`}
                    onClick={() => openChannel(ch.name)}
                    className={`w-full text-left flex items-center justify-between gap-2 px-4 py-3.5 rounded-xl border transition group ${
                      isMain
                        ? "bg-[#261e3a]/90 border-[#c8b2f1]/45 hover:border-[#c8b2f1]"
                        : "bg-[#15121d]/75 hover:bg-[#15121d] border-white/10 hover:border-[#c8b2f1]/40"
                    }`}
                  >
                    <span className="min-w-0 flex items-center gap-3.5">
                      <span
                        className={`w-9 h-9 shrink-0 rounded-xl border border-white/10 flex items-center justify-center font-bold transition ${
                          isMain
                            ? "bg-[#15121d] text-[#a2d9d3]"
                            : "bg-[#1c172c] text-[#b7aabe] group-hover:text-[#c8b2f1]"
                        }`}
                      >
                        <Hash size={16} />
                      </span>
                      <span className="min-w-0">
                        <span className="flex flex-wrap items-center gap-x-2">
                          <span className="font-bold text-sm text-[#f4effa] wrap-anywhere">
                            {ch.name}
                          </span>
                          {ch.tag && (
                            <span className="text-xs px-2 py-0.5 rounded bg-[#a2d9d3]/20 text-[#a2d9d3] font-bold">
                              {ch.tag}
                            </span>
                          )}
                          {ch.dot && (
                            <span hidden={!!backend} className="w-2 h-2 rounded-full bg-[#a2d9d3]" />
                          )}
                        </span>
                        <span className="block text-xs text-[#b7aabe] mt-0.5">
                          {ch.sub}
                        </span>
                      </span>
                    </span>
                    <span className="flex items-center gap-2.5 shrink-0">
                      {ch.badge && (
                        <span className="px-2.5 py-0.5 rounded-full bg-[#a2d9d3] text-[#15121d] text-xs font-bold">
                          {ch.badge}
                        </span>
                      )}
                      <span className="w-7 h-7 rounded-lg bg-white/5 group-hover:bg-[#c8b2f1]/20 grid place-items-center text-xs text-[#b7aabe] group-hover:text-[#c8b2f1] transition">
                        <ArrowRight size={14} />
                      </span>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* RIGHT COLUMN: PHÒNG ĐÀM THOẠI */}
          <div className="bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col gap-5 shadow-lg">
            <div className="flex items-center justify-between">
              <div>
                <div className="flex items-center gap-2">
                  <Volume2 className="w-4 h-4 text-[#a2d9d3]" />
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa]">
                    PHÒNG ĐÀM THOẠI
                  </h2>
                </div>
                <p className="text-xs text-[#b7aabe] mt-0.5">
                  {voiceRooms.length + 1} phòng đàm thoại
                </p>
              </div>

              {isCreator ? (
                <button
                  onClick={() => onOpenCreateModal?.("voice")}
                  className="flex items-center gap-1.5 text-xs text-[#f4effa] px-3 py-1.5 rounded-xl bg-white/5 hover:bg-[#a2d9d3] hover:text-[#15121d] border border-white/10 font-semibold transition"
                >
                  <Plus className="w-3.5 h-3.5 text-[#a2d9d3]" />
                  <span>Tạo phòng</span>
                </button>
              ) : (
                <span className="flex items-center gap-1 text-xs text-[#b7aabe]/60 px-2.5 py-1 rounded-lg bg-[#15121d]/60 border border-white/5">
                  <Lock className="w-3 h-3 shrink-0" /> Chỉ Chủ kênh
                </span>
              )}
            </div>

            {/* Other Voice Rooms */}
            <div className="space-y-2.5">
              {(backend ? voiceRooms : [{ id: "preview-meeting", name: "Phòng Họp Nhóm", sub: "3 người trong phòng", locked: false }, ...voiceRooms]).map((vr) =>
                vr.locked && !isCreator ? (
                  <div
                    key={vr.id}
                    className="flex items-center justify-between px-4 py-3 rounded-xl bg-[#15121d]/40 opacity-55 border border-white/10 cursor-not-allowed"
                  >
                    <div className="flex items-center gap-3">
                      <div className="w-9 h-9 rounded-xl bg-[#1c172c] border border-white/10 text-[#b7aabe] flex items-center justify-center">
                        <Lock className="w-4 h-4" />
                      </div>
                      <div>
                        <span className="font-semibold text-sm text-[#b7aabe]">
                          {vr.name}
                        </span>
                        <p className="text-xs text-[#b7aabe] mt-0.5">
                          {vr.sub}
                        </p>
                      </div>
                    </div>
                    <Lock className="w-3.5 h-3.5 text-[#b7aabe]" />
                  </div>
                ) : (
                  <button
                    key={vr.id}
                    type="button"
                    aria-label={`Vào phòng ${vr.name}`}
                    onClick={() => openVoice(vr.name)}
                    className="w-full text-left flex items-center justify-between gap-2 px-4 py-3 rounded-xl bg-[#15121d]/75 hover:bg-[#15121d] border border-white/10 hover:border-[#a2d9d3]/40 transition"
                  >
                    <span className="min-w-0 flex items-center gap-3">
                      <span className="w-9 h-9 shrink-0 rounded-xl bg-[#1c172c] border border-white/10 text-[#a2d9d3] flex items-center justify-center">
                        <Headphones className="w-4 h-4" />
                      </span>
                      <span>
                        <span className="font-semibold text-sm text-[#f4effa]">
                          {vr.name}
                        </span>
                        <span className="block text-xs text-[#b7aabe] mt-0.5">
                          {vr.sub}
                        </span>
                      </span>
                    </span>
                    <span className="shrink-0 rounded-lg text-sm font-semibold text-[#a2d9d3]">
                      Vào phòng
                    </span>
                  </button>
                ),
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
