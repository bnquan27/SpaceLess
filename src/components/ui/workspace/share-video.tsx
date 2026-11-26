import { useEffect, useRef, useState } from "react";
import { Maximize, Minimize, MonitorOff } from "lucide-react";

interface ScreenShareSession {
  owner: string;
  stream: MediaStream;
  isLocal: boolean;
}

export function ShareVideo({ share, compact = false, onOpen, onStop }: {
  share: ScreenShareSession;
  compact?: boolean;
  onOpen?: () => void;
  onStop?: () => void;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [windowExpanded, setWindowExpanded] = useState(false);
  const [notice, setNotice] = useState("");

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    let cancelled = false;
    video.srcObject = share.stream;
    void video.play().catch(() => { if (!cancelled) setNotice("Bấm vào video để phát."); });
    return () => { cancelled = true; video.srcObject = null; };
  }, [share.stream]);
  useEffect(() => {
    const update = () => setFullscreen(document.fullscreenElement === frameRef.current);
    document.addEventListener("fullscreenchange", update);
    return () => document.removeEventListener("fullscreenchange", update);
  }, []);
  useEffect(() => {
    if (!windowExpanded) return;
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setWindowExpanded(false); };
    document.addEventListener("keydown", escape);
    return () => document.removeEventListener("keydown", escape);
  }, [windowExpanded]);

  const toggleFullscreen = async () => {
    if (windowExpanded) { setWindowExpanded(false); return; }
    try {
      if (document.fullscreenElement === frameRef.current) await document.exitFullscreen();
      else await frameRef.current?.requestFullscreen();
    } catch { setWindowExpanded(true); }
  };

  const expanded = fullscreen || windowExpanded;
  return <div ref={frameRef} className={`flex w-full flex-col overflow-hidden border border-white/15 bg-black ${windowExpanded ? "fixed inset-0 z-[110]" : "relative rounded-xl"}`}>
    <video ref={videoRef} autoPlay playsInline muted={share.isLocal} aria-label={`Màn hình chia sẻ của ${share.owner}`}
      onClick={() => { void videoRef.current?.play(); onOpen?.(); }}
      onPlaying={() => setNotice("")}
      className={`w-full bg-black object-contain ${expanded ? "min-h-0 flex-1" : compact ? "aspect-video max-h-40 cursor-pointer" : "aspect-video max-h-[65dvh]"}`} />
    <div className="flex items-center justify-between gap-2 bg-[#1c172c] px-3 py-2 text-xs text-[#eee7f5]">
      <span className="min-w-0 truncate">{share.isLocal ? "Màn hình của bạn · Xem trên máy" : `${share.owner} · Trực tiếp`}</span>
      <div className="flex shrink-0 gap-1">
        <button aria-label={expanded ? "Thu nhỏ màn hình" : "Toàn màn hình"} onClick={toggleFullscreen} className="workspace-icon-button rounded-md hover:bg-white/10">{expanded ? <Minimize size={16}/> : <Maximize size={16}/>}</button>
        {onStop && <button aria-label="Dừng chia sẻ màn hình" onClick={onStop} className="workspace-icon-button rounded-md text-[#ffa8b5] hover:bg-white/10"><MonitorOff size={16}/></button>}
      </div>
    </div>
    {notice && <button className="bg-[#1c172c] px-3 pb-2 text-left text-xs text-[#c8b2f1]" onClick={() => { void videoRef.current?.play(); setNotice(""); }}>{notice}</button>}
  </div>;
}
