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
} from "lucide-react";
import { PreviewAvatar, PreviewServerPhoto } from "./preview-images";
import { VOICE_ROOMS_DATA } from "./voice-stage-view";
import { useBackend, initials } from "@/lib/workspace-backend";

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
  onOpenDm: (name?: string) => void;
  onQuickJoinVoice: (serverId: string, roomName: string) => void;
}

export const HomeHubView: React.FC<HomeHubViewProps> = ({
  currentUser,
  servers,
  onSelectServer,
  onOpenDm,
  onQuickJoinVoice,
}) => {
  const backend = useBackend();
  const [creatingChannel, setCreatingChannel] = useState(false);
  const [channelName, setChannelName] = useState("");
  const [creating, setCreating] = useState(false);
  const activeVoiceRooms = backend ? backend.data.rooms.filter(room => room.type === "VOICE" && backend.data.voice_states.some(state => state.room_id === room.room_id && state.state === "CONNECTED" && backend.data.friends.some(person => person.user_id === state.user_id))).map(room => ({ name: room.room_name, serverId: room.channel_id!, friends: backend.data.voice_states.filter(state => state.room_id === room.room_id && state.state === "CONNECTED").filter(state => backend.data.friends.some(person => person.user_id === state.user_id)).map(state => ({ name: backend.nameOf(state.user_id), init: initials(backend.nameOf(state.user_id)) })) })) : Object.entries(VOICE_ROOMS_DATA).map(([name, room]) => ({
    name,
    serverId: "uiux",
    friends: room.participants.filter(person => ["Thu Hà", "Quốc Bảo", "Minh Anh", "Hoàng Long"].includes(person.name)),
  })).filter(room => room.friends.length > 0).slice(0, 3);
  const [inviteCode, setInviteCode] = useState("");
  const [joinNotice, setJoinNotice] = useState("");

  const handleJoinByCode = async (e: React.FormEvent) => {
    e.preventDefault();
    if (backend) {
      const code = inviteCode.trim().split('/').filter(Boolean).pop();
      if (!code) return;
      try { await backend.api.request(`/channel-links/${encodeURIComponent(code)}/join`, "POST"); await backend.refresh(); setJoinNotice("Đã tham gia kênh."); setInviteCode(""); } catch (error) { backend.report(error); }
      return;
    }
    if (!inviteCode.trim()) return;
    setJoinNotice(`Đang kết nối vào kênh "${inviteCode}"...`);
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
                {backend ? "Kết nối API" : "Dữ liệu mẫu"}
              </span>
            </div>

            <h1 className="text-2xl md:text-3xl font-bold tracking-tight text-[#f4effa]">
              Chào mừng trở lại,{" "}
              <span className="text-[#c8b2f1]">{currentUser}</span>!
            </h1>
            <p className="text-sm leading-relaxed text-[#b7aabe]">
              Chọn một kênh bên dưới để bắt đầu trò chuyện hoặc nhắn tin trực
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
              Tham gia kênh bằng mã mời
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
        {backend && backend.data.invitations.filter(invite => invite.invitee_id === backend.data.profile.user_id).map(invite => <div key={invite.invitation_id} className="flex flex-wrap items-center gap-3 rounded-xl border border-[#c8b2f1]/30 bg-[#1c172c] p-4 text-sm"><span className="flex-1">Lời mời vào kênh <strong>{invite.channel_name}</strong></span>{['ACCEPT','DECLINE'].map(decision => <button key={decision} onClick={() => void backend.api.request(`/invitations/${invite.invitation_id}`,'PATCH',{decision}).then(backend.refresh).catch(backend.report)} className="rounded-lg border border-white/15 px-3 py-2">{decision === 'ACCEPT' ? 'Chấp nhận lời mời kênh' : 'Từ chối'}</button>)}</div>)}
        <div className="lg:flex-1 grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch lg:min-h-0">
          {/* CỘT TRÁI (8/12): KÊNH TRUY CẬP GẦN ĐÂY (Đã bỏ phần mô tả kênh) */}
          <div className="lg:col-span-8 bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col justify-between shadow-lg min-h-0 overflow-y-auto">
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa] flex items-center gap-2">
                    <FolderGit2 className="w-4 h-4 text-[#c8b2f1] shrink-0" />
                    KÊNH TRUY CẬP GẦN ĐÂY ({servers.length})
                  </h2>
                  <p className="text-xs text-[#b7aabe] mt-0.5">
                    Những kênh bạn đã mở trong phiên này
                  </p>
                </div>

                <button onClick={() => setCreatingChannel(value => !value)} className="workspace-action px-3.5 py-2 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 font-semibold text-[#f4effa] flex items-center gap-1.5 transition">
                  <Plus className="w-3.5 h-3.5 text-[#c8b2f1] shrink-0" />
                  <span>Tạo kênh</span>
                </button>
              </div>

              {/* Lưới 2x2 Thẻ Kênh (Gọn gàng, không có mô tả) */}
              {backend && creatingChannel && <form onSubmit={async event => { event.preventDefault(); if (creating) return; setCreating(true); try { const value = await backend.api.request<{channel:{channel_id:string}}>('/channels', 'POST', {channel_name:channelName.trim()}); await backend.refresh(); setChannelName(''); setCreatingChannel(false); onSelectServer(value.channel.channel_id); } catch(error) { backend.report(error); } finally { setCreating(false); } }} className="flex flex-wrap items-end gap-3 rounded-xl bg-[#15121d] p-4"><label className="flex-1 text-sm">Tên kênh<input value={channelName} onChange={event => setChannelName(event.target.value)} required maxLength={100} className="mt-2 w-full rounded-xl border border-white/15 bg-[#1c172c] p-3"/></label><button disabled={creating || !channelName.trim()} className="rounded-xl bg-[#c8b2f1] px-4 py-3 text-sm font-semibold text-[#271737] disabled:opacity-50">{creating ? 'Đang tạo…' : 'Lưu kênh'}</button></form>}
              {servers.length === 0 && <p className="rounded-xl bg-[#15121d]/80 p-5 text-sm text-[#b7aabe]">Chưa có kênh truy cập gần đây. Chọn kênh từ thanh điều hướng phía trên.</p>}
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
                        aria-label={`Mở kênh ${srv.name}`}
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
                            {backend ? 'Thành viên trong kênh' : `• ${srv.onlineCount} trực tuyến`}
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

            <section aria-label="Bạn bè trong phòng thoại" className="mt-6 border-t border-white/10 pt-5">
              <div className="mb-3 flex items-center gap-2">
                <Headphones className="h-4 w-4 text-[#a2d9d3]" />
                <h2 className="text-sm font-semibold text-[#f4effa]">Bạn bè trong phòng thoại</h2>
                <span className="text-xs text-[#b7aabe]">{backend ? "Trạng thái từ API" : "Dữ liệu mẫu"}</span>
              </div>
              <div className="space-y-2">
                {activeVoiceRooms.map(room => (
                  <div key={room.name} className="flex flex-wrap items-center gap-3 rounded-xl border border-white/10 bg-[#15121d]/70 p-3">
                    <div className="min-w-0 flex-1 basis-36">
                      <p className="truncate text-sm font-semibold text-[#f4effa]">{room.name}</p>
                      <p className="mt-1 truncate text-xs text-[#b7aabe]">{backend ? backend.data.channels.find(channel => channel.channel_id === room.serverId)?.channel_name : 'Nhóm Đồ Án - UI/UX'}</p>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="flex -space-x-2" aria-label={room.friends.map(friend => friend.name).join(", ")}>
                        {room.friends.slice(0, 4).map(friend => (
                          <span key={friend.name} title={friend.name} className="relative block h-9 w-9 rounded-full border-2 border-[#15121d] bg-[#30243f] text-xs text-white">
                            <PreviewAvatar name={friend.name} initials={friend.init} />
                          </span>
                        ))}
                      </div>
                      <span className="whitespace-nowrap text-xs text-[#b7aabe]">{room.friends.length} bạn</span>
                    </div>
                    <button onClick={() => onQuickJoinVoice(room.serverId, room.name)} aria-label={`Tham gia ${room.name}`} className="workspace-action rounded-lg bg-[#a2d9d3] px-4 py-2 text-sm font-semibold text-[#15121d] hover:bg-[#b5e5df]">Tham gia</button>
                  </div>
                ))}
              </div>
            </section>
          </div>

          {/* CỘT PHẢI (4/12): TIN NHẮN GẦN ĐÂY (Cao bằng khung trái) */}
          <div className="lg:col-span-4 bg-[#1c172c]/70 border border-white/10 rounded-2xl p-6 flex flex-col justify-between shadow-lg min-h-0 overflow-y-auto">
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-x-3">
                <div>
                  <h2 className="text-xs font-bold uppercase tracking-wider text-[#f4effa] flex items-center gap-2">
                    <MessageSquare className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                    TIN NHẮN GẦN ĐÂY
                  </h2>
                  <p className="text-xs text-[#b7aabe] mt-0.5">
                    Những cuộc trò chuyện gần đây
                  </p>
                </div>
                <button
                  onClick={() => onOpenDm()}
                  className="workspace-action font-semibold text-[#c8b2f1] hover:underline"
                >
                  Mở tất cả
                </button>
              </div>

              <div className="space-y-2.5">
                {(backend ? backend.data.dms.map(dm => { const id = dm.member_ids?.find(id => id !== backend.data.profile.user_id) || ""; const name = backend.nameOf(id); return { init: initials(name), name, status: dm.is_read_only ? "Chỉ đọc" : "Mở cuộc trò chuyện", time: "", unread: dm.unread_count > 0 }; }) : [
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
                    status: "Mình đã cập nhật bản thiết kế.",
                    time: "Hôm nay",
                    unread: false,
                  },
                  {
                    init: "QB",
                    name: "Quốc Bảo",
                    status: "Hẹn gặp bạn ở phòng thoại nhé.",
                    time: "Hôm nay",
                    unread: false,
                  },
                  {
                    init: "MA",
                    name: "Minh Anh",
                    status: "Đã gửi tài liệu cho bạn.",
                    time: "Hôm nay",
                    unread: false,
                  },
                ]).map((friend, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={() => onOpenDm(friend.name)}
                    className="w-full p-3.5 rounded-xl bg-[#15121d]/80 hover:bg-[#15121d] border border-white/10 hover:border-[#c8b2f1]/40 flex items-center justify-between gap-2 text-left transition"
                  >
                    <span className="flex items-center gap-3 min-w-0">
                      <span className="w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center relative shrink-0">
                        <PreviewAvatar name={friend.name} initials={friend.init} />
                        <span hidden={!!backend} className="w-2.5 h-2.5 rounded-full bg-[#a2d9d3] border-2 border-[#15121d] absolute bottom-0 right-0" />
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
              onClick={() => onOpenDm()}
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
