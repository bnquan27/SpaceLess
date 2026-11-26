import React, { useState } from "react";
import { SharePicker } from "./share-picker";
import { useBackend } from "@/lib/workspace-backend";
import { PreviewAvatar } from "./preview-images";
import {
  ArrowLeft,
  Users,
  MonitorUp,
  Mic,
  MicOff,
  Headphones,
  Video,
  VideoOff,
  VolumeX,
  SlidersHorizontal,
  PhoneOff,
  Plus,
  Volume2,
} from "lucide-react";
import { ViewProps } from "./types";

export const VOICE_ROOMS_DATA: Record<
  string,
  {
    subtitle: string;
    participants: {
      init: string;
      name: string;
      badge: string;
      speaking: boolean;
      sub: string;
    }[];
  }
> = {
  "Phòng Họp Nhóm": {
    subtitle: "Thảo luận chung toàn nhóm",
    participants: [
      {
        init: "QB",
        name: "Quốc Bảo",
        badge: "• Đang nói",
        speaking: true,
        sub: "Đang phát biểu",
      },
      {
        init: "MA",
        name: "Minh Anh",
        badge: "Đang nghe",
        speaking: false,
        sub: "Đang nghe",
      },
      {
        init: "HL",
        name: "Hoàng Long",
        badge: "Đã tắt mic",
        speaking: false,
        sub: "Đang nghe",
      },
    ],
  },
  "Sảnh Chill & Âm nhạc": {
    subtitle: "Nghe nhạc & Trò chuyện thư giãn",
    participants: [
      {
        init: "BOT",
        name: "Bot Âm Nhạc",
        badge: "♫ Đang phát",
        speaking: true,
        sub: "Nhạc Lo-Fi thư giãn",
      },
      {
        init: "TH",
        name: "Thu Hà",
        badge: "Đang nghe",
        speaking: false,
        sub: "Đang nghe nhạc",
      },
    ],
  },
  "Phòng Dev & Code Review": {
    subtitle: "Trao đổi mã nguồn & giao diện",
    participants: [],
  },
};

export const VoiceStageView: React.FC<ViewProps> = ({
  currentUser,
  onNavigate,
  selectedVoiceRoom = "Phòng Họp Nhóm",
  onSelectVoiceRoom,
  screenShares = [],
  onShareStarted,
  onWatchShare,
  onStopSharing,
  onLeaveVoice,
  roomId,
  voiceRooms = [],
  onOpenInviteModal,
}) => {
  const backend = useBackend();
  const joinedHere = !!backend && backend.voice.connected && backend.voice.roomId === roomId;
  const [sharePickerOpen, setSharePickerOpen] = useState(false);
  const [previewMicMuted, setMicMuted] = useState(false);
  const micMuted = backend ? backend.voice.micMuted : previewMicMuted;
  const [deafened, setDeafened] = useState(false);
  const [previewCameraOn, setCameraOn] = useState(false);
  const cameraOn = backend ? backend.voice.cameraOn : previewCameraOn;

  const activeRoomName = backend ? selectedVoiceRoom : VOICE_ROOMS_DATA[selectedVoiceRoom]
    ? selectedVoiceRoom
    : "Phòng Họp Nhóm";
  const roomData = backend ? {subtitle:joinedHere ? "Đã kết nối LiveKit" : "Chưa tham gia phòng thoại",participants:[]} : VOICE_ROOMS_DATA[activeRoomName];

  const allParticipants = backend ? (joinedHere ? backend.voice.participants : (backend.data.voice_states.filter(state => state.room_id === roomId && state.state === 'CONNECTED').map(state => ({init:backend.nameOf(state.user_id).slice(0,2),name:backend.nameOf(state.user_id),badge:'Trong phòng thoại',speaking:false,sub:''})))) : [
    ...roomData.participants,
    {
      init: "NM",
      name: `${currentUser} (Bạn)`,
      badge: "• Bạn",
      speaking: !micMuted,
      sub: micMuted ? "Mic đang tắt" : "Mic đang hoạt động",
    },
  ];

  return (
    <div className={`flex-1 min-h-0 w-full overflow-y-auto px-4 sm:px-6 md:px-10 xl:px-14 pt-6 ${screenShares.some(share => share.isLocal) ? "pb-60" : "pb-6"}`}>
      {sharePickerOpen && onShareStarted && <SharePicker onClose={() => setSharePickerOpen(false)} onShareStarted={onShareStarted} />}
      <div className="max-w-[1400px] mx-auto flex flex-col gap-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2.5">
            <button
              onClick={() => onNavigate("overview")}
              className="workspace-action px-3.5 py-2 rounded-xl bg-[#1c172c] hover:bg-white/10 border border-white/10 text-[#b7aabe] hover:text-[#f4effa] flex items-center gap-1.5 transition"
            >
              <ArrowLeft className="w-3.5 h-3.5 shrink-0" /> Tổng quan
            </button>

            <div className="flex flex-wrap items-center gap-1 bg-[#1c172c] p-1 rounded-xl border border-white/10">
              {(backend ? voiceRooms.map(room => room.name) : Object.keys(VOICE_ROOMS_DATA)).map((rName) => (
                <button
                  key={rName}
                  onClick={() => onSelectVoiceRoom?.(rName)}
                  aria-pressed={activeRoomName === rName}
                  className={`workspace-action px-3 py-1 rounded-lg font-semibold transition flex items-center gap-1.5 ${
                    activeRoomName === rName
                      ? "bg-[#a2d9d3] text-[#15121d]"
                      : "text-[#b7aabe] hover:text-[#f4effa]"
                  }`}
                >
                  <Volume2 className="w-3.5 h-3.5 shrink-0" />
                  <span>{rName}</span>
                </button>
              ))}
            </div>
          </div>

          <button onClick={() => onOpenInviteModal?.()} className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] text-xs font-bold transition shadow-md">
            <Plus className="w-3.5 h-3.5 stroke-[2.5]" /> Mời bạn bè
          </button>
        </div>

        {/* Thanh Trạng thái Phòng thoại */}
        <div className="px-5 py-3.5 rounded-2xl bg-[#1c172c]/90 border border-white/10 flex flex-wrap items-center justify-between gap-4 text-xs shadow-lg">
          <div className="flex flex-wrap items-center gap-5">
            <span className="text-[#a2d9d3] font-bold">• {activeRoomName}</span>
            <span className="text-[#b7aabe]">{roomData.subtitle}</span>
            <span className="text-[#b7aabe] flex items-center gap-1.5">
              <Users className="w-3.5 h-3.5 shrink-0" />{" "}
              {allParticipants.length} thành viên
            </span>
          </div>
          {backend && !joinedHere && <button disabled={!roomId || backend.voice.busy} onClick={() => { if (roomId) void backend.voice.join(roomId); }} className="workspace-action rounded-xl bg-[#a2d9d3] px-4 py-2 text-xs font-bold text-[#15121d] disabled:opacity-50">{backend.voice.busy ? 'Đang kết nối…' : 'Tham gia phòng thoại'}</button>}
          <button
            disabled={!!backend && !joinedHere}
            onClick={() => setSharePickerOpen(true)}
            className="workspace-action px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-bold flex items-center gap-1.5 transition shadow"
          >
            <MonitorUp className="w-3.5 h-3.5 shrink-0" /> Chia sẻ màn hình
          </button>
          <button onClick={() => onNavigate("screen")} className="workspace-action rounded-xl border border-[#c8b2f1]/40 px-4 py-2 text-[#c8b2f1] hover:bg-white/10">Xem chia sẻ</button>
        </div>

        {/* Lưới Thẻ Thành viên */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-5">
          {allParticipants.map((p, idx) => (
            <div
              key={idx}
              className={`rounded-2xl bg-[#1c172c]/80 border p-7 flex flex-col items-center justify-center relative transition min-h-[215px] ${
                p.speaking
                  ? "border-[#a2d9d3] shadow-[0_0_24px_rgba(162,217,211,0.16)]"
                  : "border-white/10"
              }`}
            >
              <span
                className={`absolute top-4 right-4 px-2.5 py-0.5 rounded-full text-xs font-semibold border ${
                  p.speaking
                    ? "bg-[#a2d9d3]/20 border-[#a2d9d3]/40 text-[#a2d9d3]"
                    : "bg-white/5 border-white/10 text-[#b7aabe]"
                }`}
              >
                {p.badge}
              </span>

              <button
                type="button"
                disabled={!screenShares.some(share => share.owner === (p.name.endsWith(" (Bạn)") ? currentUser : p.name))}
                onClick={() => onWatchShare?.(p.name.endsWith(" (Bạn)") ? currentUser : p.name)}
                aria-label={`Xem màn hình của ${p.name}`}
                className={`relative w-18 h-18 rounded-full grid place-items-center text-lg font-bold mb-3.5 ${
                  p.speaking
                    ? "bg-[#a2d9d3] text-[#15121d] ring-4 ring-[#a2d9d3]/30"
                    : "bg-[#261e3a] text-[#c8b2f1]"
                }`}
              >
                <PreviewAvatar name={p.name} initials={p.init} />
              </button>
              {screenShares.some(share => share.owner === (p.name.endsWith(" (Bạn)") ? currentUser : p.name)) && <span className="mb-2 rounded bg-[#9b2c42] px-2 py-1 text-xs text-white">Đang chia sẻ</span>}

              <div className="font-bold text-base text-[#f4effa] mb-3">{p.name}</div>
              <span className="px-3 py-1 rounded-full bg-[#15121d] border border-white/10 text-xs text-[#a2d9d3]">
                {p.sub}
              </span>
            </div>
          ))}

          <button onClick={() => onOpenInviteModal?.()} className="rounded-2xl bg-[#1c172c]/40 border border-dashed border-white/15 hover:border-[#c8b2f1]/50 p-7 flex flex-col items-center justify-center text-center cursor-pointer transition min-h-[215px]">
            <div className="w-12 h-12 rounded-full bg-white/5 grid place-items-center text-[#c8b2f1] mb-3">
              <Plus className="w-5 h-5 shrink-0" />
            </div>
            <div className="font-bold text-sm text-[#f4effa] mb-1">
              Mời thêm bạn bè
            </div>
            <p className="text-xs text-[#b7aabe]">
              Mời thành viên cùng tham gia {activeRoomName}
            </p>
          </button>
        </div>

        {/* Cụm Điều khiển Cuộc gọi */}
        {screenShares.some(share => share.isLocal) && <button onClick={onStopSharing} className="workspace-action self-center rounded-lg bg-[#9b2c42] px-4 py-2 text-white">Dừng chia sẻ</button>}
        <div className="mx-auto flex flex-col items-center gap-2 pt-2">
          <div className="px-3 py-2.5 rounded-2xl bg-[#1c172c] border border-white/15 flex flex-wrap items-center justify-center gap-2 shadow-2xl">
            <button
              disabled={!!backend && !joinedHere}
              onClick={() => backend ? void backend.voice.mic() : setMicMuted(!micMuted)}
              aria-label="Tắt mic"
              aria-pressed={micMuted}
              title={micMuted ? "Bật mic" : "Tắt mic"}
              className={`w-10 h-10 rounded-xl flex items-center justify-center transition ${
                micMuted
                  ? "bg-red-500/20 text-red-300 border border-red-500/30"
                  : "bg-white/10 hover:bg-white/15 text-[#f4effa]"
              }`}
            >
              {micMuted ? (
                <MicOff className="w-4 h-4 shrink-0" />
              ) : (
                <Mic className="w-4 h-4 shrink-0" />
              )}
            </button>
            <button
              disabled={!!backend && !joinedHere}
              onClick={() => { if (backend) void backend.voice.listen(!deafened); setDeafened(!deafened); }}
              aria-label="Tắt âm thanh tai nghe"
              aria-pressed={deafened}
              title={deafened ? "Bật âm thanh tai nghe" : "Tắt âm thanh tai nghe"}
              className={`w-10 h-10 rounded-xl flex items-center justify-center transition ${
                deafened
                  ? "bg-red-500/20 text-red-300 border border-red-500/30"
                  : "bg-white/10 hover:bg-white/15 text-[#f4effa]"
              }`}
            >
              {deafened ? <VolumeX className="w-4 h-4" /> : <Headphones className="w-4 h-4" />}
            </button>
            <button
              disabled={!!backend && !joinedHere}
              onClick={() => backend ? void backend.voice.camera() : setCameraOn(!cameraOn)}
              aria-label="Camera"
              aria-pressed={cameraOn}
              title={cameraOn ? "Tắt camera" : "Bật camera"}
              className={`w-10 h-10 rounded-xl flex items-center justify-center transition ${
                cameraOn
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "bg-white/10 hover:bg-white/15 text-[#f4effa]"
              }`}
            >
              {cameraOn ? <Video className="w-4 h-4" /> : <VideoOff className="w-4 h-4" />}
            </button>
            <button
              onClick={() => setSharePickerOpen(true)}
              aria-label="Mở màn hình chia sẻ"
              disabled={!!backend && !joinedHere}
              title="Mở màn hình chia sẻ"
              className="w-10 h-10 rounded-xl bg-white/10 hover:bg-[#a2d9d3]/20 text-[#a2d9d3] border border-[#a2d9d3]/30 flex items-center justify-center transition"
            >
              <MonitorUp className="w-4 h-4 shrink-0" />
            </button>
            <button aria-label="Cài đặt âm thanh" title="Cài đặt âm thanh" className="w-10 h-10 rounded-xl bg-white/10 hover:bg-white/15 text-[#b7aabe] flex items-center justify-center transition">
              <SlidersHorizontal className="w-4 h-4 shrink-0" />
            </button>
            <div className="w-[1px] h-6 bg-white/10 mx-0.5" />
            <button
              onClick={() => { onLeaveVoice?.(); onNavigate("overview"); }}
              aria-label="Rời phòng thoại"
              title="Rời phòng thoại"
              className="w-10 h-10 rounded-xl bg-[#9b2c42] text-white hover:bg-[#ac304a] flex items-center justify-center transition shadow-md"
            >
              <PhoneOff className="w-4 h-4 shrink-0" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
