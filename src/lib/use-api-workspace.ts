import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiClient, ApiError, errorText, type ApiEvent, type Bootstrap } from './api';

export function useApiWorkspace(api: ApiClient, tokenVersion: string, onExpired: () => void, onEvent: (event: ApiEvent) => void) {
  const [data, setData] = useState<Bootstrap | null>(null);
  const [error, setError] = useState('');
  const [status, setStatus] = useState('Đang kết nối…');
  const [historyEpoch, setHistoryEpoch] = useState(0);
  const callbacks = useRef({ onExpired, onEvent });
  callbacks.current = { onExpired, onEvent };
  const refreshRef = useRef<() => Promise<void>>(async () => {});

  useEffect(() => {
    let stopped = false;
    let socket: WebSocket | undefined;
    let timer: ReturnType<typeof setTimeout>;
    let attempts = 0;
    let syncing: Promise<void> | null = null;
    let dirty = false;
    let queued: ApiEvent[] = [];
    let revisions: Record<string, string> = {};
    const abort = new AbortController();

    const expire = (error: unknown) => {
      if (error instanceof ApiError && error.status === 401) callbacks.current.onExpired();
      else if (!stopped) setError(errorText(error));
    };
    const deliver = (event: ApiEvent) => {
      if (!/^\d+$/.test(event.revision)) { void refresh(); return; }
      const previous = BigInt(revisions[event.stream] || '0');
      const next = BigInt(event.revision);
      if (next <= previous) return;
      if (next !== previous + 1n) { void refresh(); return; }
      revisions[event.stream] = event.revision;
      if (event.type === 'MESSAGE_CREATE' || event.type === 'MESSAGE_READ_UPDATE') {
        setData(snapshot => {
          if (!snapshot) return snapshot;
          const update = (room: Bootstrap['rooms'][number]) => {
            if (room.room_id !== event.data.room_id) return room;
            if (event.type === 'MESSAGE_CREATE' && event.data.room_sequence) return { ...room, last_message_sequence: event.data.room_sequence, unread_count: room.unread_count + (event.data.sender_id !== snapshot.profile.user_id ? 1 : 0) };
            if (event.data.user_id === snapshot.profile.user_id && event.data.last_read_sequence) return { ...room, last_read_sequence: event.data.last_read_sequence, unread_count: BigInt(event.data.last_read_sequence) >= BigInt(room.last_message_sequence) ? 0 : room.unread_count };
            return room;
          };
          return { ...snapshot, rooms: snapshot.rooms.map(update), dms: snapshot.dms.map(update) };
        });
      }
      callbacks.current.onEvent(event);
      if (!event.type.startsWith('MESSAGE_') && event.type !== 'READ_STATE_UPDATE') void refresh();
    };
    const refresh = (): Promise<void> => {
      if (stopped) return Promise.resolve();
      if (syncing) { dirty = true; return syncing; }
      syncing = (async () => {
        try {
          const value = await api.request<Bootstrap>('/bootstrap', 'GET', undefined, abort.signal);
          if (stopped) return;
          revisions = value.revisions;
          setData(value);
          setHistoryEpoch(epoch => epoch + 1);
          setError('');
        } catch (error) { if (!stopped) expire(error); }
        finally {
          syncing = null;
          if (!stopped) {
            const events = queued;
            queued = [];
            events.forEach(deliver);
            if (dirty) { dirty = false; void refresh(); }
          }
        }
      })();
      return syncing;
    };
    refreshRef.current = refresh;
    const connect = () => {
      if (stopped) return;
      socket = api.socket();
      socket.onopen = () => {
        attempts = 0;
        setStatus('Đã kết nối thời gian thực');
        // Subscribe before the snapshot so events during bootstrap can be replayed.
        void refresh();
      };
      socket.onmessage = message => {
        try {
          const event: ApiEvent = JSON.parse(message.data);
          if (syncing) queued.push(event);
          else deliver(event);
        } catch { void refresh(); }
      };
      socket.onclose = () => {
        if (stopped) return;
        setStatus('Mất kết nối · đang thử lại');
        void refresh();
        timer = setTimeout(connect, Math.min(30000, 1000 * 2 ** Math.min(attempts++, 5)));
      };
      socket.onerror = () => setStatus('Chưa kết nối được thời gian thực');
    };
    connect();
    // REST remains useful when a deployment has not enabled WebSocket yet.
    void refresh();
    return () => { stopped = true; abort.abort(); clearTimeout(timer); socket?.close(); };
  }, [api, tokenVersion]);

  const refresh = useCallback(() => refreshRef.current(), []);
  return { data, error, status, historyEpoch, refresh };
}
