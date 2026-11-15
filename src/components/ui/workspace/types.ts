export type WorkspaceTab = "overview" | "chat" | "dm" | "voice" | "screen";

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
  currentUser: string;
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
}
