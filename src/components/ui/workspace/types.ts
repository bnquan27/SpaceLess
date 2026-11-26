export type WorkspaceTab = "overview" | "chat" | "dm" | "voice" | "screen";

export interface ScreenShareSession {
  owner: string;
  stream: MediaStream;
  isLocal: boolean;
}

export interface ChatChannelItem {
  id: string;
  name: string;
  sub: string;
  tag?: string;
  badge?: string;
  dot?: boolean;
}

export interface VoiceRoomItem {
  id: string;
  name: string;
  sub: string;
  quality?: string;
  locked?: boolean;
  isLive?: boolean;
}

export interface ViewProps {
  roomId?: string;
  channelId?: string;
  currentUser: string;
  initialFriendName?: string;
  onNavigate: (tab: WorkspaceTab) => void;
  isCreator?: boolean;
  selectedChatChannel?: string;
  onSelectChatChannel?: (channelName: string) => void;
  selectedVoiceRoom?: string;
  onSelectVoiceRoom?: (roomName: string) => void;
  chatChannels?: ChatChannelItem[];
  voiceRooms?: VoiceRoomItem[];
  onOpenInviteModal?: () => void;
  onOpenSettingsModal?: (initialMenu?: number) => void;
  onOpenCreateModal?: (type: "chat" | "voice") => void;
  screenShares?: ScreenShareSession[];
  onShareStarted?: (stream: MediaStream) => void;
  onWatchShare?: (owner: string) => void;
  onStopSharing?: () => void;
  onLeaveVoice?: () => void;
}
