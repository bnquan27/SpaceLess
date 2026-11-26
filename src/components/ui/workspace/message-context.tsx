import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Forward, Hash, Search, Send, X } from 'lucide-react';
import { useBackend, initials } from '@/lib/workspace-backend';
import { errorText, type MessageContext } from '@/lib/api';

export type ComposeMessage = { id: string | number; sender: string; text: string; reply?: MessageContext | null; forwarded?: MessageContext | null; file?: {name: string} };
export type ForwardTarget = { id: string; name: string; detail: string; friend?: boolean };

export function ReplyBanner({ message, onCancel }: { message: ComposeMessage; onCancel: () => void }) {
  return <div role="status" className="flex items-center justify-between gap-3 border-b border-white/10 pb-2 text-sm text-[#d4c8df]">
    <span className="truncate">Đang phản hồi <strong className="text-[#f4effa]">{message.sender}</strong></span>
    <button type="button" onClick={onCancel} aria-label="Hủy phản hồi" className="workspace-icon-button shrink-0 rounded-full hover:bg-white/10"><X size={16}/></button>
  </div>;
}

export function MessageQuote({ context, forwarded = false }: { context?: MessageContext | null; forwarded?: boolean }) {
  if (!context) return null;
  return <div className="mb-2 max-w-full rounded-lg bg-black/10 p-2 text-xs">
    <div className="mb-1 flex items-center gap-1 font-semibold">{forwarded && <Forward size={13}/>} {forwarded ? 'Đã chuyển tiếp' : `Phản hồi ${context.sender_name}`}</div>
    {forwarded && <strong>{context.sender_name}</strong>}
    <p className="line-clamp-3 whitespace-pre-wrap wrap-anywhere">{context.text || (context.has_attachments ? 'Tệp đính kèm' : 'Tin nhắn')}</p>
  </div>;
}

export function ForwardDialog({ message, sourceRoom, previewTargets, onPreviewSend, onClose }: {
  message: ComposeMessage; sourceRoom?: string; previewTargets: ForwardTarget[];
  onPreviewSend: (target: string, message: ComposeMessage, note: string) => void; onClose: () => void;
}) {
  const backend = useBackend();
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [note, setNote] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState('');
  const [sent, setSent] = useState<string[]>([]);
  const search = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const receipts = useRef(new Map<string,string>());
  const previousFocus = useRef(document.activeElement as HTMLElement | null);
  const busy = useRef(false);
  const close = useRef(onClose); close.current = onClose;
  useEffect(() => {
    search.current?.focus();
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape' && !busy.current) close.current(); };
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('keydown', escape); previousFocus.current?.focus(); };
  }, []);
  const targets: ForwardTarget[] = backend ? [
    ...backend.data.rooms.filter(room => room.type === 'TEXT' && !room.is_read_only).map(room => ({id:room.room_id,name:room.room_name,detail:backend.data.channels.find(channel => channel.channel_id === room.channel_id)?.channel_name || 'Phòng chat'})),
    ...backend.data.friends.map(friend => ({id:`friend:${friend.user_id}`,name:friend.full_name,detail:`@${friend.username}`,friend:true})),
  ] : previewTargets;
  const shown = targets.filter(target => `${target.name} ${target.detail}`.toLocaleLowerCase('vi').includes(query.toLocaleLowerCase('vi')));
  async function submit() {
    if (busy.current || !selected.length) return;
    busy.current = true; setSending(true); setError('');
    const completed = [...sent];
    const failed: string[] = [];
    for (const id of selected.filter(id => !completed.includes(id))) {
      try {
        if (backend) {
          let roomId = id;
          if (id.startsWith('friend:')) {
            const dm = await backend.api.request<{room_id:string}>('/dms','POST',{target_user_id:id.slice(7)});
            roomId = dm.room_id;
          }
          if (!receipts.current.has(id)) receipts.current.set(id, crypto.randomUUID());
          await backend.api.request(`/rooms/${roomId}/messages`,'POST',{client_id:receipts.current.get(id),content:note,forward_from:{room_id:sourceRoom,message_id:String(message.id)}});
        } else onPreviewSend(id, message, note);
        completed.push(id);
      } catch (reason) { failed.push(`${targets.find(target => target.id === id)?.name}: ${errorText(reason)}`); }
    }
    setSent(completed); setSending(false); busy.current = false;
    if (!failed.length) { if (backend) void backend.refresh(); onClose(); }
    else setError(`${completed.length ? `Đã gửi tới ${completed.length} nơi. ` : ''}${failed.join(' · ')}`);
  }
  const attachments = !!message.file;
  return createPortal(<div className="fixed inset-0 z-[120] flex items-center justify-center bg-black/65 p-3" onMouseDown={event => { if (event.target === event.currentTarget && !sending) onClose(); }}>
    <div ref={dialog} role="dialog" aria-modal="true" aria-labelledby="forward-title" className="flex max-h-[min(760px,calc(100dvh-24px))] w-full max-w-[460px] flex-col overflow-hidden rounded-2xl bg-[#251e30] text-[#f4effa] shadow-[0_16px_60px_#0009]" onKeyDown={event => {
      if (event.key !== 'Tab') return;
      const controls = Array.from(dialog.current!.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),textarea:not(:disabled)'));
      const first = controls[0], last = controls[controls.length-1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }}>
      <div className="px-5 pt-5 pb-3">
        <div className="flex items-center justify-between gap-3"><h2 id="forward-title" className="text-xl font-bold">Chuyển tiếp tới</h2><button disabled={sending} onClick={onClose} aria-label="Đóng chuyển tiếp" className="workspace-icon-button rounded-lg hover:bg-white/10"><X size={20}/></button></div>
        <p className="mt-1 text-sm text-[#c7bbd2]">Chọn nơi bạn muốn chia sẻ tin nhắn này.</p>
        <label className="mt-5 flex items-center gap-2 rounded-lg border border-white/20 bg-[#15121d] px-3"><Search size={17} className="text-[#c7bbd2]"/><input ref={search} aria-label="Tìm nơi chuyển tiếp" value={query} onChange={event => setQuery(event.target.value)} placeholder="Tìm kiếm" className="min-w-0 flex-1 bg-transparent py-3 text-sm outline-none placeholder:text-[#c7bbd2]"/></label>
      </div>
      <div className="min-h-24 flex-1 overflow-y-auto px-3 pb-3">
        {!shown.length && <p className="p-3 text-sm text-[#c7bbd2]">Không có nơi nhận phù hợp.</p>}
        {shown.map(target => <label key={target.id} className="flex cursor-pointer items-center gap-3 rounded-lg px-2 py-3 hover:bg-white/5">
          <span aria-hidden="true" className="relative grid h-9 w-9 shrink-0 place-items-center rounded-full bg-[#c8b2f1]/20 text-xs font-bold text-[#ddd0f4]">{initials(target.name)}{!target.friend && <Hash size={13} className="absolute -bottom-1 -right-1 rounded-full bg-[#15121d] p-0.5"/>}</span>
          <span className="min-w-0 flex-1"><strong className="block truncate text-sm">{target.name}</strong><span className="block truncate text-xs text-[#c7bbd2]">{target.detail}{sent.includes(target.id) ? ' · Đã gửi' : ''}</span></span>
          <input type="checkbox" aria-label={`Chuyển tiếp tới ${target.name}`} checked={selected.includes(target.id)} disabled={sending || sent.includes(target.id)} onChange={event => setSelected(previous => event.target.checked ? [...previous,target.id] : previous.filter(id => id !== target.id))} className="h-5 w-5 shrink-0 accent-[#c8b2f1]"/>
        </label>)}
      </div>
      <div className="space-y-3 border-t border-white/10 bg-[#201a29] p-4">
        <div className="max-h-24 overflow-y-auto rounded-lg bg-white/5 p-3 text-sm"><p className="text-xs font-semibold text-[#d4c8df]">{message.forwarded?.sender_name || message.sender}</p><p className="line-clamp-2 wrap-anywhere">{message.text || message.forwarded?.text || 'Tệp đính kèm'}</p></div>
        {attachments && <p className="text-sm text-[#ffb7c2]">Chuyển tiếp tệp chưa được hỗ trợ.</p>}
        {error && <p role="alert" className="text-sm text-[#ffb7c2]">{error}</p>}
        <div className="flex items-end gap-2"><textarea aria-label="Lời nhắn chuyển tiếp" placeholder="Thêm lời nhắn (không bắt buộc)" value={note} disabled={sending || receipts.current.size > 0} onChange={event => setNote(event.target.value)} maxLength={4000} rows={2} className="min-w-0 flex-1 resize-none rounded-lg border border-white/15 bg-[#15121d] p-3 text-sm placeholder:text-[#c7bbd2]"/><button disabled={sending || attachments || !selected.some(id => !sent.includes(id))} onClick={() => void submit()} className="flex items-center gap-2 rounded-lg bg-[#c8b2f1] px-4 py-3 font-bold text-[#271737] disabled:opacity-50"><Send size={16}/>{sending ? 'Đang gửi…' : `Gửi${selected.length ? ` (${selected.length})` : ''}`}</button></div>
      </div>
    </div>
  </div>,document.body);
}
