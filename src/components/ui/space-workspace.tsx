import React, { useState, useRef, useEffect } from "react";
import {
  Bell,
  MessageSquare,
  ChevronDown,
  Search,
  Users,
  LayoutGrid,
  Compass,
  Plus,
  X,
  Code2,
  ShieldAlert,
  Gamepad2,
  UserPlus,
  Settings,
  Shield,
  LogOut as LeaveIcon,
  Copy,
  Check,
  Crown,
  ArrowLeft,
  Wifi,
} from "lucide-react";
import {
  WorkspaceTab,
  ChatChannelItem,
  VoiceRoomItem,
} from "./workspace/types";
import { OverviewView } from "./workspace/overview-view";
import { ChannelChatView } from "./workspace/channel-chat-view";
import { DirectMessageView } from "./workspace/direct-message-view";
import { VoiceStageView } from "./workspace/voice-stage-view";
import { ScreenShareView } from "./workspace/screen-share-view";
import { AdminRolesView } from "./workspace/admin-roles-view";
import { UserSettingsView } from "./workspace/user-settings-view";
import { HomeHubView, HomeServerItem } from "./workspace/home-hub-view";
import { PreviewAvatar, PreviewServerPhoto } from "./workspace/preview-images";
import { ShareVideo } from "./workspace/share-video";
import { useScreenShare } from "@/lib/use-screen-share";

export interface SpaceWorkspaceProps {
  currentUser?: string;
  onLogout?: () => void;
}

export interface ServerExtendedItem extends HomeServerItem {
  serverCode: string;
  inviteLink: string;
  isOwner: boolean;
}

interface NotificationItem {
  id: string;
  type: "dm" | "server";
  sender: string;
  target: string;
  text: string;
  time: string;
  unread: boolean;
}

export function SpaceWorkspace({
  currentUser = "Nguyễn Minh",
  onLogout,
}: SpaceWorkspaceProps) {
  // 1. VỊ TRÍ ĐẦU TIÊN KHI VÀO APP LÀ SẢNH CHỜ (isInLobby = true)
  const [isInLobby, setIsInLobby] = useState<boolean>(true);
  const [activeTab, setActiveTab] = useState<WorkspaceTab>("overview");
  // Lưu kênh đang truy cập (kể cả khi bấm sang DM thì kênh này vẫn hiển thị trên Topbar)
  const [activeServerId, setActiveServerId] = useState<string>("uiux");
  const [userSettingsOpen, setUserSettingsOpen] = useState(false);
  const [dmFriendName, setDmFriendName] = useState<string | undefined>();
  const [recentServerIds, setRecentServerIds] = useState<string[]>([]);

  // Lịch sử điều hướng để nút Quay lại (Back) trên Topbar hoạt động mượt mà
  const [navHistory, setNavHistory] = useState<
    { isInLobby: boolean; tab: WorkspaceTab; serverId: string }[]
  >([]);

  // State ẩn/hiện thanh Dock bằng nút mũi tên (Mặc định ẩn để không che thanh chat)

  const [joinedServers, setJoinedServers] = useState<ServerExtendedItem[]>([
    {
      id: "uiux",
      name: "Nhóm Đồ Án - UI/UX",
      shortName: "UI",
      category: "Đồ Án Tốt Nghiệp",
      membersCount: 12,
      onlineCount: 4,
      onlineDot: true,
      icon: LayoutGrid,
      serverCode: "WS-8849-VN",
      inviteLink: "https://meet.space.vn/inv/ui-ux-k21-992a",
      isOwner: true,
    },
    {
      id: "dev",
      name: "SpaceLess DEV & Backend",
      shortName: "DEV",
      category: "Kỹ Thuật Phần Mềm",
      membersCount: 18,
      onlineCount: 7,
      badge: 3,
      icon: Code2,
      serverCode: "WS-7102-DEV",
      inviteLink: "https://meet.space.vn/inv/spaceless-dev-02",
      isOwner: true,
    },
    {
      id: "sec",
      name: "Lab An Toàn Thông Tin",
      shortName: "SEC",
      category: "Nghiên Cứu Bảo Mật",
      membersCount: 24,
      onlineCount: 6,
      onlineDot: true,
      icon: ShieldAlert,
      serverCode: "WS-9011-SEC",
      inviteLink: "https://meet.space.vn/inv/crypto-lab-sec",
      isOwner: false,
    },
    {
      id: "game",
      name: "Hội Anh Em Chiến Game",
      shortName: "GG",
      category: "Cộng Đồng & Giải Trí",
      membersCount: 45,
      onlineCount: 11,
      icon: Gamepad2,
      serverCode: "WS-5540-GG",
      inviteLink: "https://meet.space.vn/inv/gaming-chill-gg",
      isOwner: false,
    },
  ]);

  const [chatChannels, setChatChannels] = useState<ChatChannelItem[]>([
    {
      id: "c1",
      name: "chung",
      tag: "CHÍNH",
      sub: "12 thành viên • 2 phút trước",
      badge: "3 mới",
    },
    {
      id: "c2",
      name: "thiet-ke-ui-ux",
      sub: "Design Sprint 4 • 15 phút trước",
      dot: true,
    },
    {
      id: "c3",
      name: "bao-cao-tien-do",
      sub: "Tự động 09:00 • 1 giờ trước",
    },
    {
      id: "c4",
      name: "tai-nguyen-assets",
      sub: "42 Files tài nguyên • Hôm qua",
    },
  ]);

  const [voiceRooms, setVoiceRooms] = useState<VoiceRoomItem[]>([
    {
      id: "v1",
      name: "Sảnh Chill & Âm nhạc",
      sub: "Bot Lo-Fi • 2 người nghe",
    },
    {
      id: "v2",
      name: "Phòng Dev & Code Review",
      sub: "Phòng trống • Sẵn sàng",
    },
    {
      id: "v3",
      name: "Phòng Gaming & Giải trí",
      sub: "Cần quyền quản trị",
      locked: true,
    },
  ]);

  const [selectedChatChannel, setSelectedChatChannel] =
    useState<string>("chung");
  const [selectedVoiceRoom, setSelectedVoiceRoom] =
    useState<string>("Phòng Họp Nhóm");
  const sharing = useScreenShare(activeServerId, selectedVoiceRoom);
  const [viewedShareOwner, setViewedShareOwner] = useState(currentUser);
  const screenShares = sharing.stream ? [{ owner: currentUser, stream: sharing.stream, isLocal: true }] : [];

  // States điều khiển Dropdown & Popup
  const [serverDropdownOpen, setServerDropdownOpen] = useState(false);
  const [notifDropdownOpen, setNotifDropdownOpen] = useState(false);
  const [inviteModalOpen, setInviteModalOpen] = useState(false);
  const [settingsModalOpen, setSettingsModalOpen] = useState(false);
  const [settingsInitialMenu, setSettingsInitialMenu] = useState<number>(0);
  const [createChannelModal, setCreateChannelModal] = useState<
    "chat" | "voice" | null
  >(null);
  const [quickPopupType, setQuickPopupType] = useState<
    "notifications" | "privacy" | "leave" | null
  >(null);

  // Danh sách thông báo & Popup Toast thông báo góc màn hình
  const [notifications, setNotifications] = useState<NotificationItem[]>([
    {
      id: "n1",
      type: "dm",
      sender: "Hoàng Đạt",
      target: "Tin nhắn trực tiếp",
      text: "Chào Minh! Mình vừa gửi file tài liệu giao diện mới nhé.",
      time: "1 phút trước",
      unread: true,
    },
    {
      id: "n2",
      type: "server",
      sender: "Minh Anh",
      target: "Nhóm Đồ Án - UI/UX (#chung)",
      text: "Mọi người kiểm tra bản thiết kế trước buổi họp 14:00 chiều nay.",
      time: "5 phút trước",
      unread: true,
    },
    {
      id: "n3",
      type: "server",
      sender: "Thu Hà",
      target: "Nhóm Đồ Án - UI/UX (#thiet-ke-ui-ux)",
      text: "Đã cập nhật xong thanh Dock ẩn/hiện bằng nút mũi tên.",
      time: "12 phút trước",
      unread: false,
    },
  ]);

  const [toastPopup, setToastPopup] = useState<NotificationItem | null>(null);

  // Tự động hiện 1 Popup thông báo mẫu sau khi vào app 2.5 giây
  useEffect(() => {
    const timer = setTimeout(() => {
      setToastPopup({
        id: "toast_init",
        type: "dm",
        sender: "Hoàng Đạt",
        target: "Tin nhắn riêng mới",
        text: "Minh ơi, khi nào vào app nhớ kiểm tra phần tin nhắn trực tiếp nhé!",
        time: "Vừa xong",
        unread: true,
      });
    }, 2500);
    return () => clearTimeout(timer);
  }, []);

  // Tự ẩn Toast sau 5 giây
  useEffect(() => {
    if (!toastPopup) return;
    const t = setTimeout(() => setToastPopup(null), 5000);
    return () => clearTimeout(t);
  }, [toastPopup]);

  const triggerSimulatedIncomingMessage = () => {
    const incoming: NotificationItem = {
      id: `n_${Date.now()}`,
      type: Math.random() > 0.5 ? "dm" : "server",
      sender: Math.random() > 0.5 ? "Thu Hà" : "Quốc Bảo",
      target:
        Math.random() > 0.5
          ? "Tin nhắn trực tiếp"
          : "Nhóm Đồ Án - UI/UX (#chung)",
      text: "Mình vừa cập nhật tiến độ mới nhất vào kênh nhé!",
      time: "Vừa xong",
      unread: true,
    };
    setNotifications((prev) => [incoming, ...prev]);
    setToastPopup(incoming);
  };

  // States bên trong Modal Mời bạn bè & Tạo kênh
  const [friendSearch, setFriendSearch] = useState("");
  const [invitedIds, setInvitedIds] = useState<string[]>([]);
  const [copiedInviteLink, setCopiedInviteLink] = useState(false);
  const [copiedServerId, setCopiedServerId] = useState(false);
  const [newChannelName, setNewChannelName] = useState("");
  const [newChannelDesc, setNewChannelDesc] = useState("");
  const [newChannelPrivate, setNewChannelPrivate] = useState(false);

  const dropdownRef = useRef<HTMLDivElement>(null);
  const notifRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(e.target as Node)
      ) {
        setServerDropdownOpen(false);
      }
      if (notifRef.current && !notifRef.current.contains(e.target as Node)) {
        setNotifDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const currentServer =
    joinedServers.find((s) => s.id === activeServerId) || joinedServers[0];
  const isCreator = currentServer?.isOwner ?? false;

  // Hàm chuyển trang có lưu lịch sử để bấm nút Quay lại (Back)
  const navigateWithHistory = (
    nextInLobby: boolean,
    nextTab: WorkspaceTab,
    nextServerId: string = activeServerId,
  ) => {
    setNavHistory((prev) => [
      ...prev,
      { isInLobby, tab: activeTab, serverId: activeServerId },
    ]);
    if (!nextInLobby && nextTab !== "dm") setRecentServerIds(prev => [nextServerId, ...prev.filter(id => id !== nextServerId)].slice(0, 4));
    setIsInLobby(nextInLobby);
    setActiveTab(nextTab);
    setActiveServerId(nextServerId);
  };

  const handleGoBack = () => {
    if (navHistory.length > 0) {
      const last = navHistory[navHistory.length - 1];
      setNavHistory((prev) => prev.slice(0, -1));
      setIsInLobby(last.isInLobby);
      setActiveTab(last.tab);
      setActiveServerId(last.serverId);
    } else if (!isInLobby && activeTab !== "overview") {
      setActiveTab("overview");
    } else {
      setIsInLobby(true);
    }
  };

  const toggleRoleForDemo = () => {
    setJoinedServers((prev) =>
      prev.map((srv) =>
        srv.id === currentServer.id ? { ...srv, isOwner: !srv.isOwner } : srv,
      ),
    );
  };

  const handleUpdateServerProfile = (newName: string, newDesc: string) => {
    if (!isCreator) return;
    setJoinedServers((prev) =>
      prev.map((srv) =>
        srv.id === currentServer.id
          ? { ...srv, name: newName.trim() || srv.name, desc: newDesc }
          : srv,
      ),
    );
  };

  const handleCreateChannelOrRoom = (e: React.FormEvent) => {
    e.preventDefault();
    if (!isCreator || !newChannelName.trim() || !createChannelModal) return;

    if (createChannelModal === "chat") {
      const slug = newChannelName.trim().toLowerCase().replace(/\s+/g, "-");
      setChatChannels((prev) => [
        ...prev,
        {
          id: `c_${Date.now()}`,
          name: slug,
          sub: newChannelDesc.trim() || "Kênh mới tạo • Vừa xong",
          dot: true,
        },
      ]);
    } else {
      setVoiceRooms((prev) => [
        ...prev,
        {
          id: `v_${Date.now()}`,
          name: newChannelName.trim(),
          sub: newChannelDesc.trim() || "Phòng thoại mới • Sẵn sàng",
          locked: newChannelPrivate,
        },
      ]);
    }

    setNewChannelName("");
    setNewChannelDesc("");
    setNewChannelPrivate(false);
    setCreateChannelModal(null);
  };

  const handleCopyServerId = () => {
    navigator.clipboard?.writeText(currentServer.serverCode);
    setCopiedServerId(true);
    setTimeout(() => {
      setCopiedServerId(false);
      setServerDropdownOpen(false);
    }, 1000);
  };

  const friendsList = [
    {
      id: "f1",
      init: "HĐ",
      name: "Hoàng Đạt",
      username: "@hoangdat",
      status: "Trực tuyến",
    },
    {
      id: "f2",
      init: "TH",
      name: "Thu Hà",
      username: "@thuha",
      status: "Trực tuyến",
    },
    {
      id: "f3",
      init: "QB",
      name: "Quốc Bảo",
      username: "@quocbao",
      status: "Trực tuyến",
    },
    {
      id: "f4",
      init: "MA",
      name: "Minh Anh",
      username: "@minhanh",
      status: "Trực tuyến",
    },
  ];

  const filteredFriends = friendsList.filter(
    (f) =>
      f.name.toLowerCase().includes(friendSearch.toLowerCase()) ||
      f.username.toLowerCase().includes(friendSearch.toLowerCase()),
  );

  const unreadCount = notifications.filter((n) => n.unread).length;

  return (
    <div className="space-workspace flex flex-col h-dvh w-full bg-[#15121d] text-[#f4effa] font-sans antialiased overflow-hidden relative">
      {/* POPUP TOAST THÔNG BÁO NỔI KHI CÓ TIN NHẮN MỚI */}
      {toastPopup && (
        <div className="fixed top-16 right-3 sm:right-6 z-50 w-80 max-w-[calc(100%-1.5rem)] p-3.5 rounded-2xl bg-[#1c172c]/95 backdrop-blur-xl border border-[#c8b2f1]/50 shadow-[0_15px_40px_rgba(0,0,0,0.8)] flex items-start gap-3">
          <button
            type="button"
            aria-label={`Mở thông báo của ${toastPopup.sender}`}
            onClick={() => {
              navigateWithHistory(false, toastPopup.type === "dm" ? "dm" : "chat", activeServerId);
              setToastPopup(null);
            }}
            className="absolute inset-0 rounded-2xl"
          />
          <div className="w-9 h-9 rounded-xl bg-[#c8b2f1] text-[#271737] font-bold text-xs grid place-items-center shrink-0">
            <Bell className="w-4 h-4 shrink-0" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-bold text-[#f4effa] truncate">
                {toastPopup.sender}
              </span>
              <span className="text-xs text-[#a2d9d3] shrink-0">
                {toastPopup.time}
              </span>
            </div>
            <div className="text-xs text-[#c8b2f1] truncate">
              {toastPopup.target}
            </div>
            <p className="text-xs text-[#b7aabe] truncate mt-0.5">
              {toastPopup.text}
            </p>
          </div>
          <button
            onClick={() => setToastPopup(null)}
            aria-label="Đóng thông báo"
            className="workspace-icon-button relative rounded-lg text-[#b7aabe] hover:text-[#f4effa] hover:bg-white/10"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* 1. TOP NAVIGATION BAR */}
      <header className="min-h-14 px-3 sm:px-6 border-b border-white/10 bg-[#120f19] flex items-center justify-between gap-2 sm:gap-4 shrink-0 z-50">
        <div className="flex min-w-0 items-center gap-2.5">
          {/* NÚT QUAY LẠI TRANG TRƯỚC PHẦN CÀI ĐẶT MÁY CHỦ */}
          {!isInLobby && <button
            onClick={handleGoBack}
            title="Quay lại trang trước"
            className="p-2 rounded-xl bg-[#1c172c] hover:bg-[#261e3a] border border-white/10 text-[#b7aabe] hover:text-[#f4effa] transition"
          >
            <ArrowLeft className="w-4 h-4 shrink-0" />
          </button>}

          {/* NÚT TÊN MÁY CHỦ HIỆN TẠI (Luôn hiện kênh đang truy cập kể cả khi bấm sang DM) */}
          <div className={isInLobby ? "hidden" : "relative min-w-0 hidden sm:block"} ref={dropdownRef}>
            <button
              onClick={() => setServerDropdownOpen((v) => !v)}
              aria-expanded={serverDropdownOpen}
              className={`workspace-action max-w-full flex items-center gap-2.5 px-3.5 py-1.5 rounded-xl transition border font-semibold ${
                serverDropdownOpen
                  ? "bg-[#261e3a] border-[#c8b2f1]/60 text-[#f4effa]"
                  : "bg-[#1c172c] hover:bg-[#261e3a] border-white/10 text-[#f4effa]"
              }`}
            >
              <span className="w-2 h-2 shrink-0 rounded-full bg-[#c8b2f1]" />
              <span className="truncate">{currentServer.name}</span>
              <ChevronDown
                className={`w-3.5 h-3.5 text-[#b7aabe] transition-transform duration-200 ${
                  serverDropdownOpen ? "rotate-180 text-[#c8b2f1]" : ""
                }`}
              />
            </button>

            {/* SERVER DROPDOWN MENU */}
            {serverDropdownOpen && (
              <div className="absolute left-0 top-full mt-2 w-64 rounded-2xl bg-[#110e18]/98 backdrop-blur-xl border border-white/15 shadow-[0_20px_60px_rgba(0,0,0,0.85)] p-2 z-50 space-y-1">
                <div className="px-3 py-2 rounded-xl bg-[#1c172c]/70 border border-white/5 flex items-center justify-between mb-1">
                  <span className="text-xs font-semibold text-[#b7aabe]">
                    Vai trò:
                  </span>
                  <span
                    className={`text-xs px-2 py-0.5 rounded-full font-bold flex items-center gap-1 ${
                      isCreator
                        ? "bg-[#a2d9d3]/20 text-[#a2d9d3]"
                        : "bg-white/10 text-[#b7aabe]"
                    }`}
                  >
                    {isCreator && <Crown className="w-3 h-3 shrink-0" />}
                    {isCreator ? "Chủ kênh (Creator)" : "Thành viên"}
                  </span>
                </div>

                <button
                  onClick={() => {
                    setServerDropdownOpen(false);
                    setInviteModalOpen(true);
                  }}
                  className="w-full px-3 py-2.5 rounded-xl text-xs font-semibold text-[#c8b2f1] hover:bg-[#c8b2f1] hover:text-[#271737] flex items-center justify-between transition"
                >
                  <span>Mời vào Kênh</span>
                  <UserPlus className="w-4 h-4 shrink-0" />
                </button>

                <button
                  onClick={() => {
                    setServerDropdownOpen(false);
                    setSettingsInitialMenu(0);
                    setSettingsModalOpen(true);
                  }}
                  className="w-full px-3 py-2.5 rounded-xl text-xs font-medium text-[#f4effa] hover:bg-white/10 flex items-center justify-between transition"
                >
                  <span>Cài đặt kênh</span>
                  <Settings className="w-4 h-4 text-[#b7aabe] shrink-0" />
                </button>

                {isCreator && (
                  <>
                    <button
                      onClick={() => {
                        setServerDropdownOpen(false);
                        setCreateChannelModal("chat");
                      }}
                      className="w-full px-3 py-2 rounded-xl text-xs font-medium text-[#f4effa] hover:bg-white/10 flex items-center justify-between transition"
                    >
                      <span>Tạo Kênh / Phòng mới</span>
                      <Plus className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                    </button>
                    <button
                      onClick={() => {
                        setServerDropdownOpen(false);
                        setSettingsInitialMenu(2);
                        setSettingsModalOpen(true);
                      }}
                      className="w-full px-3 py-2 rounded-xl text-xs font-medium text-[#f4effa] hover:bg-white/10 flex items-center justify-between transition"
                    >
                      <span>Quản lý Phân quyền</span>
                      <Users className="w-4 h-4 text-[#c8b2f1] shrink-0" />
                    </button>
                  </>
                )}

                <div className="h-[1px] bg-white/10 my-1" />

                <button
                  onClick={() => {
                    setServerDropdownOpen(false);
                    setQuickPopupType("notifications");
                  }}
                  className="w-full px-3 py-2.5 rounded-xl text-xs font-medium text-[#f4effa] hover:bg-white/10 flex items-center justify-between transition"
                >
                  <span>Cài đặt thông báo</span>
                  <Bell className="w-4 h-4 text-[#b7aabe] shrink-0" />
                </button>

                <button
                  onClick={() => {
                    setServerDropdownOpen(false);
                    setQuickPopupType("privacy");
                  }}
                  className="w-full px-3 py-2.5 rounded-xl text-xs font-medium text-[#f4effa] hover:bg-white/10 flex items-center justify-between transition"
                >
                  <span>Cài đặt bảo mật</span>
                  <Shield className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                </button>

                <div className="h-[1px] bg-white/10 my-1" />

                <button
                  onClick={() => {
                    setServerDropdownOpen(false);
                    setQuickPopupType("leave");
                  }}
                  className="w-full px-3 py-2.5 rounded-xl text-xs font-semibold text-red-400 hover:bg-red-500/20 hover:text-red-300 flex items-center justify-between transition"
                >
                  <span>Rời khỏi kênh</span>
                  <LeaveIcon className="w-4 h-4 shrink-0" />
                </button>

                <div className="h-[1px] bg-white/10 my-1" />

                <button
                  onClick={handleCopyServerId}
                  className="w-full px-3 py-2 rounded-xl text-xs font-medium text-[#b7aabe] hover:bg-white/10 hover:text-[#f4effa] flex items-center justify-between transition"
                >
                  <span>
                    {copiedServerId ? "Đã sao chép ID!" : "Sao chép ID Kênh"}
                  </span>
                  {copiedServerId ? (
                    <Check className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                  ) : (
                    <Copy className="w-4 h-4 shrink-0" />
                  )}
                </button>
              </div>
            )}
          </div>

          <div className="hidden lg:flex items-center gap-2 text-sm text-[#d4c9dd]">
            <button
              onClick={() =>
                navigateWithHistory(true, "overview", activeServerId)
              }
              className={`min-h-8 hover:text-[#c8b2f1] transition ${
                isInLobby ? "text-[#c8b2f1] font-semibold" : ""
              }`}
            >
              Sảnh chờ
            </button>
            <span className="text-white/20">/</span>
            <button
              onClick={() =>
                navigateWithHistory(false, "overview", activeServerId)
              }
              className={`min-h-8 hover:text-[#f4effa] transition ${
                !isInLobby && activeTab !== "dm"
                  ? "text-[#f4effa] font-semibold"
                  : ""
              }`}
            >
              Kênh & Phòng
            </button>
            {!isInLobby && activeTab === "dm" && (
              <>
                <span className="text-white/20">/</span>
                <span className="text-[#a2d9d3] font-semibold">
                  Tin nhắn trực tiếp
                </span>
              </>
            )}
          </div>
          <span className="hidden sm:block lg:hidden whitespace-nowrap text-sm font-semibold text-[#e9dff5]">{userSettingsOpen ? "Cài đặt" : isInLobby ? "Sảnh chờ" : activeTab === "dm" ? "Tin nhắn" : "Kênh & Phòng"}</span>
        </div>

        <nav aria-label="Điều hướng kênh và tin nhắn" className="min-w-0 flex-1 flex justify-center">
          <div className="flex max-w-full items-center gap-1.5 overflow-x-auto px-1 py-2">
            {/* ĐẦU THANH DOCK: ICON TIN NHẮN TRỰC TIẾP (DM) */}
            <button
              onClick={() => {
                navigateWithHistory(false, "dm", activeServerId);
              }}
              title="Tin nhắn trực tiếp (DM)"
              aria-label="Tin nhắn trực tiếp"
              className={`w-10 h-10 shrink-0 rounded-xl flex items-center justify-center transition relative ${
                !isInLobby && activeTab === "dm"
                  ? "bg-[#c8b2f1] text-[#271737] shadow-[0_0_15px_rgba(200,178,241,0.45)]"
                  : "bg-[#1c172c] hover:bg-[#261e3a] text-[#c8b2f1] border border-white/10"
              }`}
            >
              <MessageSquare className="w-4 h-4 shrink-0" />
              <span className="w-4 h-4 rounded-full bg-[#a2d9d3] text-[#15121d] text-xs font-bold grid place-items-center absolute -top-1 -right-1 border border-[#120f19]">
                2
              </span>
            </button>

            <div className="w-[1px] h-6 bg-white/15 mx-0.5" />

            {/* DANH SÁCH MÁY CHỦ (Khi đang ở DM, kênh đang truy cập vẫn có viền sáng báo hiệu) */}
            {joinedServers.map((srv) => {
              const Icon = srv.icon;
              const isSelectedServer = activeServerId === srv.id;
              const isViewingServer =
                isSelectedServer && !isInLobby && activeTab !== "dm";
              return (
                <button
                  key={srv.id}
                  onClick={() => {
                    navigateWithHistory(false, "overview", srv.id);
                      }}
                  title={`Kênh: ${srv.name}`}
                  className={`w-10 h-10 shrink-0 rounded-xl flex items-center justify-center font-bold text-xs transition relative ${
                    isViewingServer
                      ? "bg-[#c8b2f1] text-[#271737] shadow-[0_0_15px_rgba(200,178,241,0.4)]"
                      : isSelectedServer
                        ? "bg-[#261e3a] text-[#c8b2f1] border-2 border-[#c8b2f1]"
                        : "bg-[#1c172c] hover:bg-[#261e3a] text-[#f4effa] border border-white/10"
                  }`}
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  <PreviewServerPhoto id={srv.id} />
                  {srv.onlineDot && (
                    <span className="w-2.5 h-2.5 rounded-full bg-[#a2d9d3] border-2 border-[#120f19] absolute -top-0.5 -right-0.5" />
                  )}
                </button>
              );
            })}

            <div className="w-[1px] h-6 bg-white/15 mx-0.5" />

            {/* Nút về Sảnh chờ */}
            <button
              onClick={() => {
                navigateWithHistory(true, "overview", activeServerId);
              }}
              title="Sảnh chờ"
              className={`w-10 h-10 shrink-0 rounded-xl flex items-center justify-center transition ${
                isInLobby
                  ? "bg-[#a2d9d3] text-[#15121d]"
                  : "bg-[#1c172c]/70 hover:bg-[#1c172c] text-[#b7aabe] hover:text-[#f4effa]"
              }`}
            >
              <Compass className="w-4 h-4 shrink-0" />
            </button>
          </div>
        </nav>

        {/* Right Controls */}
        <div className="flex shrink-0 items-center gap-2">
          {!isInLobby && <button
            onClick={toggleRoleForDemo}
            title="Chuyển thử nghiệm Chủ kênh / Thành viên"
            className={`hidden sm:flex items-center gap-1.5 px-3 py-1.5 rounded-xl border text-xs font-bold transition ${
              isCreator
                ? "bg-[#a2d9d3]/15 border-[#a2d9d3]/40 text-[#a2d9d3]"
                : "bg-white/5 border-white/15 text-[#b7aabe]"
            }`}
          >
            {isCreator ? (
              <>
                <Crown className="w-3.5 h-3.5 shrink-0" />
                <span>Chủ kênh</span>
              </>
            ) : (
              <>
                <Users className="w-3.5 h-3.5 shrink-0" />
                <span>Thành viên</span>
              </>
            )}
          </button>}

          {/* NÚT CHUÔNG THÔNG BÁO + THANH THẢ XUỐNG CỠ BÉ */}
          <div className="relative" ref={notifRef}>
            <button
              onClick={() => setNotifDropdownOpen((v) => !v)}
              aria-label={unreadCount ? `Thông báo (${unreadCount} chưa đọc)` : "Thông báo"}
              aria-expanded={notifDropdownOpen}
              className="workspace-icon-button relative p-2 rounded-xl text-[#b7aabe] hover:bg-white/5 hover:text-[#f4effa] transition"
              title="Thông báo"
            >
              <Bell className="w-4 h-4" />
              {unreadCount > 0 && (
                <span className="min-w-[15px] h-[15px] px-1 rounded-full bg-[#c8b2f1] text-[#271737] text-xs font-bold grid place-items-center absolute top-0.5 right-0.5">
                  {unreadCount}
                </span>
              )}
            </button>

            {notifDropdownOpen && (
              <div className="fixed right-3 sm:right-6 top-14 mt-2 w-80 max-w-[calc(100%-1.5rem)] rounded-2xl bg-[#110e18]/98 backdrop-blur-xl border border-white/15 shadow-[0_20px_60px_rgba(0,0,0,0.85)] p-3 z-50 space-y-2.5">
                <div className="flex items-center justify-between px-1">
                  <span className="text-xs font-bold text-[#f4effa]">
                    Thông báo mới nhất
                  </span>
                  <button
                    onClick={triggerSimulatedIncomingMessage}
                    className="text-xs px-2 py-0.5 rounded bg-[#c8b2f1]/15 text-[#c8b2f1] hover:bg-[#c8b2f1] hover:text-[#271737] font-semibold transition"
                  >
                    + Thử nhận tin mới
                  </button>
                </div>

                <div className="max-h-64 overflow-y-auto space-y-1.5 pr-0.5">
                  {notifications.map((n) => (
                    <button
                      key={n.id}
                      type="button"
                      onClick={() => {
                        setNotifications((prev) =>
                          prev.map((item) =>
                            item.id === n.id
                              ? { ...item, unread: false }
                              : item,
                          ),
                        );
                        setNotifDropdownOpen(false);
                        if (n.type === "dm") {
                          navigateWithHistory(false, "dm", activeServerId);
                        } else {
                          navigateWithHistory(false, "chat", activeServerId);
                        }
                      }}
                      className={`w-full text-left p-2.5 rounded-xl border hover:border-[#c8b2f1]/60 transition ${
                        n.unread
                          ? "bg-[#1c172c] border-[#c8b2f1]/40"
                          : "bg-[#15121d]/60 border-white/5"
                      }`}
                    >
                      <span className="flex items-center justify-between text-xs">
                        <span className="font-bold text-[#c8b2f1]">
                          {n.sender}
                        </span>
                        <span className="text-[#b7aabe]">{n.time}</span>
                      </span>
                      <span className="block text-xs text-[#a2d9d3] truncate">
                        {n.target}
                      </span>
                      <span className="text-sm text-[#f4effa] line-clamp-2 mt-0.5">
                        {n.text}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>

          <button type="button" onClick={() => setUserSettingsOpen(true)} aria-label={`Cài đặt người dùng ${currentUser}`} title="Cài đặt người dùng" aria-haspopup="dialog" className="relative ml-1 block h-9 w-9 shrink-0 rounded-full bg-[#30243f] hover:ring-2 hover:ring-[#c8b2f1]">
            <PreviewAvatar name={currentUser} initials={currentUser.slice(0, 2)} />
          </button>
        </div>
      </header>

      {userSettingsOpen && <UserSettingsView currentUser={currentUser} onClose={() => setUserSettingsOpen(false)} onLogout={onLogout} />}

      {/* 2. DYNAMIC PAGE VIEW */}
      <div className="flex-1 flex flex-col overflow-hidden relative">
        {isInLobby ? (
          <HomeHubView
            currentUser={currentUser}
            servers={recentServerIds.flatMap(id => joinedServers.filter(server => server.id === id))}
            onSelectServer={(srvId) => {
              navigateWithHistory(false, "overview", srvId);
            }}
            onOpenDm={(name) => {
              setDmFriendName(name);
              navigateWithHistory(false, "dm", activeServerId);
            }}
            onQuickJoinVoice={(srvId, roomName) => {
              setSelectedVoiceRoom(roomName);
              navigateWithHistory(false, "voice", srvId);
            }}
          />
        ) : activeTab === "dm" ? (
          <DirectMessageView
            currentUser={currentUser}
            initialFriendName={dmFriendName}
            onNavigate={(tab) =>
              navigateWithHistory(false, tab, activeServerId)
            }
          />
        ) : (
          <>
            {activeTab === "overview" && (
              <OverviewView
                currentUser={currentUser}
                isCreator={isCreator}
                onNavigate={(tab) =>
                  navigateWithHistory(false, tab, activeServerId)
                }
                chatChannels={chatChannels}
                voiceRooms={voiceRooms}
                selectedChatChannel={selectedChatChannel}
                onSelectChatChannel={(chName) => {
                  setSelectedChatChannel(chName);
                  navigateWithHistory(false, "chat", activeServerId);
                }}
                selectedVoiceRoom={selectedVoiceRoom}
                onSelectVoiceRoom={(rName) => {
                  setSelectedVoiceRoom(rName);
                  navigateWithHistory(false, "voice", activeServerId);
                }}
                onOpenInviteModal={() => setInviteModalOpen(true)}
                onOpenSettingsModal={(menuIdx = 0) => {
                  setSettingsInitialMenu(menuIdx);
                  setSettingsModalOpen(true);
                }}
                onOpenCreateModal={(type) => setCreateChannelModal(type)}
              />
            )}
            {activeTab === "chat" && (
              <ChannelChatView
                currentUser={currentUser}
                onNavigate={(tab) =>
                  navigateWithHistory(false, tab, activeServerId)
                }
                selectedChatChannel={selectedChatChannel}
                onSelectChatChannel={(chName) => {
                  setSelectedChatChannel(chName);
                  navigateWithHistory(false, "chat", activeServerId);
                }}
              />
            )}
            {activeTab === "voice" && (
              <VoiceStageView
                currentUser={currentUser}
                onOpenInviteModal={() => setInviteModalOpen(true)}
                screenShares={screenShares}
                onShareStarted={stream => {
                  sharing.start(stream);
                  setViewedShareOwner(currentUser);
                  navigateWithHistory(false, "screen", activeServerId);
                }}
                onWatchShare={owner => {
                  setViewedShareOwner(owner);
                  navigateWithHistory(false, "screen", activeServerId);
                }}
                onStopSharing={sharing.stop}
                onLeaveVoice={sharing.stop}
                onNavigate={(tab) =>
                  navigateWithHistory(false, tab, activeServerId)
                }
                selectedVoiceRoom={selectedVoiceRoom}
                onSelectVoiceRoom={(rName) => {
                  setSelectedVoiceRoom(rName);
                  navigateWithHistory(false, "voice", activeServerId);
                }}
              />
            )}
            {activeTab === "screen" && (
              <ScreenShareView
                currentUser={currentUser}
                screenShares={screenShares}
                viewedOwner={viewedShareOwner}
                onWatchShare={setViewedShareOwner}
                onStopSharing={sharing.stop}
                onNavigate={(tab) =>
                  navigateWithHistory(false, tab, activeServerId)
                }
                selectedVoiceRoom={selectedVoiceRoom}
              />
            )}
          </>
        )}
      </div>

      {sharing.stream && (isInLobby || activeTab !== "screen" || viewedShareOwner !== currentUser) && <aside aria-label="Màn hình chia sẻ thu nhỏ" className="fixed bottom-9 right-4 z-40 w-64 max-w-[calc(100vw-32px)] shadow-xl">
        <ShareVideo compact share={{ owner: currentUser, stream: sharing.stream, isLocal: true }} onStop={sharing.stop} onOpen={() => {
          setViewedShareOwner(currentUser);
          navigateWithHistory(false, "screen", activeServerId);
        }} />
      </aside>}

      {/* POPUP 1: MỜI BẠN BÈ */}
      {inviteModalOpen && (
        <div
          className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setInviteModalOpen(false)}
        >
          <div
            className="w-full max-w-md rounded-2xl bg-[#1c172c] border border-white/15 shadow-2xl overflow-hidden flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-5 py-4 bg-[#120f19] border-b border-white/10 flex items-center justify-between">
              <h3 className="text-sm font-bold text-[#f4effa]">
                Mời bạn bè vào {currentServer.name}
              </h3>
              <button
                onClick={() => setInviteModalOpen(false)}
                className="p-1.5 rounded-lg bg-white/5 hover:bg-white/10 text-[#b7aabe]"
              >
                <X className="w-4 h-4 shrink-0" />
              </button>
            </div>

            <div className="p-5 space-y-4">
              <div className="relative">
                <Search className="w-3.5 h-3.5 absolute left-3.5 top-1/2 -translate-y-1/2 text-[#b7aabe]" />
                <input
                  type="text"
                  value={friendSearch}
                  onChange={(e) => setFriendSearch(e.target.value)}
                  placeholder="Tìm kiếm bạn bè..."
                  className="w-full bg-[#15121d] border border-white/10 rounded-xl pl-9 pr-4 py-2 text-xs text-[#f4effa] outline-none"
                />
              </div>

              <div className="max-h-56 overflow-y-auto space-y-2">
                {filteredFriends.map((fr) => {
                  const isInvited = invitedIds.includes(fr.id);
                  return (
                    <div
                      key={fr.id}
                      className="p-2.5 rounded-xl bg-[#15121d]/80 border border-white/5 flex items-center justify-between"
                    >
                      <div className="flex items-center gap-3">
                        <div className="relative shrink-0 w-9 h-9 rounded-full bg-[#261e3a] text-[#c8b2f1] text-xs font-bold grid place-items-center">
                          <PreviewAvatar name={fr.name} initials={fr.init} />
                        </div>
                        <div>
                          <div className="text-xs font-bold text-[#f4effa]">
                            {fr.name}
                          </div>
                          <div className="text-xs text-[#b7aabe]">
                            {fr.username}
                          </div>
                        </div>
                      </div>

                      <button
                        onClick={() => {
                          if (!isInvited) setInvitedIds([...invitedIds, fr.id]);
                        }}
                        className={`px-3.5 py-1.5 rounded-xl text-xs font-bold transition ${
                          isInvited
                            ? "bg-[#a2d9d3]/20 text-[#a2d9d3]"
                            : "bg-white/5 hover:bg-[#c8b2f1] text-[#f4effa] hover:text-[#271737] border border-[#c8b2f1]/40"
                        }`}
                      >
                        {isInvited ? "Đã mời ✓" : "Mời"}
                      </button>
                    </div>
                  );
                })}
              </div>

              <div className="pt-3 border-t border-white/10 space-y-2">
                <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  ĐƯỜNG DẪN MỜI CỦA MÁY CHỦ
                </label>
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    readOnly
                    value={currentServer.inviteLink}
                    className="flex-1 bg-[#15121d] border border-white/10 rounded-xl px-3 py-2 text-xs font-mono text-[#a2d9d3] outline-none"
                  />
                  <button
                    onClick={() => {
                      navigator.clipboard?.writeText(currentServer.inviteLink);
                      setCopiedInviteLink(true);
                      setTimeout(() => setCopiedInviteLink(false), 1800);
                    }}
                    className="px-4 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] text-xs font-bold shrink-0"
                  >
                    {copiedInviteLink ? "Đã chép" : "Sao chép"}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* POPUP 2: TẠO KÊNH / PHÒNG */}
      {createChannelModal && (
        <div
          className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setCreateChannelModal(null)}
        >
          <div
            className="w-full max-w-md rounded-2xl bg-[#1c172c] border border-white/15 shadow-2xl overflow-hidden"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-5 py-4 bg-[#120f19] border-b border-white/10 flex items-center justify-between">
              <h3 className="text-sm font-bold text-[#f4effa]">
                {createChannelModal === "chat"
                  ? "Tạo Kênh Văn Bản"
                  : "Tạo Phòng Đàm Thoại"}
              </h3>
              <button
                onClick={() => setCreateChannelModal(null)}
                className="p-1.5 rounded-lg bg-white/5 text-[#b7aabe]"
              >
                <X className="w-4 h-4 shrink-0" />
              </button>
            </div>

            <form
              onSubmit={handleCreateChannelOrRoom}
              className="p-5 space-y-4"
            >
              <div className="space-y-1.5">
                <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  TÊN {createChannelModal === "chat" ? "KÊNH" : "PHÒNG"}
                </label>
                <input
                  type="text"
                  required
                  value={newChannelName}
                  onChange={(e) => setNewChannelName(e.target.value)}
                  placeholder="Nhập tên..."
                  className="w-full bg-[#15121d] border border-white/10 rounded-xl px-3.5 py-2.5 text-xs text-[#f4effa] outline-none"
                />
              </div>

              <div className="space-y-1.5">
                <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  MÔ TẢ NGẮN
                </label>
                <input
                  type="text"
                  value={newChannelDesc}
                  onChange={(e) => setNewChannelDesc(e.target.value)}
                  placeholder="Mô tả..."
                  className="w-full bg-[#15121d] border border-white/10 rounded-xl px-3.5 py-2.5 text-xs text-[#f4effa] outline-none"
                />
              </div>

              <div className="flex items-center justify-between pt-2 border-t border-white/10">
                <button
                  type="button"
                  onClick={() => {
                    setCreateChannelModal(null);
                    setSettingsInitialMenu(2);
                    setSettingsModalOpen(true);
                  }}
                  className="text-xs font-semibold text-[#a2d9d3] hover:underline"
                >
                  Quản lý phân quyền →
                </button>
                <button
                  type="submit"
                  className="px-4 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] text-xs font-bold"
                >
                  Tạo ngay
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* POPUP 3: THÔNG BÁO / BẢO MẬT / RỜI MÁY CHỦ */}
      {quickPopupType && (
        <div
          className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setQuickPopupType(null)}
        >
          <div
            className="w-full max-w-md rounded-2xl bg-[#1c172c] border border-white/15 shadow-2xl overflow-hidden p-5 space-y-4 text-xs"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between border-b border-white/10 pb-3">
              <h3 className="text-sm font-bold text-[#f4effa]">
                {quickPopupType === "notifications" && "Cài đặt thông báo"}
                {quickPopupType === "privacy" && "Cài đặt bảo mật"}
                {quickPopupType === "leave" &&
                  `Rời khỏi ${currentServer.name}?`}
              </h3>
              <button onClick={() => setQuickPopupType(null)}>
                <X className="w-4 h-4 text-[#b7aabe]" />
              </button>
            </div>

            {quickPopupType === "leave" ? (
              <div className="space-y-4">
                <p className="text-[#b7aabe]">
                  Bạn có chắc chắn muốn rời khỏi kênh{" "}
                  <strong className="text-[#f4effa]">
                    {currentServer.name}
                  </strong>
                  ?
                </p>
                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => setQuickPopupType(null)}
                    className="px-4 py-2 rounded-xl bg-white/5 text-[#f4effa]"
                  >
                    Hủy
                  </button>
                  <button
                    onClick={() => {
                      setQuickPopupType(null);
                      setIsInLobby(true);
                    }}
                    className="px-4 py-2 rounded-xl bg-red-500 text-white font-bold"
                  >
                    Rời kênh
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-3">
                <label className="p-3 rounded-xl bg-[#15121d] border border-white/10 flex items-center justify-between">
                  <span>Bật thông báo tin nhắn mới</span>
                  <input
                    type="checkbox"
                    defaultChecked
                    className="accent-[#c8b2f1]"
                  />
                </label>
                <label className="p-3 rounded-xl bg-[#15121d] border border-white/10 flex items-center justify-between">
                  <span>Cho phép thành viên cùng kênh nhắn tin riêng</span>
                  <input
                    type="checkbox"
                    defaultChecked
                    className="accent-[#a2d9d3]"
                  />
                </label>
                <div className="flex justify-end">
                  <button
                    onClick={() => setQuickPopupType(null)}
                    className="px-4 py-2 rounded-xl bg-[#c8b2f1] text-[#271737] font-bold"
                  >
                    Lưu cài đặt
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* POPUP 4: HỒ SƠ MÁY CHỦ & QUẢN LÝ PHÂN QUYỀN */}
      {settingsModalOpen && (
        <div
          className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4 md:p-8"
          onClick={() => setSettingsModalOpen(false)}
        >
          <div
            className="w-full max-w-[1280px] h-[85vh] rounded-3xl bg-[#15121d] border border-white/15 shadow-2xl overflow-hidden flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="h-13 px-6 bg-[#120f19] border-b border-white/10 flex items-center justify-between shrink-0">
              <div className="flex items-center gap-2.5 text-xs">
                <span className="w-2.5 h-2.5 rounded-full bg-[#c8b2f1]" />
                <span className="font-bold text-[#f4effa]">
                  Cài đặt Kênh — {currentServer.name}
                </span>
              </div>
              <button
                onClick={() => setSettingsModalOpen(false)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-white/5 hover:bg-red-500/20 text-[#b7aabe] hover:text-red-300 text-xs font-semibold transition"
              >
                <span>Đóng</span>
                <X className="w-4 h-4 shrink-0" />
              </button>
            </div>

            <div className="flex-1 flex overflow-hidden">
              <AdminRolesView
                currentUser={currentUser}
                isCreator={isCreator}
                initialMenu={settingsInitialMenu}
                serverName={currentServer.name}
                serverDesc={currentServer.desc}
                serverCode={currentServer.serverCode}
                inviteLink={currentServer.inviteLink}
                onUpdateServerProfile={handleUpdateServerProfile}
                onOpenCreateChannel={() => {
                  setSettingsModalOpen(false);
                  setCreateChannelModal("chat");
                }}
                onNavigate={(tab) => {
                  setSettingsModalOpen(false);
                  navigateWithHistory(false, tab, activeServerId);
                }}
              />
            </div>
          </div>
        </div>
      )}

      {/* 5. DOWN-BAR (Đã xóa phần không gian làm việc, chỉ hiện icon mạng + số ping và số phiên bản) */}
      <footer className="h-7 bg-[#100d16] border-t border-white/10 px-6 flex items-center justify-end gap-4 text-xs text-[#b7aabe] shrink-0 z-30">
        <span className="text-[#a2d9d3] flex items-center gap-1 font-medium">
          <Wifi className="w-3 h-3 shrink-0" />
          <span>18ms</span>
        </span>
        <span>v2.4.0</span>
      </footer>
    </div>
  );
}
