export interface Person {
  user_id: string;
  username: string;
  full_name: string;
  avatar_url: string | null;
}
export interface Profile extends Person {
  email: string;
  phone: string;
  date_of_birth: string;
  address: string | null;
}
export interface Channel {
  invite_code?: string | null;
  channel_id: string;
  channel_name: string;
  description: string | null;
  role: string;
  icon_url: string | null;
  members: (Person & { role: string })[];
}
export interface ApiRoom {
  room_id: string;
  channel_id: string | null;
  room_name: string;
  type: 'TEXT' | 'VOICE' | 'DM';
  is_read_only: boolean;
  member_ids: string[] | null;
  last_message_sequence: string;
  last_read_sequence: string;
  unread_count: number;
}
export interface VoiceState {
  room_id: string;
  user_id: string;
  voice_session_id: string;
  state: string;
  screen_video_sid: string | null;
}
export interface Bootstrap {
  profile: Profile;
  channels: Channel[];
  rooms: ApiRoom[];
  dms: ApiRoom[];
  friends: Person[];
  friend_requests: { request_id: string; requester_id: string; user: Person }[];
  invitations: { invitation_id: string; invitee_id: string; channel_name: string }[];
  voice_states: VoiceState[];
  revisions: Record<string, string>;
}
export interface MessageContext { message_id: string; room_id: string; sender_name: string; text: string; has_attachments?: boolean }
export interface ApiMessage {
  reply_context?: MessageContext | null;
  forward_context?: MessageContext | null;
  message_id: string;
  room_id: string;
  sender_id: string;
  client_id: string;
  room_sequence: string;
  content: string;
  time_created: string;
  time_edited: string | null;
  attachments: { attachment_id: string; file_name: string; file_size_bytes: number }[];
  reaction_counts: Record<string, number>;
  my_reaction: string | null;
}
export interface History {
  items: ApiMessage[];
  next_cursor: string | null;
  has_more: boolean;
  revision: string;
  last_message_sequence: string;
}
export interface ApiEvent {
  type: string;
  stream: string;
  revision: string;
  data: Partial<ApiMessage> & { voice_session_id?: string; state?: string; user_id?: string; last_read_sequence?: string };
}
export interface Session { access_token: string; expires_at: string }
export type Registration = { username: string; password: string; full_name: string; date_of_birth: string; email: string; phone: string };

const configuredOrigin = (import.meta as ImportMeta & { env: Record<string, string> }).env.VITE_API_ORIGIN;
// The desktop bundle has no Vite proxy. Point it directly at the configured API.
export const apiOrigin = configuredOrigin || ('__TAURI_INTERNALS__' in window ? 'http://127.0.0.1:8080' : '');
export class ApiError extends Error {
  constructor(message: string, public status: number, public code: string) { super(message); }
}
export const errorText = (error: unknown) => error instanceof Error ? error.message : 'Thao tác chưa hoàn tất. Vui lòng thử lại.';

export class ApiClient {
  constructor(private token = '') {}
  setToken(token: string) { this.token = token; }
  clear() { this.token = ''; }

  async upload(roomId: string, file: File): Promise<string> {
    const grant = await this.request<{ upload_id: string; put_url: string; required_headers: Record<string, string> }>(`/rooms/${roomId}/uploads/presign`, 'POST', { file_name: file.name, mime_type: file.type || 'application/octet-stream', file_size_bytes: file.size });
    const headers = Object.fromEntries(Object.entries(grant.required_headers).filter(([key]) => !['host', 'content-length'].includes(key.toLowerCase())));
    const response = await fetch(grant.put_url, { method: 'PUT', body: file, headers, credentials: 'omit', signal: AbortSignal.timeout(120000) });
    if (!response.ok) throw new ApiError('Chưa tải được tệp lên kho lưu trữ. Vui lòng thử lại.', response.status, 'UPLOAD_FAILED');
    return grant.upload_id;
  }

  async download(path: string): Promise<Blob> {
    const url = new URL(path, apiOrigin || window.location.origin);
    const fileOrigin = (import.meta as ImportMeta & { env: Record<string, string> }).env.VITE_FILE_ORIGIN || 'http://127.0.0.1:8081';
    const allowed = [new URL(apiOrigin || window.location.origin).origin, new URL(fileOrigin).origin];
    if (!allowed.includes(url.origin)) throw new ApiError('Địa chỉ tải tệp chưa được cấu hình cho ứng dụng.', 0, 'FILE_ORIGIN');
    const response = await fetch(url, { headers: { Authorization: `Bearer ${this.token}` }, credentials: 'omit', cache: 'no-store', signal: AbortSignal.timeout(30000) });
    if (!response.ok) throw new ApiError('Không tải được tệp. Quyền truy cập có thể đã thay đổi.', response.status, 'DOWNLOAD_FAILED');
    return response.blob();
  }

  async request<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<T> {
    let response: Response;
    try {
      response = await fetch(`${apiOrigin}/api/v1${path}`, {
        method,
        headers: { ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}), ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}) },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(15000)]) : AbortSignal.timeout(15000),
        credentials: 'omit',
        cache: 'no-store',
      });
    } catch (error) {
      if (signal?.aborted) throw error;
      throw new ApiError('Không kết nối được máy chủ. Hãy kiểm tra API đã chạy rồi thử lại.', 0, 'NETWORK_ERROR');
    }
    if (response.status === 204) return undefined as T;
    const data = await response.json().catch(() => null);
    if (!response.ok) {
      if (response.status >= 500 && !data?.error) throw new ApiError('Chưa kết nối được API. Hãy khởi động máy chủ rồi thử lại.', response.status, 'UPSTREAM_UNAVAILABLE');
      throw new ApiError(data?.error?.message || (response.status === 401 ? 'Phiên đăng nhập không còn hợp lệ. Vui lòng đăng nhập lại.' : 'Máy chủ chưa xử lý được yêu cầu. Vui lòng thử lại.'), response.status, data?.error?.code || 'API_ERROR');
    }
    if (data === null) throw new ApiError('Phản hồi máy chủ không đúng định dạng JSON.', response.status, 'INVALID_RESPONSE');
    return data as T;
  }

  socket() {
    const url = new URL(`${apiOrigin}/api/v1/ws`, window.location.origin);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    // Browser WebSocket cannot set Authorization. Credentials stay out of URLs/storage.
    return new WebSocket(url, ['spaceless', `bearer.${this.token}`]);
  }
}

export function orderedMessages(items: ApiMessage[]) {
  return [...new Map(items.map(item => [item.message_id, item])).values()]
    .sort((a, b) => BigInt(a.room_sequence) < BigInt(b.room_sequence) ? -1 : BigInt(a.room_sequence) > BigInt(b.room_sequence) ? 1 : 0);
}
