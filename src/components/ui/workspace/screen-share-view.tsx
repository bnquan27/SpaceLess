import { ArrowLeft, MonitorPlay } from "lucide-react";
import { ViewProps } from "./types";
import { ShareVideo } from "./share-video";
import { PreviewAvatar } from "./preview-images";

export function ScreenShareView({ onNavigate, selectedVoiceRoom = "Phòng thoại", screenShares = [], viewedOwner, onWatchShare, onStopSharing }: ViewProps & { viewedOwner?: string }) {
  const selected = screenShares.find(share => share.owner === viewedOwner) || screenShares[0];
  return <main className="flex-1 min-h-0 overflow-y-auto bg-[#100d16] p-4 sm:p-6">
    <div className="mx-auto max-w-[1500px]">
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3">
        <button onClick={() => onNavigate("voice")} className="workspace-action flex items-center gap-2 rounded-xl border border-white/10 bg-[#1c172c] px-4 text-[#d1c7d9] hover:bg-[#30243f]"><ArrowLeft size={16}/> Quay lại phòng thoại</button>
        <span className="text-sm text-[#a2d9d3]">{selectedVoiceRoom}</span>
      </div>
      {selected ? <>
        <h1 className="mb-3 text-lg font-semibold">{selected.isLocal ? "Màn hình của bạn" : `Màn hình của ${selected.owner}`}</h1>
        {selected.isLocal && <p className="mb-3 text-sm text-[#b7aabe]">Đang xem nguồn trên máy. Chia sẻ tới thành viên khác cần kết nối LiveKit.</p>}
        <ShareVideo share={selected} onStop={selected.isLocal ? onStopSharing : undefined}/>
        <div className="mt-4 flex flex-wrap gap-3" aria-label="Người đang chia sẻ">{screenShares.map(share => <button key={share.owner} onClick={() => onWatchShare?.(share.owner)} aria-pressed={selected.owner === share.owner} className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-sm ${selected.owner === share.owner ? "border-[#c8b2f1] bg-[#30243f]" : "border-white/10 bg-[#1c172c]"}`}>
          <span className="relative h-10 w-10 rounded-full bg-[#30243f]"><PreviewAvatar name={share.owner} initials={share.owner.slice(0,2)} /></span>
          {share.owner}{share.isLocal ? " (Bạn)" : ""}
        </button>)}</div>
      </> : <div className="flex min-h-80 flex-col items-center justify-center rounded-2xl border border-white/10 bg-[#1c172c] p-8 text-center">
        <MonitorPlay size={36} className="text-[#c8b2f1]"/><h1 className="mt-5 text-lg font-semibold">Chưa có màn hình để xem</h1>
        <p className="mt-2 max-w-md text-sm leading-6 text-[#b7aabe]">Màn hình của thành viên sẽ xuất hiện tại đây khi có luồng chia sẻ.</p>
      </div>}
    </div>
  </main>;
}
