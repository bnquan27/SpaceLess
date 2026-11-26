import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { SmilePlus, MoreHorizontal, Copy, Reply, Forward, Pin, Bookmark, Bell, Headphones, Link2, Info, Trash2, Flag, Fingerprint } from "lucide-react";

const reactions = ["❤️", "👍", "😂", "🔥"];

export function MessageActions({ text, messageId, isMe, onReply, onPin, onDelete, children }: {
  text: string;
  messageId: string;
  isMe: boolean;
  onReply: (text: string) => void;
  onPin?: (text: string, pinned: boolean) => void;
  onDelete: () => void;
  children: ReactNode;
}) {
  const [reaction, setReaction] = useState("");
  const [pinned, setPinned] = useState(false);
  const [saved, setSaved] = useState(false);
  const [unread, setUnread] = useState(false);
  const [notice, setNotice] = useState("");
  const [popup, setPopup] = useState<{ x: number; y: number; picker: boolean } | null>(null);
  const popupRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);

  useLayoutEffect(() => {
    if (!popup || !popupRef.current) return;
    const menu = popupRef.current;
    const bounds = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(popup.x, window.innerWidth - bounds.width - 8))}px`;
    menu.style.top = `${Math.max(8, Math.min(popup.y, window.innerHeight - bounds.height - 8))}px`;
    menu.querySelector<HTMLButtonElement>("button")?.focus();
  }, [popup]);

  useEffect(() => {
    if (!popup) return;
    const outside = (event: PointerEvent) => {
      if (!popupRef.current?.contains(event.target as Node)) setPopup(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setPopup(null); moreRef.current?.focus(); }
    };
    const dismiss = () => setPopup(null);
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", dismiss);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("resize", dismiss);
    };
  }, [popup]);

  const react = (emoji: string) => { setReaction(previous => previous === emoji ? "" : emoji); setPopup(null); };
  const action = (callback: () => void) => { callback(); setPopup(null); };
  const open = (button: HTMLButtonElement, picker = false) => {
    const box = button.getBoundingClientRect();
    setPopup({ x: box.right - 248, y: box.bottom + 4, picker });
  };
  const copy = async (value: string) => {
    try { await navigator.clipboard.writeText(value); setNotice("Đã sao chép."); }
    catch { setNotice("Chưa sao chép được. Vui lòng kiểm tra quyền clipboard."); }
  };
  const itemClass = "flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm hover:bg-[#55416d] focus-visible:bg-[#55416d] disabled:cursor-default disabled:opacity-50";

  return <div data-message-frame className={`group relative flex flex-col rounded-xl px-2 py-3 hover:bg-white/5 focus-within:bg-white/5 ${isMe ? "items-end" : "items-start"}`}
    onContextMenu={event => { event.preventDefault(); setPopup({ x: event.clientX, y: event.clientY, picker: false }); }}>
    {children}
    <div role="toolbar" aria-label="Tương tác tin nhắn" className="absolute right-2 top-0 z-10 flex items-center gap-0.5 rounded-lg border border-white/15 bg-[#2b2338] p-1 shadow-lg opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:pointer-events-auto max-sm:opacity-100 max-sm:pointer-events-auto">
      {reactions.slice(0, 3).map(emoji => <button key={emoji} className="grid h-8 w-8 place-items-center rounded-md hover:bg-white/10" aria-label={`Biểu cảm ${emoji}`} onClick={() => react(emoji)}>{emoji}</button>)}
      <button aria-label="Thêm biểu cảm" className="workspace-icon-button rounded-md hover:bg-white/10" onClick={event => open(event.currentTarget, true)}><SmilePlus size={17}/></button>
      <button aria-label="Trả lời" className="workspace-icon-button rounded-md hover:bg-white/10" onClick={() => onReply(text)}><Reply size={17}/></button>
      <button ref={moreRef} aria-label="Thao tác tin nhắn" aria-haspopup="menu" aria-expanded={Boolean(popup)} className="workspace-icon-button rounded-md hover:bg-white/10" onClick={event => open(event.currentTarget)}><MoreHorizontal size={17}/></button>
    </div>
    {(reaction || pinned || saved || unread) && <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-[#c8b2f1]">
      {reaction && <button className="rounded-full bg-[#c8b2f1]/20 px-2 py-1" aria-label="Gỡ biểu cảm" onClick={() => setReaction("")}>{reaction} 1</button>}
      {pinned && <span>Đã ghim</span>}{saved && <span>Đã lưu</span>}{unread && <span>Chưa đọc</span>}
    </div>}
    {notice && <p role="status" className="mt-1 text-xs text-[#c8b2f1]">{notice}</p>}
    {popup && createPortal(<div ref={popupRef} role="menu" aria-label={popup.picker ? "Chọn biểu cảm" : "Thao tác tin nhắn"} onContextMenu={event => { event.preventDefault(); event.stopPropagation(); }}
      onKeyDown={event => {
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
        buttons[next]?.focus();
      }}
      className="fixed z-[100] w-[248px] max-w-[calc(100vw-16px)] max-h-[calc(100dvh-16px)] overflow-y-auto rounded-xl border border-white/15 bg-[#251e30] p-2 text-[#eee7f5] shadow-[0_12px_36px_#0008]"
      style={{ left: popup.x, top: popup.y }}>
      <div className="flex justify-between gap-1 p-1">{(popup.picker ? [...reactions, "🎉"] : reactions).map(emoji => <button key={emoji} role="menuitem" className="grid h-10 flex-1 place-items-center rounded-lg bg-white/5 text-lg hover:bg-white/15" aria-label={`Biểu cảm ${emoji}`} onClick={() => react(emoji)}>{emoji}</button>)}</div>
      {!popup.picker && <>
        <button role="menuitem" className={itemClass} onClick={() => setPopup({ ...popup, picker: true })}><SmilePlus size={17}/>Thêm biểu cảm</button>
        <div className="my-1 border-t border-white/10"/>
        <button role="menuitem" className={itemClass} onClick={() => action(() => onReply(text))}><Reply size={17}/>Trả lời</button>
        <button role="menuitem" className={itemClass} onClick={() => action(() => onReply(`Chuyển tiếp: ${text}`))}><Forward size={17}/>Chuyển tiếp</button>
        <div className="my-1 border-t border-white/10"/>
        <button role="menuitem" className={itemClass} onClick={() => action(() => { onPin?.(text, !pinned); setPinned(!pinned); })}><Pin size={17}/>{pinned ? "Bỏ ghim" : "Ghim tin nhắn"}</button>
        <button role="menuitem" className={itemClass} onClick={() => action(() => setSaved(!saved))}><Bookmark size={17}/>{saved ? "Bỏ lưu tin nhắn" : "Lưu tin nhắn"}</button>
        <button role="menuitem" disabled title="Chưa kết nối nhắc nhở" className={itemClass}><Bell size={17}/>Tạo thông báo nhắc nhở</button>
        <button role="menuitem" className={itemClass} onClick={() => action(() => setUnread(!unread))}><Headphones size={17}/>Đánh dấu {unread ? "đã đọc" : "chưa đọc"}</button>
        <button role="menuitem" disabled title="Chưa có liên kết tin nhắn trong bản xem thử" className={itemClass}><Link2 size={17}/>Sao chép liên kết tin nhắn</button>
        <button role="menuitem" className={itemClass} onClick={() => action(() => setNotice(reaction ? `Biểu cảm của bạn: ${reaction} · 1 lượt` : "Chưa có biểu cảm."))}><Info size={17}/>Xem thông tin tương tác</button>
        <button role="menuitem" className={itemClass} onClick={() => action(() => { void copy(text); })}><Copy size={17}/>Sao chép nội dung</button>
        <div className="my-1 border-t border-white/10"/>
        {isMe && <button role="menuitem" className={`${itemClass} text-[#ff9ba9]`} onClick={() => action(onDelete)}><Trash2 size={17}/>Xóa tin nhắn</button>}
        <button role="menuitem" disabled title="Chưa kết nối báo cáo" className={`${itemClass} text-[#ff9ba9]`}><Flag size={17}/>Báo cáo tin nhắn</button>
        <div className="my-1 border-t border-white/10"/>
        <button role="menuitem" className={itemClass} onClick={() => action(() => { void copy(messageId); })}><Fingerprint size={17}/>Sao chép ID tin nhắn</button>
      </>}
    </div>, document.body)}
  </div>;
}
