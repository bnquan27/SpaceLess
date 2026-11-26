import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiClient, ApiError, errorText, orderedMessages, type ApiEvent, type ApiMessage, type Bootstrap, type History } from './api';
import { useApiWorkspace } from './use-api-workspace';
import { useBackendVoice } from './use-backend-voice';

export interface WorkspaceBackend {
  api: ApiClient;
  data: Bootstrap;
  epoch: number;
  status: string;
  refresh: () => Promise<void>;
  report: (error: unknown) => void;
  subscribe: (listener: (event: ApiEvent) => void) => () => void;
  nameOf: (id: string) => string;
  voice: ReturnType<typeof useBackendVoice>;
}
const Context = createContext<WorkspaceBackend | null>(null);
export const useBackend = () => useContext(Context);
export const initials = (name: string) => name.trim().split(/\s+/).slice(-2).map(part => Array.from(part)[0]).join('').toUpperCase();

export function BackendProvider({ api, tokenVersion, onExpired, children }: { api: ApiClient; tokenVersion: string; onExpired: () => void; children: ReactNode }) {
  const listeners = useRef(new Set<(event: ApiEvent) => void>());
  const subscribe = useCallback((listener: (event: ApiEvent) => void) => { listeners.current.add(listener); return () => { listeners.current.delete(listener); }; }, []);
  const { data, error, status, historyEpoch, refresh } = useApiWorkspace(api, tokenVersion, onExpired, event => listeners.current.forEach(listener => listener(event)));
  const [notice, setNotice] = useState('');
  const report = useCallback((error: unknown) => { if (error instanceof ApiError && error.status === 401) onExpired(); else setNotice(errorText(error)); }, [onExpired]);
  const voice = useBackendVoice(api, data, subscribe, report);
  const nameOf = (id: string) => id === data?.profile.user_id ? data.profile.full_name : data?.friends.find(person => person.user_id === id)?.full_name || data?.channels.flatMap(channel => channel.members).find(person => person.user_id === id)?.full_name || `Thành viên ${id}`;
  if (!data) return <main className="min-h-screen bg-[#15121d] text-[#f4effa] flex flex-col items-center justify-center gap-4 p-6"><h1 className="text-xl font-bold">Đang mở không gian của bạn</h1><p role={error ? 'alert' : 'status'}>{error || status}</p><button className="rounded-xl bg-[#c8b2f1] px-5 py-3 text-[#271737]" onClick={() => void refresh()}>Thử lại</button></main>;
  return <Context.Provider value={{ api, data, epoch: historyEpoch, refresh, report, subscribe, status, nameOf, voice }}>
    {(notice || error) && <div role="alert" className="fixed bottom-12 left-1/2 z-[150] flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-xl border border-[#c8b2f1]/40 bg-[#261e3a] p-4 text-sm text-[#f4effa]"><span>{notice || error}</span><button onClick={() => { setNotice(''); void refresh(); }}>Thử lại</button><button aria-label="Ẩn thông báo" onClick={() => setNotice('')}>×</button></div>}
    {children}
  </Context.Provider>;
}

export function useRoomMessages(roomId?: string) {
  const backend = useBackend();
  const api = backend?.api;
  const subscribe = backend?.subscribe;
  const epoch = backend?.epoch;
  const [items, setItems] = useState<ApiMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [sending, setSending] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const receipt = useRef<{ client_id: string; content: string; upload_ids: string[]; reply_to?: string } | null>(null);
  const uploaded = useRef(new Map<File,string>());
  const activeRoom = useRef(roomId);
  activeRoom.current = roomId;
  const current = useRef(backend);
  current.current = backend;
  useEffect(() => {
    setItems([]); setError('');
    if (!api || !roomId || !subscribe) return;
    const abort = new AbortController();
    let waiting = true;
    let queued: ApiEvent[] = [];
    let revision = '0';
    const apply = (event: ApiEvent) => {
      if (event.data.room_id !== roomId || BigInt(event.revision) <= BigInt(revision)) return;
      revision = event.revision;
      if (event.type === 'MESSAGE_CREATE' || event.type === 'MESSAGE_UPDATE') {
        const item = event.data as ApiMessage;
        if (item.message_id && item.room_sequence) setItems(previous => orderedMessages([...previous.filter(value => value.message_id !== item.message_id), item]));
      } else if (event.type === 'MESSAGE_DELETE') setItems(previous => previous.filter(item => item.message_id !== event.data.message_id));
      else if (event.type === 'MESSAGE_REACTION_UPDATE') setItems(previous => previous.map(item => item.message_id === event.data.message_id ? { ...item, reaction_counts: event.data.reaction_counts || {}, my_reaction: event.data.my_reaction ?? null } : item));
    };
    const unsubscribe = subscribe(event => { if (waiting) queued.push(event); else apply(event); });
    setLoading(true);
    api.request<History>(`/rooms/${roomId}/messages?limit=50`, 'GET', undefined, abort.signal).then(value => {
      if (abort.signal.aborted) return;
      revision = value.revision; setItems(orderedMessages(value.items)); setHasMore(value.has_more);
      waiting = false; queued.forEach(apply); queued = [];
    }).catch(error => { if (!abort.signal.aborted) { setError(errorText(error)); current.current?.report(error); } }).finally(() => { if (!abort.signal.aborted) setLoading(false); });
    return () => { abort.abort(); unsubscribe(); };
  }, [api, roomId, subscribe, epoch]);
  useEffect(() => { receipt.current=null; uploaded.current.clear(); setSending(false); },[roomId]);
  useEffect(() => {
    const mark = () => {
      const last = items[items.length - 1];
      if (!document.hidden && api && roomId && last) void api.request(`/rooms/${roomId}/read-state`, 'PUT', { last_read_sequence: last.room_sequence }).catch(error => current.current?.report(error));
    };
    mark(); document.addEventListener('visibilitychange', mark);
    return () => document.removeEventListener('visibilitychange', mark);
  }, [api, roomId, items.length]);
  async function send(content: string, files: File[] = [], replyTo?: string) {
    if (!api || !roomId || sending || loading || error) return false;
    setSending(true);
    try {
      const uploads = await Promise.all(files.map(async file => {const cached=uploaded.current.get(file);if(cached)return cached;const id=await api.upload(roomId,file);uploaded.current.set(file,id);return id;}));
      if (activeRoom.current !== roomId) return false;
      if (!receipt.current || receipt.current.content !== content || receipt.current.reply_to !== replyTo || receipt.current.upload_ids.join(',') !== uploads.join(',')) receipt.current = { client_id: crypto.randomUUID(), content, upload_ids: uploads, ...(replyTo ? {reply_to: replyTo} : {}) };
      const value = await api.request<ApiMessage>(`/rooms/${roomId}/messages`, 'POST', receipt.current);
      if (activeRoom.current === roomId) { setItems(previous => orderedMessages([...previous.filter(item => item.message_id !== value.message_id), value])); receipt.current = null; uploaded.current.clear(); }
      return activeRoom.current === roomId;
    } catch (error) { current.current?.report(error); return false; }
    finally { if (activeRoom.current === roomId) setSending(false); }
  }
  async function remove(id: string) {
    if (!api || !roomId) return;
    try { await api.request(`/rooms/${roomId}/messages/${id}`, 'DELETE'); if (activeRoom.current === roomId) setItems(previous => previous.filter(item => item.message_id !== id)); }
    catch (error) { current.current?.report(error); }
  }
  async function react(id: string, emoji: string) {
    if (!api || !roomId) return;
    const item = items.find(item => item.message_id === id);
    if (!item) return;
    try {
      await api.request(`/rooms/${roomId}/messages/${id}/reactions/me`, item.my_reaction === emoji ? 'DELETE' : 'PUT', item.my_reaction === emoji ? undefined : { reaction_type: emoji });
      const value = await api.request<History>(`/rooms/${roomId}/messages?after_sequence=${BigInt(item.room_sequence) - 1n}&through_sequence=${item.room_sequence}&limit=1`);
      if (activeRoom.current === roomId && value.items[0]) setItems(previous => previous.map(item => item.message_id === id ? value.items[0] : item));
    } catch (error) { current.current?.report(error); }
  }
  async function older() {
    if (!api || !roomId || !items[0] || loadingOlder) return;
    setLoadingOlder(true);
    try { const value = await api.request<History>(`/rooms/${roomId}/messages?before_sequence=${items[0].room_sequence}&limit=50`); if (activeRoom.current === roomId) { setItems(previous => orderedMessages([...value.items, ...previous])); setHasMore(value.has_more); } }
    catch (error) { current.current?.report(error); }
    finally { setLoadingOlder(false); }
  }
  return { items, loading, error, sending, send, remove, react, older, hasMore, loadingOlder };
}

export function messageView(message: ApiMessage, backend: WorkspaceBackend) {
  return { reply: message.reply_context, forwarded: message.forward_context, id: message.message_id, sender: backend.nameOf(message.sender_id), isMe: message.sender_id === backend.data.profile.user_id, text: message.content, time: new Date(message.time_created).toLocaleTimeString('vi-VN', { hour: '2-digit', minute: '2-digit' }), file: message.attachments[0] ? { name: message.attachments[0].file_name, size: `${(message.attachments[0].file_size_bytes / 1024).toLocaleString('vi-VN')} KB`, id: message.attachments[0].attachment_id } : undefined };
}
