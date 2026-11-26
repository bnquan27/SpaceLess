import React, { useEffect, useRef, useState } from "react";
import {
  Menu,
  Download,
  Eye,
  Send,
  Code,
  Smile,
  Link2,
  UserPlus,
  Clock,
  X,
  Check,
} from "lucide-react";
import { ViewProps } from "./types";
import { ForwardDialog, ReplyBanner, MessageQuote, type ComposeMessage } from './message-context';
import { MessageActions } from "./message-actions";
import { PreviewAvatar } from "./preview-images";
import { useBackend, useRoomMessages, messageView, initials } from "@/lib/workspace-backend";
import { BackendAttachments, BackendFilePicker } from "./backend-files";

interface FriendContact {
  id: string;
  init: string;
  name: string;
  username: string;
  status: string;
  online: boolean;
  dot: string;
}

export const DirectMessageView: React.FC<ViewProps> = ({
  currentUser,
  initialFriendName,
}) => {
  const backend = useBackend();
  const [files,setFiles] = useState<File[]>([]);
  const [dmFilter, setDmFilter] = useState<
    "all" | "online" | "pending" | "add"
  >("all");
  const [selectedFriendId, setSelectedFriendId] = useState<string>(
    ({ "Hoàng Đạt": "f1", "Thu Hà": "f2", "Quốc Bảo": "f3", "Minh Anh": "f4" } as Record<string, string>)[initialFriendName || ""] || "f1",
  );
  const [dmInput, setDmInput] = useState("");
  const [reply, setReply] = useState<ComposeMessage | null>(null);
  const [forward, setForward] = useState<ComposeMessage | null>(null);
  const composer = useRef<HTMLInputElement>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [detailTab, setDetailTab] = useState("Tệp");
  const [messageSearch, setMessageSearch] = useState("");
  const [pinnedMessages, setPinnedMessages] = useState<Record<string, string[]>>({});
  const [addUsernameInput, setAddUsernameInput] = useState("");

  // Danh sách bạn bè hiện tại
  const [previewFriends] = useState<FriendContact[]>([
    {
      id: "f1",
      init: "HĐ",
      name: "Hoàng Đạt",
      username: "@hoangdat",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f2",
      init: "TH",
      name: "Thu Hà",
      username: "@thuha",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f3",
      init: "QB",
      name: "Quốc Bảo",
      username: "@quocbao",
      status: "Đang trực tuyến",
      online: true,
      dot: "#a2d9d3",
    },
    {
      id: "f4",
      init: "MA",
      name: "Minh Anh",
      username: "@minhanh",
      status: "Ngoại tuyến (2 giờ trước)",
      online: false,
      dot: "#6b7280",
    },
    {
      id: "f5",
      init: "ĐK",
      name: "Đăng Khoa",
      username: "@dangkhoa",
      status: "Ngoại tuyến",
      online: false,
      dot: "#6b7280",
    },
  ]);

  // Danh sách người mình đã bấm Kết bạn nhưng đang chờ họ phản hồi
  const [previewPendingRequests, setPendingRequests] = useState<
    { id: string; init: string; name: string; username: string; time: string }[]
  >([
    {
      id: "p1",
      init: "TN",
      name: "Tuấn Nghĩa",
      username: "@tuannghia.ui",
      time: "Đã gửi 10 phút trước",
    },
    {
      id: "p2",
      init: "KL",
      name: "Khánh Linh",
      username: "@khanhlinh.design",
      time: "Đã gửi hôm qua",
    },
  ]);

  // Danh sách gợi ý kết bạn trong tab "Thêm bạn"
  const [previewSuggestions] = useState([
    {
      id: "s1",
      init: "VH",
      name: "Việt Hoàng",
      username: "@viethoang.fe",
      mutual: "4 bạn chung",
    },
    {
      id: "s2",
      init: "PL",
      name: "Phương Thảo",
      username: "@phuongthao.ux",
      mutual: "2 bạn chung",
    },
    {
      id: "s3",
      init: "ĐM",
      name: "Đức Mạnh",
      username: "@ducmanh.dev",
      mutual: "Cùng nhóm Đồ Án",
    },
  ]);

  const [messagesByFriend, setMessagesByFriend] = useState<Record<string, {id: number; sender: string; time: string; isMe: boolean; text: string; reply?: import("@/lib/api").MessageContext | null; forwarded?: import("@/lib/api").MessageContext | null; file?: {name: string; size: string}}[]>>({ f1: [
    {
      id: 1,
      sender: "Hoàng Đạt",
      time: "09:40 SA",
      isMe: false,
      text: "Chào Minh! Mình gửi tài liệu tổng hợp giao diện nhé, bạn xem qua rồi phản hồi giúp mình.",
      file: {
        name: "Tai_Lieu_Giao_Dien_v2.pdf",
        size: "2.4 MB",
      },
    },
    {
      id: 2,
      sender: `Bạn (${currentUser})`,
      time: "09:45 SA",
      isMe: true,
      text: "Cảm ơn Đạt nhé! Mình đã nhận được file rồi, giao diện nhìn rất gọn và đẹp.",
    },
    {
      id: 3,
      sender: "Hoàng Đạt",
      time: "09:46 SA",
      isMe: false,
      text: "Khi nào rảnh anh em mình bấm gọi thoại trao đổi nhanh một chút nhé!",
    },
  ] });

  const friends: FriendContact[] = backend ? [...backend.data.friends.map(person => ({id:person.user_id,init:initials(person.full_name),name:person.full_name,username:`@${person.username}`,status:"Bạn bè",online:false,dot:"#b7aabe"})), ...backend.data.dms.flatMap(room => (room.member_ids || []).filter(id => id !== backend.data.profile.user_id && !backend.data.friends.some(person => person.user_id === id)).map(id => ({id,init:initials(backend.nameOf(id)),name:backend.nameOf(id),username:"",status:"DM chỉ đọc",online:false,dot:"#b7aabe"})))].filter((person,index,all) => all.findIndex(item => item.id === person.id) === index) : previewFriends;
  const pendingRequests = backend ? backend.data.friend_requests.map(request => ({id:request.request_id,init:initials(request.user.full_name),name:request.user.full_name,username:`@${request.user.username}`,time:request.requester_id === backend.data.profile.user_id ? "Đã gửi · đang chờ phản hồi" : "Lời mời đến · bấm để chấp nhận"})) : previewPendingRequests;
  const suggestions = backend ? [] : previewSuggestions;
  const activeFriend = friends.find(f => f.id === selectedFriendId) || friends[0] || { id:"", name:"Chọn một người bạn", init:"", username:"", status:"", online:false, dot:"#b7aabe" };
  const dmRoom = backend?.data.dms.find(room => room.member_ids?.includes(activeFriend.id));
  const live = useRoomMessages(dmRoom?.room_id);
  useEffect(() => { setReply(null); setForward(null); }, [activeFriend.id]);
  useEffect(() => { if (dmRoom?.is_read_only) setReply(null); }, [dmRoom?.is_read_only]);
  useEffect(() => { if (backend) { setFiles([]); setDmInput(''); } },[activeFriend.id]);
  useEffect(() => {
    if (!backend) return;
    const wanted = friends.find(person => person.id === initialFriendName || person.name === initialFriendName) || friends[0];
    if (wanted) setSelectedFriendId(wanted.id);
  }, [initialFriendName, !!backend]);
  useEffect(() => {
    if (!backend || friends.some(person => person.id === selectedFriendId)) return;
    const wanted = friends.find(person => person.id === initialFriendName || person.name === initialFriendName) || friends[0];
    if (wanted) setSelectedFriendId(wanted.id);
  }, [backend?.data.friends, backend?.data.dms, selectedFriendId, initialFriendName]);
  useEffect(() => {
    if (!backend || !activeFriend.id || dmRoom || !backend.data.friends.some(person => person.user_id === activeFriend.id)) return;
    let active = true;
    void backend.api.request('/dms', 'POST', {target_user_id:activeFriend.id}).then(() => { if (active) return backend.refresh(); }).catch(backend.report);
    return () => { active = false; };
  }, [backend?.api, activeFriend.id, dmRoom?.room_id]);
  const dmMessages = backend ? live.items.map(item => messageView(item, backend)) : messagesByFriend[selectedFriendId] || [];


  const displayedFriends =
    dmFilter === "online" ? friends.filter((f) => f.online) : friends;

  const handleSendDm = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!dmInput.trim() && !files.length) return;
    if (backend) { if (dmRoom?.is_read_only) return; if (await live.send(dmInput,files,reply ? String(reply.id) : undefined)) {setDmInput("");setFiles([]);setReply(null);} return; }
    setMessagesByFriend(previous => ({ ...previous, [selectedFriendId]: [
      ...(previous[selectedFriendId] || []),
      {
        id: Date.now(),
        sender: `Bạn (${currentUser})`,
        time: "Vừa xong",
        isMe: true,
        text: dmInput,
        reply: reply ? {message_id:String(reply.id),room_id:'preview',sender_name:reply.sender,text:reply.text} : undefined,
      },
    ] }));
    setDmInput("");
  };

  const handleSendFriendRequest = (person: {
    id: string;
    init: string;
    name: string;
    username: string;
  }) => {
    if (pendingRequests.some((p) => p.username === person.username)) return;
    setPendingRequests((prev) => [
      {
        id: `p_${Date.now()}`,
        init: person.init,
        name: person.name,
        username: person.username,
        time: "Vừa gửi lời mời",
      },
      ...prev,
    ]);
  };

  const handleAddByInput = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!addUsernameInput.trim()) return;
    const raw = addUsernameInput.trim();
    if (backend) {
      try {
        const result = await backend.api.request<{items:{user_id:string;username:string}[]}>(`/users/search?q=${encodeURIComponent(raw.replace(/^@/,''))}&limit=50`);
        const person = result.items.find(person => person.username.toLocaleLowerCase() === raw.replace(/^@/,'').toLocaleLowerCase());
        if (!person) throw new Error("Không tìm thấy tên đăng nhập chính xác. Vui lòng kiểm tra lại.");
        await backend.api.request('/friends/requests','POST',{target_user_id:person.user_id}); await backend.refresh(); setAddUsernameInput(''); setDmFilter('pending');
      } catch(error) { backend.report(error); }
      return;
    }
    const initials = raw.slice(0, 2).toUpperCase();
    setPendingRequests((prev) => [
      {
        id: `p_${Date.now()}`,
        init: initials,
        name: raw,
        username: raw.startsWith("@")
          ? raw
          : `@${raw.toLowerCase().replace(/\s+/g, "")}`,
        time: "Vừa gửi lời mời",
      },
      ...prev,
    ]);
    setAddUsernameInput("");
    setDmFilter("pending");
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col sm:flex-row overflow-hidden">
      {forward && <ForwardDialog message={forward} sourceRoom={dmRoom?.room_id} onClose={() => setForward(null)} previewTargets={friends.map(friend => ({id:friend.id,name:friend.name,detail:friend.username,friend:true}))} onPreviewSend={(target, message, note) => setMessagesByFriend(previous => ({...previous,[target]:[...(previous[target] || []),{id:Date.now(),sender:currentUser,isMe:true,time:'Vừa xong',text:note,forwarded:message.forwarded || {message_id:String(message.id),room_id:'preview',sender_name:message.sender,text:message.text}}]}))}/>}
      {/* Cột Danh bạ & Bộ lọc bên trái */}
      <aside aria-label="Danh bạ" className="w-full max-sm:h-52 sm:w-60 lg:w-80 bg-[#120f19] border-b sm:border-b-0 sm:border-r border-white/10 p-3 sm:p-5 flex flex-col shrink-0">
        <div className="space-y-4 flex-1 overflow-y-auto p-1">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
              TIN NHẮN TRỰC TIẾP
            </span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-[#a2d9d3]/15 text-[#a2d9d3] font-semibold">
              {backend ? "Bạn bè" : `${friends.filter((f) => f.online).length} trực tuyến`}
            </span>
          </div>

          {/* 4 Nút Tab: Tất cả | Trực tuyến | Chờ phản hồi | Thêm bạn */}
          <div className="grid grid-cols-2 gap-1.5 bg-[#1c172c] p-1.5 rounded-xl border border-white/10 text-xs">
            <button
              onClick={() => setDmFilter("all")}
              aria-pressed={dmFilter === "all"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition ${
                dmFilter === "all"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              Tất cả ({friends.length})
            </button>
            <button
              disabled={!!backend} title={backend ? "API chưa công bố trạng thái hiện diện" : undefined} onClick={() => setDmFilter("online")}
              aria-pressed={dmFilter === "online"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition ${
                dmFilter === "online"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              {backend ? "Hiện diện" : `Trực tuyến (${friends.filter((f) => f.online).length})`}
            </button>
            <button
              onClick={() => setDmFilter("pending")}
              aria-pressed={dmFilter === "pending"}
              className={`workspace-action py-1.5 rounded-lg font-semibold transition flex items-center justify-center gap-1 ${
                dmFilter === "pending"
                  ? "bg-[#c8b2f1] text-[#271737]"
                  : "text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              <span>Chờ phản hồi</span>
              {pendingRequests.length > 0 && (
                <span className="px-1.5 py-0.2 rounded-full bg-[#a2d9d3] text-[#15121d] text-xs font-bold">
                  {pendingRequests.length}
                </span>
              )}
            </button>
            <button
              onClick={() => setDmFilter("add")}
              aria-pressed={dmFilter === "add"}
              className={`workspace-action py-1.5 rounded-lg font-bold transition flex items-center justify-center gap-1 ${
                dmFilter === "add"
                  ? "bg-[#a2d9d3] text-[#15121d]"
                  : "text-[#a2d9d3] hover:bg-white/5"
              }`}
            >
              <UserPlus className="w-3 h-3 shrink-0" />
              <span>Thêm bạn</span>
            </button>
          </div>

          {/* NỘI DUNG TỪNG TAB */}
          {(dmFilter === "all" || dmFilter === "online") && (
            <div className="space-y-2 pt-1">
              {displayedFriends.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  onClick={() => { setSelectedFriendId(f.id); setMessageSearch(""); }}
                  aria-pressed={selectedFriendId === f.id}
                  className={`w-full text-left p-3 rounded-xl border transition flex items-center justify-between ${
                    selectedFriendId === f.id
                      ? "bg-[#1c172c] border-[#c8b2f1]/50 shadow-md"
                      : "border-transparent hover:bg-white/5"
                  }`}
                >
                  <span className="flex items-center gap-3 min-w-0">
                    <span className="w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center relative shrink-0">
                      <PreviewAvatar name={f.name} initials={f.init} />
                      <span
                        className="w-2.5 h-2.5 rounded-full border-2 border-[#120f19] absolute -bottom-0.5 -right-0.5"
                        style={{ backgroundColor: f.dot }}
                      />
                    </span>
                    <span className="min-w-0">
                      <span className="block text-sm font-semibold text-[#f4effa] truncate">
                        {f.name}
                      </span>
                      <span className="block text-xs text-[#b7aabe] truncate mt-0.5">
                        {f.status}
                      </span>
                    </span>
                  </span>
                </button>
              ))}
            </div>
          )}

          {/* TAB: CHỜ PHẢN HỒI (Những người mình đã bấm kết bạn nhưng chưa phản hồi) */}
          {dmFilter === "pending" && (
            <div className="space-y-2.5 pt-1">
              <div className="text-xs font-bold uppercase tracking-wider text-[#b7aabe] px-1">
                LỜI MỜI KẾT BẠN ({pendingRequests.length})
              </div>
              {pendingRequests.length === 0 ? (
                <p className="text-xs text-[#b7aabe] p-3 rounded-xl bg-[#1c172c]/50">
                  Chưa có lời mời kết bạn nào đang chờ phản hồi.
                </p>
              ) : (
                pendingRequests.map((req) => (
                  <div
                    key={req.id}
                    className="p-3 rounded-xl bg-[#1c172c]/80 border border-white/10 flex items-center justify-between gap-2"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <div className="relative w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center shrink-0">
                        <PreviewAvatar name={req.name} initials={req.init} />
                      </div>
                      <div className="min-w-0">
                        <div className="text-xs font-bold text-[#f4effa] truncate">
                          {req.name}
                        </div>
                        <div className="text-xs text-[#a2d9d3] flex items-center gap-1 mt-0.5">
                          <Clock className="w-3 h-3 shrink-0" />
                          <span className="truncate">{req.time}</span>
                        </div>
                      </div>
                    </div>

                    <button
                      disabled={!!backend && backend.data.friend_requests.find(item => item.request_id === req.id)?.requester_id === backend.data.profile.user_id}
                      onClick={() => backend ? void backend.api.request(`/friends/requests/${req.id}`, 'PATCH', {decision:'ACCEPT'}).then(backend.refresh).catch(backend.report) :
                        setPendingRequests((prev) =>
                          prev.filter((p) => p.id !== req.id),
                        )
                      }
                      title={backend ? "Chấp nhận lời mời" : "Hủy lời mời"}
                      className="p-1.5 rounded-lg bg-white/5 hover:bg-red-500/20 text-[#b7aabe] hover:text-red-300 transition shrink-0"
                    >
                      {backend ? <Check className="w-3.5 h-3.5 shrink-0"/> : <X className="w-3.5 h-3.5 shrink-0" />}
                    </button>
                  </div>
                ))
              )}
            </div>
          )}

          {/* TAB: THÊM BẠN */}
          {dmFilter === "add" && (
            <div className="space-y-4 pt-1">
              <form onSubmit={handleAddByInput} className="space-y-2">
                <label htmlFor="dm-add-contact" className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  {backend ? "TÊN ĐĂNG NHẬP CỦA BẠN BÈ" : "NHẬP TÊN HOẶC ID NGƯỜI DÙNG"}
                </label>
                <div className="flex gap-1.5">
                  <input
                    id="dm-add-contact"
                    type="text"
                    value={addUsernameInput}
                    onChange={(e) => setAddUsernameInput(e.target.value)}
                    placeholder={backend ? "Nhập tên đăng nhập của bạn bè" : "VD: Minh Khoa hoặc @minhkhoa"}
                    className="flex-1 min-w-0 bg-[#1c172c] border border-white/10 rounded-xl px-3 py-2 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1]"
                  />
                  <button
                    type="submit"
                    className="px-3 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] text-xs font-bold shrink-0"
                  >
                    Kết bạn
                  </button>
                </div>
              </form>

              <div className="space-y-2">
                <div className="text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  GỢI Ý KẾT BẠN
                </div>
                {suggestions.map((sug) => {
                  const isSent = pendingRequests.some(
                    (p) => p.username === sug.username,
                  );
                  return (
                    <div
                      key={sug.id}
                      className="p-3 rounded-xl bg-[#1c172c]/70 border border-white/10 flex items-center justify-between gap-2"
                    >
                      <div className="flex items-center gap-2.5 min-w-0">
                        <div className="relative w-8 h-8 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center shrink-0">
                          <PreviewAvatar name={sug.name} initials={sug.init} />
                        </div>
                        <div className="min-w-0">
                          <div className="text-xs font-bold text-[#f4effa] truncate">
                            {sug.name}
                          </div>
                          <div className="text-xs text-[#b7aabe] truncate">
                            {sug.mutual}
                          </div>
                        </div>
                      </div>

                      <button
                        onClick={() => handleSendFriendRequest(sug)}
                        className={`px-2.5 py-1.5 rounded-lg text-xs font-bold transition shrink-0 ${
                          isSent
                            ? "bg-[#a2d9d3]/20 text-[#a2d9d3]"
                            : "bg-[#c8b2f1] text-[#271737] hover:bg-[#d8c7f7]"
                        }`}
                      >
                        {isSent ? "Chờ phản hồi" : "+ Kết bạn"}
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </aside>

      {/* Khu vực Trò chuyện 1-1 */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col bg-[#15121d] overflow-hidden">
        <div className="min-h-15 px-4 lg:px-8 py-2 border-b border-white/10 flex flex-wrap gap-2 items-center justify-between shrink-0">
          <div className="flex min-w-0 items-center gap-3.5">
            <div className="relative w-9 h-9 shrink-0 rounded-full bg-[#261e3a] border border-[#c8b2f1]/40 text-[#c8b2f1] text-xs font-bold grid place-items-center">
              <PreviewAvatar name={activeFriend.name} initials={activeFriend.init} />
            </div>
            <div>
              <div className="text-sm font-bold flex flex-wrap items-center gap-x-2 gap-y-0.5">
                {activeFriend.name}
                <span className="text-xs font-normal text-[#b7aabe]">
                  {activeFriend.username}
                </span>
                <span className="text-xs px-2 py-0.5 rounded-full bg-[#a2d9d3]/15 text-[#a2d9d3]">
                  • {activeFriend.status}
                </span>
              </div>
            </div>
          </div>

          <div className="relative">
            <button aria-label="Thông tin hội thoại" aria-expanded={detailsOpen} onClick={() => setDetailsOpen(!detailsOpen)} className="workspace-icon-button rounded-lg bg-[#261e3a] hover:bg-[#3a2c4b]"><Menu size={19}/></button>
            {detailsOpen && <div className="absolute right-0 top-full z-30 mt-2 w-72 max-w-[90vw] rounded-xl border border-white/15 bg-[#211a2e] p-3 shadow-xl">
              <div className="flex flex-wrap gap-1">{["Tệp", "Phương tiện", "Tìm tin nhắn", "Tin nhắn đã ghim"].map(tab => <button key={tab} onClick={() => setDetailTab(tab)} className={`rounded-lg px-2 py-2 text-xs ${detailTab===tab ? "bg-[#c8b2f1] text-[#271737]" : "hover:bg-white/10"}`}>{tab}</button>)}</div>
              {detailTab==="Tìm tin nhắn" ? <label className="mt-3 block text-xs">Tìm trong hội thoại<input value={messageSearch} onChange={e=>setMessageSearch(e.target.value)} className="mt-2 w-full rounded-lg border border-white/15 bg-[#15121d] p-2 text-sm" placeholder="Nhập nội dung"/></label> : detailTab === "Tin nhắn đã ghim" ? <div className="mt-3 space-y-2 text-sm text-[#d1c7d9]">{(pinnedMessages[selectedFriendId] || []).length ? pinnedMessages[selectedFriendId].map(text => <p key={text} className="rounded-lg bg-[#15121d] p-2">{text}</p>) : <p>Chưa có tin nhắn đã ghim.</p>}</div> : detailTab === "Tệp" ? <div className="mt-3 space-y-2 text-sm text-[#d1c7d9]">{dmMessages.some(message => message.file) ? dmMessages.filter(message => message.file).map(message => <p key={message.id} className="rounded-lg bg-[#15121d] p-2">{message.file?.name} · {message.file?.size}</p>) : <p>Chưa có tệp đính kèm.</p>}</div> : <p className="mt-4 text-sm text-[#b7aabe]">Chưa có phương tiện trong bản xem thử.</p>}
            </div>}
          </div>
        </div>

        {/* Lịch sử Tin nhắn 1-1 */}
        <div className="flex-1 min-h-0 overflow-y-auto px-4 lg:px-8 py-4 sm:py-6">
          <div className="max-w-3xl mx-auto space-y-5">
            <div className="flex items-center gap-4 text-xs text-[#b7aabe] uppercase tracking-wider">
              <div className="h-[1px] flex-1 bg-white/10" />
              <span className="px-3.5 py-1 rounded-full bg-[#1c172c] border border-white/10 font-semibold">
                HÔM NAY
              </span>
              <div className="h-[1px] flex-1 bg-white/10" />
            </div>

            {backend && live.loading && <p role="status" className="text-sm text-[#b7aabe]">Đang tải tin nhắn…</p>}
            {backend && live.error && <p role="alert" className="text-sm text-[#ffa8b5]">{live.error}</p>}
            {backend && !activeFriend.id && <p className="text-sm text-[#b7aabe]">Chưa có bạn bè. Mở Thêm bạn để gửi lời mời kết bạn.</p>}
            {backend && dmRoom?.is_read_only && <p role="status" className="rounded-xl bg-[#261e3a] p-3 text-sm text-[#c8b2f1]">DM đang ở chế độ chỉ đọc. Cần kết bạn để tiếp tục nhắn tin.</p>}
            {backend && live.hasMore && <button disabled={live.loadingOlder} onClick={() => void live.older()} className="workspace-action rounded-xl border border-white/15 px-4 py-2 text-sm">{live.loadingOlder ? 'Đang tải…' : 'Xem tin nhắn trước'}</button>}
            {dmMessages.filter(dm => dm.text.toLocaleLowerCase("vi").includes(messageSearch.toLocaleLowerCase("vi"))).map((dm) => (
              <MessageActions
                key={dm.id}
                messageId={String(dm.id)}
                isMe={dm.isMe}
                onDelete={() => { if (backend) { void live.remove(String(dm.id)); return; } setMessagesByFriend(previous => ({ ...previous, [selectedFriendId]: (previous[selectedFriendId] || []).filter(message => message.id !== dm.id) })); setPinnedMessages(previous => ({ ...previous, [selectedFriendId]: (previous[selectedFriendId] || []).filter(text => text !== dm.text) })); }}
                onReaction={backend ? emoji => live.react(String(dm.id), emoji) : undefined}
                reactionCounts={backend ? live.items.find(item => item.message_id === dm.id)?.reaction_counts : undefined}
                myReaction={backend ? live.items.find(item => item.message_id === dm.id)?.my_reaction : undefined}
                readOnly={!!backend && !!dmRoom?.is_read_only}
                text={dm.text} onReply={() => { setReply(dm); composer.current?.focus(); }} onForward={() => setForward(dm)} onPin={(text, pinned) => setPinnedMessages(previous => ({ ...previous, [selectedFriendId]: pinned ? [...(previous[selectedFriendId] || []), text] : (previous[selectedFriendId] || []).filter(item => item !== text) }))}
              >
                <div className="text-xs text-[#b7aabe] mb-1.5 px-1">
                  <strong className="text-[#f4effa]">{dm.sender}</strong> •{" "}
                  {dm.time}
                </div>
                <div
                  className={`w-fit max-w-full lg:max-w-xl p-4 rounded-2xl text-sm leading-relaxed wrap-anywhere ${
                    dm.isMe
                      ? "bg-[#c8b2f1] text-[#271737] font-medium rounded-br-none shadow-lg"
                      : "bg-[#1c172c] border border-white/10 text-[#f4effa] rounded-bl-none"
                  }`}
                >
                  <MessageQuote context={dm.reply}/>
                  <MessageQuote context={dm.forwarded} forwarded/>
                  <span className="whitespace-pre-wrap">{dm.text}</span>
                  {backend && <BackendAttachments attachments={live.items.find(item => item.message_id === dm.id)?.attachments || []}/>}

                  {!backend && dm.file && (
                    <div className="mt-3 p-3 rounded-xl bg-[#15121d] border border-white/10 flex flex-wrap items-center justify-between gap-3">
                      <div>
                        <div className="font-semibold text-[#f4effa]">
                          {dm.file.name}
                        </div>
                        <div className="text-xs text-[#a2d9d3] mt-0.5">
                          {dm.file.size}
                        </div>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <button className="px-2.5 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 text-xs flex items-center gap-1 text-[#f4effa]">
                          <Eye size={12} /> Xem
                        </button>
                        <button aria-label="Tải tệp đính kèm" className="workspace-icon-button rounded-lg bg-[#c8b2f1]/20 text-[#c8b2f1]">
                          <Download size={13} />
                        </button>
                      </div>
                    </div>
                  )}


                </div>
                {dm.isMe && (
                  <span className="text-xs text-[#a2d9d3] mt-1">
                    ✓ Đã gửi
                  </span>
                )}
              </MessageActions>
            ))}
          </div>
        </div>

        {/* Khung Nhập tin nhắn 1-1 (Đã bỏ chữ P2P Stream) */}
        <form
          onSubmit={handleSendDm}
          className="px-3 lg:px-8 py-2 sm:py-4 border-t border-white/10 bg-[#120f19] shrink-0"
        >
          <div className="max-w-3xl mx-auto rounded-2xl bg-[#1c172c] border border-white/15 p-3 space-y-2 shadow-lg">
            {reply && <ReplyBanner message={reply} onCancel={() => {setReply(null);composer.current?.focus();}}/>}
            <div className="flex flex-wrap items-center justify-between gap-x-2 text-xs text-[#b7aabe]">
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  aria-label="In đậm"
                  className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1] font-bold"
                >
                  B
                </button>
                <button type="button" aria-label="In nghiêng" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1] italic">
                  I
                </button>
                <button type="button" aria-label="Định dạng mã" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Code size={13} />
                </button>
                <button type="button" aria-label="Chèn liên kết" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Link2 size={13} />
                </button>
                <button type="button" aria-label="Chèn biểu cảm" className="workspace-icon-button rounded-md hover:bg-white/10 hover:text-[#c8b2f1]">
                  <Smile size={13} />
                </button>
              </div>
              <span className="hidden sm:inline">Enter để gửi tin nhắn</span>
            </div>

            <input
              type="text"
              maxLength={4000}
              disabled={!!backend && (!dmRoom || dmRoom.is_read_only || live.loading || live.sending || !!live.error)}
              ref={composer}
              value={dmInput}
              aria-label={`Nhắn tin riêng cho ${activeFriend.name}`}
              onChange={(e) => setDmInput(e.target.value)}
              placeholder={reply ? `Nhắn @${reply.sender}` : `Nhắn tin riêng cho ${activeFriend.name}...`}
              className="w-full bg-[#15121d]/80 border border-white/10 rounded-xl px-4 py-2.5 text-sm text-[#f4effa] placeholder-[#b7aabe] focus:border-[#c8b2f1] transition"
            />

            <div className="flex items-center justify-between pt-1">
              <div className="flex items-center gap-2.5">
                {backend && <BackendFilePicker files={files} onChange={setFiles} disabled={!dmRoom || dmRoom.is_read_only || live.loading || live.sending}/>}
              </div>
              <button
                type="submit"
                disabled={!!backend && (!dmRoom || dmRoom.is_read_only || live.loading || live.sending || !!live.error || (!dmInput.trim() && !files.length))}
                className="workspace-action px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] font-bold flex items-center gap-1.5 shadow transition"
              >
                Gửi <Send size={13} />
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
};
