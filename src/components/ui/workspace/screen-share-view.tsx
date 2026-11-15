import React, { useState } from "react";
import { PreviewAvatar } from "./preview-images";
import {
  ArrowLeft,
  UserPlus,
  Radio,
  Maximize2,
  Volume2,
  UserPlus2,
  Mic,
  MicOff,
  Headphones,
  Video,
  VideoOff,
  VolumeX,
  MonitorUp,
  SlidersHorizontal,
  PhoneOff,
} from "lucide-react";
import { ViewProps } from "./types";

export const ScreenShareView: React.FC<ViewProps> = ({
  currentUser,
  onNavigate,
}) => {
  const [focusedStream, setFocusedStream] = useState<
    "both" | "vscode" | "figma"
  >("both");
  const [micMuted, setMicMuted] = useState(false);
  const [deafened, setDeafened] = useState(false);
  const [cameraOn, setCameraOn] = useState(true);

  return (
    <div className="flex-1 min-h-0 w-full overflow-y-auto px-4 sm:px-6 md:px-10 xl:px-14 py-6">
      <div className="max-w-[1400px] mx-auto flex flex-col gap-5">
        {/* Thanh Điều hướng phụ */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2.5">
            <button className="px-3.5 py-2 rounded-xl bg-[#c8b2f1]/15 border border-[#c8b2f1]/40 text-[#c8b2f1] text-xs font-semibold flex items-center gap-1.5">
              <UserPlus className="w-3.5 h-3.5 shrink-0" /> Mời
            </button>
            <button
              onClick={() => onNavigate("overview")}
              className="px-3.5 py-2 rounded-xl bg-[#1c172c] hover:bg-white/10 border border-white/10 text-xs text-[#b7aabe] hover:text-[#f4effa] flex items-center gap-1.5 transition"
            >
              <ArrowLeft className="w-3.5 h-3.5 shrink-0" /> Tổng quan Kênh
            </button>
          </div>

          <div className="flex flex-wrap items-center gap-2.5">
            <span className="px-4 py-2 rounded-xl bg-[#1c172c] border border-[#a2d9d3]/40 text-xs text-[#a2d9d3] font-semibold flex items-center gap-2">
              <Radio className="w-3.5 h-3.5 shrink-0" /> Spatial Audio 3D •
            </span>
            {focusedStream !== "both" && (
              <button
                onClick={() => setFocusedStream("both")}
                className="px-4 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] text-xs font-bold"
              >
                Xem song song 2 màn hình
              </button>
            )}
          </div>
        </div>

        {/* Lưới 2 Màn hình Chia sẻ trực tiếp */}
        <div
          className={`grid gap-6 ${
            focusedStream === "both"
              ? "grid-cols-1 lg:grid-cols-2"
              : "grid-cols-1"
          }`}
        >
          {/* Stream 1: VS Code */}
          {(focusedStream === "both" || focusedStream === "vscode") && (
            <div className="rounded-2xl bg-[#1c172c] border border-white/10 overflow-hidden flex flex-col shadow-xl">
              <div className="px-4 py-3 bg-[#120f19] border-b border-white/10 flex items-center justify-between text-xs">
                <div className="flex items-center gap-2.5">
                  <span className="px-2 py-0.5 rounded bg-red-500 text-white text-xs font-bold">
                    • TRỰC TIẾP
                  </span>
                  <span className="font-semibold text-[#f4effa]">
                    microservice-auth.ts — Quốc Bảo
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="px-2 py-0.5 rounded bg-white/5 text-xs text-[#c8b2f1] font-mono">
                    1080p 60fps
                  </span>
                  <Volume2 className="w-3.5 h-3.5 text-[#a2d9d3] shrink-0" />
                </div>
              </div>

              <div className="px-4 py-2 bg-[#161222] border-b border-white/5 flex items-center justify-between text-xs text-[#b7aabe] font-mono">
                <div className="flex items-center gap-2">
                  <span className="w-2.5 h-2.5 rounded-full bg-red-400" />
                  <span className="w-2.5 h-2.5 rounded-full bg-yellow-400" />
                  <span className="w-2.5 h-2.5 rounded-full bg-green-400" />
                  <span className="ml-2 text-[#f4effa]">
                    apps/auth/src/verify-token.ts
                  </span>
                </div>
                <span className="text-[#a2d9d3]">
                  TypeScript Strict • UTF-8
                </span>
              </div>

              <pre className="p-5 text-xs font-mono text-[#e9ddff] bg-[#100d16] overflow-x-auto flex-1 leading-relaxed">
                {`01  import { FastifyRequest, FastifyReply } from 'fastify';
02  import { verifySessionToken } from '@spatial/crypto-mesh';
03  // Xác thực token phiên làm việc WebRTC phân tán
04  export async function authMiddleware(req: FastifyRequest, reply: FastifyReply) {
05    const bearer = req.headers['authorization']?.replace('Bearer ', '');
06    if (!bearer) return reply.status(401).send({ err: 'Chưa đăng nhập' });
07    const payload = await verifySessionToken(bearer, process.env.SESSION_SECRET);
08    if (!payload.isValid) throw new UnauthorizedException('Phiên hết hạn');
09    req.user = { id: payload.sub, guildId: payload.gid, room: '#ui-design' };
10  }`}
              </pre>

              <div className="p-4 border-t border-white/10 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="relative shrink-0 w-9 h-9 rounded-full bg-[#a2d9d3] text-[#15121d] font-bold text-xs grid place-items-center">
                    <PreviewAvatar name="Quốc Bảo" initials="QB" />
                  </div>
                  <div>
                    <div className="text-xs font-bold text-[#f4effa]">
                      Quốc Bảo
                    </div>
                    <div className="text-xs text-[#b7aabe] mt-0.5">
                      Đang chia sẻ ứng dụng: Visual Studio Code
                    </div>
                  </div>
                </div>
                <button
                  onClick={() =>
                    setFocusedStream(
                      focusedStream === "vscode" ? "both" : "vscode",
                    )
                  }
                  className="px-3.5 py-1.5 rounded-xl bg-white/10 hover:bg-[#c8b2f1] hover:text-[#271737] text-xs font-semibold flex items-center gap-1.5 transition"
                >
                  <Maximize2 className="w-3.5 h-3.5 shrink-0" />
                  <span>
                    {focusedStream === "vscode" ? "Thu nhỏ" : "Tập trung"}
                  </span>
                </button>
              </div>
            </div>
          )}

          {/* Stream 2: Figma */}
          {(focusedStream === "both" || focusedStream === "figma") && (
            <div className="rounded-2xl bg-[#1c172c] border border-white/10 overflow-hidden flex flex-col shadow-xl">
              <div className="px-4 py-3 bg-[#120f19] border-b border-white/10 flex items-center justify-between text-xs">
                <div className="flex items-center gap-2.5">
                  <span className="px-2 py-0.5 rounded bg-red-500 text-white text-xs font-bold">
                    • TRỰC TIẾP
                  </span>
                  <span className="font-semibold text-[#f4effa]">
                    Spatial UI Figma — Minh Anh
                  </span>
                </div>
                <span className="px-2 py-0.5 rounded bg-white/5 text-xs text-[#c8b2f1] font-mono">
                  1080p 60fps
                </span>
              </div>

              <div className="p-5 bg-[#100d16] flex-1 flex flex-col justify-between">
                <div className="grid grid-cols-3 gap-3.5 my-auto">
                  {[
                    { title: "Space Orbit Login", tag: "#15121d" },
                    { title: "Bento Workspace", tag: "#1c172c" },
                    { title: "3D Voice Stage", tag: "#c8b2f1" },
                  ].map((art, i) => (
                    <div
                      key={i}
                      className="h-40 rounded-xl bg-[#1c172c] border border-[#c8b2f1]/30 p-3.5 flex flex-col justify-between"
                    >
                      <div className="flex items-center justify-between">
                        <span className="w-6 h-6 rounded-full bg-[#c8b2f1]/20 text-[#c8b2f1] grid place-items-center text-xs font-bold">
                          0{i + 1}
                        </span>
                        <span className="text-xs font-mono text-[#a2d9d3]">
                          {art.tag}
                        </span>
                      </div>
                      <div>
                        <div className="text-xs font-semibold text-[#f4effa]">
                          {art.title}
                        </div>
                        <div className="text-xs text-[#b7aabe] mt-0.5">
                          Auto-layout v3
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
                <div className="mt-4">
                  <span className="px-3 py-1 rounded-lg bg-[#1c172c] border border-white/10 text-xs text-[#a2d9d3] font-mono">
                    • Artboard: Frame-Voice-Desktop-v2.fig
                  </span>
                </div>
              </div>

              <div className="p-4 border-t border-white/10 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="relative shrink-0 w-9 h-9 rounded-full bg-[#2a223d] text-[#c8b2f1] font-bold text-xs grid place-items-center">
                    <PreviewAvatar name="Minh Anh" initials="MA" />
                  </div>
                  <div>
                    <div className="text-xs font-bold text-[#f4effa]">
                      Minh Anh
                    </div>
                    <div className="text-xs text-[#b7aabe] mt-0.5">
                      Figma Workspace: Design System V3
                    </div>
                  </div>
                </div>
                <button
                  onClick={() =>
                    setFocusedStream(
                      focusedStream === "figma" ? "both" : "figma",
                    )
                  }
                  className="px-3 py-1.5 rounded-xl bg-white/10 hover:bg-[#c8b2f1] hover:text-[#271737] text-xs font-semibold flex items-center gap-1.5 transition"
                >
                  <Maximize2 className="w-3.5 h-3.5 shrink-0" />
                  <span>
                    {focusedStream === "figma" ? "Thu nhỏ" : "Phóng to"}
                  </span>
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Hàng 4 Thẻ Thành viên dưới cùng */}
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <div className="p-4 rounded-2xl bg-[#1c172c] border border-[#a2d9d3]/50 flex flex-col items-center text-center">
            <span className="self-start text-xs px-2.5 py-0.5 rounded-full bg-[#a2d9d3]/15 text-[#a2d9d3] font-semibold mb-2">
              • Đang nói
            </span>
            <div className="relative w-12 h-12 rounded-full bg-[#a2d9d3] text-[#15121d] font-bold text-sm grid place-items-center mb-2">
              <PreviewAvatar name="Thu Hà" initials="TH" />
            </div>
            <div className="text-xs font-bold text-[#f4effa]">Thu Hà</div>
            <div className="text-xs text-[#b7aabe] mt-0.5">
              UI/UX Designer
            </div>
          </div>

          <div className="p-4 rounded-2xl bg-[#1c172c] border border-white/10 flex flex-col items-center text-center">
            <span className="self-start text-xs px-2.5 py-0.5 rounded-full bg-white/5 text-[#b7aabe] mb-2">
              Đang nghe
            </span>
            <div className="relative w-12 h-12 rounded-full bg-[#261e3a] text-[#c8b2f1] font-bold text-sm grid place-items-center mb-2">
              <PreviewAvatar name="Hoàng Long" initials="HL" />
            </div>
            <div className="text-xs font-bold text-[#f4effa]">Hoàng Long</div>
            <div className="text-xs text-[#b7aabe] mt-0.5">
              Frontend Developer
            </div>
          </div>

          <div className="p-4 rounded-2xl bg-[#1c172c] border border-[#c8b2f1]/40 flex flex-col items-center text-center">
            <span className="self-start text-xs px-2.5 py-0.5 rounded-full bg-[#c8b2f1]/20 text-[#c8b2f1] font-semibold mb-2">
              Bạn
            </span>
            <div className="relative w-12 h-12 rounded-full bg-[#c8b2f1] text-[#271737] font-bold text-sm grid place-items-center mb-2">
              <PreviewAvatar name={currentUser} initials="NM" />
            </div>
            <div className="text-xs font-bold text-[#f4effa]">
              {currentUser}
            </div>
            <div className="text-xs text-[#b7aabe] mt-0.5">
              Product Lead
            </div>
          </div>

          <div className="p-4 rounded-2xl bg-[#1c172c]/50 border border-dashed border-white/15 flex flex-col items-center justify-center text-center">
            <div className="w-10 h-10 rounded-full bg-white/5 grid place-items-center text-[#c8b2f1] mb-2">
              <UserPlus2 className="w-4 h-4 shrink-0" />
            </div>
            <div className="text-xs font-bold text-[#f4effa]">
              Mời thành viên
            </div>
            <div className="text-xs text-[#b7aabe] mb-2">
              Thêm bạn vào phòng thoại
            </div>
            <button className="px-3 py-1 rounded-lg bg-white/10 hover:bg-white/15 text-xs">
              Gửi liên kết mời
            </button>
          </div>
        </div>

        {/* CỤM ĐIỀU KHIỂN CUỘC GỌI ĐỒNG BỘ 100% */}
        <div className="mx-auto flex flex-col items-center gap-2 pt-2">
          <div className="px-3 py-2.5 rounded-2xl bg-[#1c172c] border border-white/15 flex flex-wrap items-center justify-center gap-2 shadow-2xl">
            <button
              onClick={() => setMicMuted(!micMuted)}
              aria-label="Tắt mic"
              aria-pressed={micMuted}
              title={micMuted ? "Bật Mic" : "Tắt Mic"}
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
              title={
                deafened ? "Bật âm thanh tai nghe" : "Tắt âm thanh tai nghe"
              }
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
              onClick={() => onNavigate("voice")}
              title="Đang chia sẻ màn hình (Bấm để quay về Sân khấu Voice)"
              className="w-10 h-10 rounded-xl bg-[#a2d9d3] text-[#15121d] flex items-center justify-center shadow-[0_0_12px_rgba(162,217,211,0.4)] transition"
            >
              <MonitorUp className="w-4 h-4 shrink-0" />
            </button>
            <button
              title="Cài đặt âm thanh & chống ồn Krisp"
              className="w-10 h-10 rounded-xl bg-white/10 hover:bg-white/15 text-[#b7aabe] hover:text-[#f4effa] flex items-center justify-center transition"
            >
              <SlidersHorizontal className="w-4 h-4 shrink-0" />
            </button>
            <div className="w-[1px] h-6 bg-white/10 mx-0.5" />
            <button
              onClick={() => onNavigate("overview")}
              title="Rời phòng thoại"
              className="w-10 h-10 rounded-xl bg-[#9b2c42] text-white hover:bg-[#ac304a] flex items-center justify-center transition shadow-md"
            >
              <PhoneOff className="w-4 h-4 shrink-0" />
            </button>
          </div>
          <button
            onClick={() => onNavigate("chat")}
            className="text-xs text-[#b7aabe] hover:text-[#c8b2f1] transition font-medium"
          >
            Kênh & Tin nhắn ▲
          </button>
        </div>
      </div>
    </div>
  );
};
