import React, { useState } from "react";
import { PreviewAvatar } from "./preview-images";
import {
  ArrowLeft,
  UserPlus,
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

const VOICE_ROOMS_DATA: Record<
  string,
  {
    subtitle: string;
    participants: {
      init: string;
      name: string;
      role: string;
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
        role: "Trưởng nhóm",
        badge: "• Đang nói",
        speaking: true,
        sub: "Đang phát biểu",
      },
      {
        init: "TH",
        name: "Thu Hà",
        role: "UI/UX Designer",
        badge: "Mic bật",
        speaking: false,
        sub: "Sẵn sàng trò chuyện",
      },
      {
        init: "MA",
        name: "Minh Anh",
        role: "Design Lead",
        badge: "Đang nghe",
        speaking: false,
        sub: "Đang nghe",
      },
      {
        init: "HL",
        name: "Hoàng Long",
        role: "Frontend",
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
        role: "Phát nhạc tự động",
        badge: "♫ Đang phát",
        speaking: true,
        sub: "Nhạc Lo-Fi thư giãn",
      },
      {
        init: "TH",
        name: "Thu Hà",
        role: "UI/UX Designer",
        badge: "Đang nghe",
        speaking: false,
        sub: "Đang nghe nhạc",
      },
    ],
  },
  "Phòng Dev & Code Review": {
    subtitle: "Trao đổi mã nguồn & giao diện",
    participants: [
      {
        init: "QB",
        name: "Quốc Bảo",
        role: "Trưởng nhóm",
        badge: "• Đang chia sẻ",
        speaking: true,
        sub: "Sẵn sàng trao đổi",
      },
    ],
  },
};

export const VoiceStageView: React.FC<ViewProps> = ({
  currentUser,
  onNavigate,
  selectedVoiceRoom = "Phòng Họp Nhóm",
  onSelectVoiceRoom,
}) => {
  const [micMuted, setMicMuted] = useState(false);
  const [deafened, setDeafened] = useState(false);
  const [cameraOn, setCameraOn] = useState(false);

  const activeRoomName = VOICE_ROOMS_DATA[selectedVoiceRoom]
    ? selectedVoiceRoom
    : "Phòng Họp Nhóm";
  const roomData = VOICE_ROOMS_DATA[activeRoomName];

  const allParticipants = [
    ...roomData.participants,
    {
      init: "NM",
      name: `${currentUser} (Bạn)`,
      role: "Chủ kênh",
      badge: "• Bạn",
      speaking: !micMuted,
      sub: micMuted ? "Mic đang tắt" : "Mic đang hoạt động",
    },
  ];

  return (
    <div className="flex-1 min-h-0 w-full overflow-y-auto px-4 sm:px-6 md:px-10 xl:px-14 py-6">
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
              {Object.keys(VOICE_ROOMS_DATA).map((rName) => (
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

          <button className="px-3.5 py-2 rounded-xl bg-[#c8b2f1]/15 border border-[#c8b2f1]/40 text-[#c8b2f1] text-xs font-semibold flex items-center gap-1.5">
            <UserPlus className="w-3.5 h-3.5 shrink-0" /> Mời bạn bè
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
          <button
            onClick={() => onNavigate("screen")}
            className="workspace-action px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-bold flex items-center gap-1.5 transition shadow"
          >
            <MonitorUp className="w-3.5 h-3.5 shrink-0" /> Chia sẻ màn hình
          </button>
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

              <div
                className={`relative w-18 h-18 rounded-full grid place-items-center text-lg font-bold mb-3.5 ${
                  p.speaking
                    ? "bg-[#a2d9d3] text-[#15121d] ring-4 ring-[#a2d9d3]/30"
                    : "bg-[#261e3a] text-[#c8b2f1]"
                }`}
              >
                <PreviewAvatar name={p.name} initials={p.init} />
              </div>

              <div className="font-bold text-base text-[#f4effa]">{p.name}</div>
              <div className="text-xs text-[#b7aabe] mt-0.5 mb-3">{p.role}</div>
              <span className="px-3 py-1 rounded-full bg-[#15121d] border border-white/10 text-xs text-[#a2d9d3]">
                {p.sub}
              </span>
            </div>
          ))}

          <div className="rounded-2xl bg-[#1c172c]/40 border border-dashed border-white/15 hover:border-[#c8b2f1]/50 p-7 flex flex-col items-center justify-center text-center cursor-pointer transition min-h-[215px]">
            <div className="w-12 h-12 rounded-full bg-white/5 grid place-items-center text-[#c8b2f1] mb-3">
              <Plus className="w-5 h-5 shrink-0" />
            </div>
            <div className="font-bold text-sm text-[#f4effa] mb-1">
              Mời thêm bạn bè
            </div>
            <p className="text-xs text-[#b7aabe]">
              Mời thành viên cùng tham gia {activeRoomName}
            </p>
          </div>
        </div>

        {/* Cụm Điều khiển Cuộc gọi */}
        <div className="mx-auto flex flex-col items-center gap-2 pt-2">
          <div className="px-3 py-2.5 rounded-2xl bg-[#1c172c] border border-white/15 flex flex-wrap items-center justify-center gap-2 shadow-2xl">
            <button
              onClick={() => setMicMuted(!micMuted)}
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
              onClick={() => setDeafened(!deafened)}
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
              onClick={() => setCameraOn(!cameraOn)}
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
              onClick={() => onNavigate("screen")}
              aria-label="Mở màn hình chia sẻ"
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
              onClick={() => onNavigate("overview")}
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
