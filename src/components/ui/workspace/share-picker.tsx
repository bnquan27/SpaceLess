import { useEffect, useRef, useState } from "react";
import { Monitor, AppWindow, Video, X } from "lucide-react";
import { ShareVideo } from "./share-video";

export function SharePicker({ onClose, onShareStarted }: { onClose: () => void; onShareStarted: (stream: MediaStream) => void }) {
  const [tab, setTab] = useState("Ứng dụng");
  const [notice, setNotice] = useState("");
  const [source, setSource] = useState<MediaStream | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef<MediaStream | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; pending.current?.getTracks().forEach(track => track.stop()); };
  }, []);
  const options = [{ name:"Ứng dụng", icon:AppWindow }, { name:"Toàn màn hình", icon:Monitor }, { name:"Thiết bị", icon:Video }];
  const select = async () => {
    if (busy) return;
    if (!navigator.mediaDevices?.getDisplayMedia) { setNotice("Trình duyệt chưa hỗ trợ chọn màn hình."); return; }
    setBusy(true);
    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({ video:true });
      if (!mounted.current) { stream.getTracks().forEach(track => track.stop()); return; }
      pending.current?.getTracks().forEach(track => track.stop());
      pending.current = stream;
      setSource(stream);
      setNotice("");
      stream.getVideoTracks()[0]?.addEventListener("ended", () => {
        if (mounted.current && pending.current === stream) { pending.current = null; setSource(null); setNotice("Nguồn chia sẻ đã dừng."); }
      }, { once: true });
    } catch { if (mounted.current) setNotice("Chưa chọn nguồn chia sẻ."); }
    finally { if (mounted.current) setBusy(false); }
  };
  return <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/75 p-4" onClick={onClose}>
    <section role="dialog" aria-modal="true" aria-label="Chọn nguồn chia sẻ màn hình" onClick={e=>e.stopPropagation()} className="w-full max-w-3xl rounded-2xl border border-white/10 bg-[#211d2a] p-5 shadow-2xl">
      <div className="mb-4 flex items-center justify-between"><div><h2 className="text-lg font-semibold">Chia sẻ màn hình</h2><p className="mt-1 text-sm text-[#b7aabe]">Chọn nội dung bạn muốn chia sẻ trong phòng thoại</p></div><button aria-label="Đóng" className="workspace-icon-button rounded-lg hover:bg-white/10" onClick={onClose}><X size={18}/></button></div>
      <div role="tablist" className="grid grid-cols-3 gap-1 rounded-xl bg-[#14111a] p-1">{options.map(({name,icon:Icon}) => <button key={name} role="tab" aria-selected={tab===name} onClick={()=>setTab(name)} className={`flex items-center justify-center gap-2 rounded-lg p-3 text-sm ${tab===name ? "bg-[#3c324b] text-white" : "text-[#b7aabe]"}`}><Icon size={16}/>{name}</button>)}</div>
      <div className="my-5">{source ? <ShareVideo share={{ owner: "Bạn", stream: source, isLocal: true }} /> : <div className="flex min-h-52 flex-col items-center justify-center rounded-xl border border-white/10 bg-[#15121d] p-6 text-center"><Monitor size={36} className="text-[#c8b2f1]"/><p className="mt-4 font-semibold">{tab === "Thiết bị" ? "Thiết bị chia sẻ" : `Chọn ${tab.toLowerCase()}`}</p><p className="mt-2 max-w-md text-sm text-[#b7aabe]">{tab === "Thiết bị" ? "Nguồn thiết bị sẽ xuất hiện khi được hỗ trợ." : "Danh sách nguồn sẽ do hệ điều hành hoặc trình duyệt cung cấp."}</p></div>}</div>
      {notice && <p role="status" className="mb-3 text-sm text-[#c8b2f1]">{notice}</p>}
      <div className="flex flex-wrap justify-end gap-2"><button onClick={onClose} className="rounded-lg px-4 py-2 text-sm hover:bg-white/10">Hủy</button><button disabled={tab==="Thiết bị" || busy} onClick={select} className="rounded-lg bg-white/10 px-4 py-2 text-sm disabled:opacity-50">{busy ? "Đang chọn…" : source ? "Đổi nguồn" : "Chọn nguồn"}</button>{source && <button onClick={() => { if (source.getVideoTracks()[0]?.readyState !== "live") return; pending.current = null; onShareStarted(source); onClose(); }} className="rounded-lg bg-[#c8b2f1] px-4 py-2 text-sm font-semibold text-[#271737]">Chia sẻ</button>}</div>
    </section>
  </div>;
}
