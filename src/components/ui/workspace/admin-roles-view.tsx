import React, { useState, useEffect } from "react";
import { PreviewAvatar } from "./preview-images";
import {
  Sliders,
  FolderGit2,
  Users,
  Shield,
  Check,
  X,
  Plus,
  Hash,
  Copy,
  Cloud,
  Volume2,
  Code,
  Eye,
  SlidersHorizontal,
  Layers,
  Lock,
  Save,
} from "lucide-react";
import { ViewProps } from "./types";

interface AdminExtendedProps extends ViewProps {
  initialMenu?: number;
  serverName?: string;
  serverDesc?: string;
  serverCode?: string;
  inviteLink?: string;
  onUpdateServerProfile?: (name: string, desc: string) => void;
  onOpenCreateChannel?: () => void;
}

export const AdminRolesView: React.FC<AdminExtendedProps> = ({
  currentUser,
  isCreator = true,
  initialMenu = 0,
  serverName = "Nhóm Đồ Án - UI/UX",
  serverDesc = "Nghiên cứu kiến trúc hệ thống, wireframe prototype đa nền tảng cho đồ án tốt nghiệp.",
  serverCode = "WS-8849-VN",
  inviteLink = "https://meet.space.vn/inv/ui-ux-k21-992a",
  onUpdateServerProfile,
  onOpenCreateChannel,
}) => {
  // Chỉ còn 3 menu: 0 (Hồ sơ), 1 (Quản lý Kênh & Phòng), 2 (Thành viên & Phân quyền)
  const [activeMenu, setActiveMenu] = useState<number>(
    initialMenu > 2 ? 0 : initialMenu,
  );
  const [editName, setEditName] = useState(serverName);
  const [editDesc, setEditDesc] = useState(serverDesc);
  const [savedToast, setSavedToast] = useState(false);
  const [copiedText, setCopiedText] = useState<string | null>(null);

  useEffect(() => {
    setActiveMenu(initialMenu > 2 ? 0 : initialMenu);
  }, [initialMenu]);

  useEffect(() => {
    setEditName(serverName);
    setEditDesc(serverDesc);
  }, [serverName, serverDesc]);

  const [channels] = useState([
    {
      id: "c1",
      name: "chung",
      badge: "CHÍNH",
      badgeType: "live",
      sub: "Sảnh trò chuyện toàn nhóm",
      access: "Tất cả (18)",
      accessStyle: "bg-white/5 text-[#f4effa] border-white/10",
    },
    {
      id: "c2",
      name: "thông-báo-nội-bộ",
      icon: "megaphone",
      sub: "Chỉ quản trị viên đăng tin",
      access: "Chỉ đọc",
      accessStyle: "bg-white/5 text-[#b7aabe] border-white/10",
    },
    {
      id: "c3",
      name: "thiết-kế-ui-ux",
      badge: "Figma",
      badgeType: "sync",
      sub: "Đánh giá hệ thống giao diện",
      access: "Designers (6)",
      accessStyle: "bg-[#a2d9d3]/15 text-[#a2d9d3] border-[#a2d9d3]/30",
    },
    {
      id: "c4",
      name: "kỹ-thuật-dev",
      icon: "code",
      sub: "Phát triển Backend & Frontend",
      access: "Devs (8)",
      accessStyle: "bg-[#a2d9d3]/15 text-[#a2d9d3] border-[#a2d9d3]/30",
    },
  ]);

  const handleSaveProfile = () => {
    if (!isCreator) return;
    onUpdateServerProfile?.(editName, editDesc);
    setSavedToast(true);
    setTimeout(() => setSavedToast(false), 2000);
  };

  const handleCopy = (text: string, tag: string) => {
    navigator.clipboard?.writeText(text);
    setCopiedText(tag);
    setTimeout(() => setCopiedText(null), 1800);
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col sm:flex-row overflow-hidden">
      {/* CỘT MENU QUẢN TRỊ BÊN TRÁI (Đã bỏ Bảo mật & WebRTC và Tích hợp API) */}
      <aside className="w-full sm:w-52 lg:w-64 bg-[#120f19] border-b sm:border-b-0 sm:border-r border-white/10 p-3 sm:p-5 flex flex-col justify-between shrink-0">
        <div className="space-y-2">
          <div className="text-xs font-bold uppercase tracking-wider text-[#b7aabe] px-2 mb-3">
            CÀI ĐẶT MÁY CHỦ
          </div>
          {[
            { icon: Sliders, label: "Hồ sơ & Cài đặt chung" },
            { icon: FolderGit2, label: "Quản lý Kênh & Phòng" },
            { icon: Users, label: "Thành viên & Phân quyền" },
          ].map((item, idx) => {
            const Icon = item.icon;
            const isSelected = activeMenu === idx;
            return (
              <button
                key={idx}
                onClick={() => setActiveMenu(idx)}
                aria-pressed={isSelected}
                className={`workspace-action w-full px-3.5 py-2.5 rounded-xl text-left flex items-center gap-3 transition ${
                  isSelected
                    ? "bg-[#c8b2f1] text-[#271737] font-bold shadow"
                    : "text-[#b7aabe] hover:bg-white/5 hover:text-[#f4effa]"
                }`}
              >
                <Icon className="w-4 h-4 shrink-0" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>
      </aside>

      {/* KHU VỰC NỘI DUNG CHÍNH */}
      <div className="flex-1 min-w-0 overflow-y-auto px-4 lg:px-8 py-4 sm:py-6">
        {!isCreator && (
          <div className="max-w-[1250px] mx-auto mb-5 px-4 py-3 rounded-xl bg-amber-500/10 border border-amber-400/30 flex items-center justify-between text-xs text-amber-200">
            <span className="flex items-center gap-2">
              <Lock className="w-4 h-4 text-amber-300 shrink-0" />
              Bạn đang xem với tư cách <strong>Thành viên</strong>. Chỉ{" "}
              <strong>Chủ kênh (Creator)</strong> mới có thể sửa Hồ sơ máy chủ
              hoặc thêm kênh/phòng.
            </span>
          </div>
        )}

        {/* MỤC 0 & 1: HỒ SƠ MÁY CHỦ & QUẢN LÝ KÊNH PHÒNG (Đã bỏ cột Mã hóa E2E) */}
        {(activeMenu === 0 || activeMenu === 1) && (
          <div className="max-w-[1250px] mx-auto space-y-6">
            <div>
              <h2 className="text-2xl font-bold text-[#f4effa]">
                Cài đặt Không gian làm việc
              </h2>
              <p className="text-xs text-[#b7aabe] mt-1">
                Quản lý thông tin máy chủ và danh sách các kênh trong{" "}
                <strong className="text-[#f4effa]">{editName}</strong>.
              </p>
            </div>

            <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
              {/* CỘT TRÁI (5/12): Hồ sơ Workspace */}
              <div className="xl:col-span-5 space-y-5">
                <div className="p-6 rounded-2xl bg-[#1c172c] border border-white/10 space-y-5 shadow-lg">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <div className="w-9 h-9 rounded-xl bg-[#a2d9d3]/15 border border-[#a2d9d3]/30 text-[#a2d9d3] grid place-items-center">
                        <Layers className="w-4 h-4 shrink-0" />
                      </div>
                      <h3 className="text-base font-bold text-[#f4effa]">
                        Hồ sơ Máy chủ
                      </h3>
                    </div>
                    <span className="px-2.5 py-1 rounded-lg bg-[#15121d] border border-white/10 text-xs font-mono text-[#b7aabe]">
                      ID: {serverCode}
                    </span>
                  </div>

                  <div className="space-y-1.5">
                    <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                      TÊN MÁY CHỦ
                    </label>
                    <input
                      type="text"
                      disabled={!isCreator}
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      className={`w-full bg-[#15121d] border border-white/10 rounded-xl px-3.5 py-2 text-xs font-semibold text-[#f4effa] outline-none ${
                        isCreator
                          ? "focus:border-[#c8b2f1]/50"
                          : "opacity-60 cursor-not-allowed"
                      }`}
                    />
                  </div>

                  <div className="space-y-2">
                    <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                      GIỚI THIỆU MÁY CHỦ
                    </label>
                    <textarea
                      rows={3}
                      disabled={!isCreator}
                      value={editDesc}
                      onChange={(e) => setEditDesc(e.target.value)}
                      className={`w-full bg-[#15121d] border border-white/10 rounded-xl p-3.5 text-xs text-[#f4effa]/90 leading-relaxed outline-none resize-none ${
                        isCreator
                          ? "focus:border-[#c8b2f1]/50"
                          : "opacity-60 cursor-not-allowed"
                      }`}
                    />
                  </div>

                  {isCreator && (
                    <div className="flex items-center justify-between pt-1">
                      {savedToast ? (
                        <span className="text-xs font-semibold text-[#a2d9d3] flex items-center gap-1">
                          <Check className="w-4 h-4 shrink-0" /> Đã lưu thay
                          đổi!
                        </span>
                      ) : (
                        <span className="text-xs text-[#b7aabe]">
                          Cập nhật tên máy chủ trực tiếp
                        </span>
                      )}
                      <button
                        type="button"
                        onClick={handleSaveProfile}
                        className="px-4 py-2 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] text-xs font-bold flex items-center gap-1.5 transition shadow"
                      >
                        <Save className="w-3.5 h-3.5 shrink-0" />
                        <span>Lưu Hồ sơ</span>
                      </button>
                    </div>
                  )}

                  <div className="p-4 rounded-xl bg-[#15121d]/80 border border-white/10 space-y-2">
                    <label className="block text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                      ĐƯỜNG DẪN MỜI THÀNH VIÊN
                    </label>
                    <div className="flex items-center gap-2">
                      <div className="flex-1 bg-[#1c172c] border border-white/10 rounded-xl px-3 py-2 text-xs font-mono text-[#a2d9d3] truncate">
                        {inviteLink}
                      </div>
                      <button
                        type="button"
                        onClick={() => handleCopy(inviteLink, "invite")}
                        className="p-2.5 rounded-xl bg-[#1c172c] hover:bg-[#c8b2f1] text-[#b7aabe] hover:text-[#271737] border border-white/10 transition shrink-0"
                      >
                        {copiedText === "invite" ? (
                          <Check className="w-4 h-4 shrink-0" />
                        ) : (
                          <Copy className="w-4 h-4 shrink-0" />
                        )}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="p-5 rounded-2xl bg-[#1c172c] border border-white/10 space-y-3 shadow-lg">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-bold text-[#f4effa] flex items-center gap-2">
                      <Cloud className="w-4 h-4 text-[#a2d9d3] shrink-0" />
                      Dung lượng Lưu trữ Nhóm
                    </span>
                    <span className="text-xs font-semibold text-[#f4effa]">
                      14.2 GB / 50 GB
                    </span>
                  </div>
                  <div className="w-full h-2 rounded-full bg-[#15121d] overflow-hidden flex">
                    <div className="w-[35%] bg-[#c8b2f1] h-full" />
                  </div>
                </div>
              </div>

              {/* CỘT PHẢI (7/12): Bảng Quản lý Kênh & Phòng (Đã bỏ cột Mã hóa E2E) */}
              <div className="xl:col-span-7 p-6 rounded-2xl bg-[#1c172c] border border-white/10 space-y-5 shadow-lg">
                <div className="flex flex-wrap items-center justify-between gap-4">
                  <div className="flex items-center gap-3">
                    <div className="w-10 h-10 rounded-xl bg-[#a2d9d3]/15 border border-[#a2d9d3]/30 text-[#a2d9d3] grid place-items-center">
                      <FolderGit2 className="w-5 h-5 shrink-0" />
                    </div>
                    <div>
                      <h3 className="text-base font-bold text-[#f4effa]">
                        Quản lý Kênh & Phòng
                      </h3>
                      <p className="text-xs text-[#b7aabe] mt-0.5">
                        Danh sách các kênh văn bản và phòng đàm thoại trong máy
                        chủ.
                      </p>
                    </div>
                  </div>

                  {isCreator ? (
                    <button
                      onClick={() => onOpenCreateChannel?.()}
                      className="px-4 py-2.5 rounded-xl bg-[#f4effa] hover:bg-white text-[#15121d] text-xs font-bold flex items-center gap-1.5 transition shadow"
                    >
                      <Plus className="w-4 h-4 shrink-0" />
                      <span>Thêm kênh mới</span>
                    </button>
                  ) : (
                    <span className="px-3 py-2 rounded-xl bg-[#15121d] border border-white/10 text-xs text-[#b7aabe] flex items-center gap-1.5">
                      <Lock className="w-3.5 h-3.5 shrink-0" /> Chỉ Chủ kênh
                      được thêm
                    </span>
                  )}
                </div>

                {/* Bảng 3 Cột: KÊNH / KHÔNG GIAN | QUYỀN TRUY CẬP | TÁC VỤ */}
                <div className="grid grid-cols-12 gap-3 px-3 py-2 border-y border-white/10 text-xs font-bold uppercase tracking-wider text-[#b7aabe]">
                  <div className="col-span-6">KÊNH / KHÔNG GIAN</div>
                  <div className="col-span-4 text-center">QUYỀN TRUY CẬP</div>
                  <div className="col-span-2 text-right">TÁC VỤ</div>
                </div>

                <div className="divide-y divide-white/10">
                  {channels.map((ch) => (
                    <div
                      key={ch.id}
                      className="grid grid-cols-12 gap-3 items-center px-3 py-4 hover:bg-white/[0.02] transition"
                    >
                      <div className="col-span-6 flex items-center gap-3 min-w-0">
                        <div className="w-9 h-9 rounded-xl bg-[#15121d] border border-white/10 text-[#b7aabe] grid place-items-center shrink-0">
                          <Hash className="w-4 h-4 shrink-0" />
                        </div>
                        <div className="min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="text-xs font-bold text-[#f4effa] truncate">
                              {ch.name}
                            </span>
                            {ch.badge && (
                              <span className="text-xs px-2 py-0.5 rounded font-bold bg-[#a2d9d3]/20 text-[#a2d9d3]">
                                {ch.badge}
                              </span>
                            )}
                            {ch.icon === "megaphone" && (
                              <Volume2 className="w-3.5 h-3.5 text-[#b7aabe] shrink-0" />
                            )}
                            {ch.icon === "code" && (
                              <Code className="w-3.5 h-3.5 text-[#b7aabe] shrink-0" />
                            )}
                          </div>
                          <p className="text-xs text-[#b7aabe] truncate mt-0.5">
                            {ch.sub}
                          </p>
                        </div>
                      </div>

                      <div className="col-span-4 flex justify-center">
                        <span
                          className={`px-3 py-1 rounded-lg text-xs font-semibold border ${ch.accessStyle}`}
                        >
                          {ch.access}
                        </span>
                      </div>

                      <div className="col-span-2 flex items-center justify-end gap-2 text-[#b7aabe]">
                        <button
                          disabled={!isCreator}
                          className="p-1.5 rounded-lg hover:bg-white/10 hover:text-[#f4effa] disabled:opacity-40"
                        >
                          <Eye className="w-4 h-4 shrink-0" />
                        </button>
                        <button
                          disabled={!isCreator}
                          className="p-1.5 rounded-lg hover:bg-white/10 hover:text-[#f4effa] disabled:opacity-40"
                        >
                          <SlidersHorizontal className="w-4 h-4 shrink-0" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* MỤC 2: THÀNH VIÊN & MA TRẬN PHÂN QUYỀN */}
        {activeMenu === 2 && (
          <div className="max-w-[1250px] mx-auto grid grid-cols-1 xl:grid-cols-3 gap-6">
            <div className="xl:col-span-2 space-y-5">
              <div>
                <h2 className="text-2xl font-bold text-[#f4effa]">
                  Thành viên & Phân quyền
                </h2>
                <p className="text-xs text-[#b7aabe] mt-1">
                  Quản lý danh sách thành viên và vai trò trong máy chủ.
                </p>
              </div>

              <div className="rounded-2xl bg-[#1c172c] border border-white/10 overflow-hidden shadow-lg">
                <div className="px-5 py-4 border-b border-white/10 flex items-center justify-between">
                  <span className="text-xs font-bold uppercase tracking-wider text-[#f4effa]">
                    Danh sách thành viên
                  </span>
                  {isCreator && (
                    <button className="px-3.5 py-1.5 rounded-xl bg-[#c8b2f1] hover:bg-[#d8c7f7] text-[#271737] text-xs font-bold flex items-center gap-1.5 transition">
                      <Plus className="w-3.5 h-3.5 shrink-0" /> Cấp quyền mới
                    </button>
                  )}
                </div>

                <div className="divide-y divide-white/10">
                  {[
                    {
                      init: "TĐ",
                      name: "Trần Văn Tuấn Đình",
                      role: "Creator",
                      isOwnerRole: true,
                    },
                    {
                      init: "TA",
                      name: "Phạm Tuấn Anh",
                      role: "Moderator",
                      isOwnerRole: false,
                    },
                    {
                      init: "NQ",
                      name: "Bùi Nam Quân",
                      role: "Moderator",
                      isOwnerRole: false,
                    },
                    {
                      init: "NM",
                      name: `${currentUser} (Bạn)`,
                      role: isCreator ? "Creator" : "Member",
                      isOwnerRole: isCreator,
                    },
                  ].map((m, i) => (
                    <div
                      key={i}
                      className="px-5 py-4 flex items-center justify-between hover:bg-white/[0.02] transition"
                    >
                      <div className="flex items-center gap-3.5">
                        <div className="relative shrink-0 w-10 h-10 rounded-full bg-[#261e3a] border border-[#c8b2f1]/40 text-xs font-bold grid place-items-center text-[#c8b2f1]">
                          <PreviewAvatar name={m.name} initials={m.init} />
                        </div>
                        <div>
                          <div className="text-sm font-semibold text-[#f4effa]">
                            {m.name}
                          </div>
                        </div>
                      </div>
                      <span
                        className={`px-3 py-1 rounded-full text-xs font-semibold border ${
                          m.isOwnerRole
                            ? "bg-[#a2d9d3]/15 text-[#a2d9d3] border-[#a2d9d3]/30"
                            : "bg-[#c8b2f1]/15 text-[#c8b2f1] border-[#c8b2f1]/30"
                        }`}
                      >
                        {m.role}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>

            <div className="space-y-4">
              <h3 className="text-sm font-bold flex items-center gap-2 text-[#f4effa]">
                <Shield className="w-4 h-4 text-[#c8b2f1] shrink-0" /> Ma trận
                Phân quyền
              </h3>
              {[
                {
                  title: "Người sáng lập (Creator)",
                  badge: "Toàn quyền",
                  badgeColor: "text-[#a2d9d3] bg-[#a2d9d3]/15",
                  desc: "Người tạo máy chủ. Có toàn quyền sửa hồ sơ máy chủ, tạo/xóa kênh & phòng và phân quyền.",
                  allowed: [
                    "Sửa Hồ sơ Máy chủ",
                    "Tạo / Xóa Kênh & Phòng",
                    "Quản lý Phân quyền",
                    "Xóa thành viên",
                  ],
                  denied: [],
                },
                {
                  title: "Điều hành viên (Moderator)",
                  badge: "Trung cấp",
                  badgeColor: "text-[#c8b2f1] bg-[#c8b2f1]/15",
                  desc: "Quản lý tin nhắn trong kênh và điều phối micro trong phòng đàm thoại.",
                  allowed: ["Quản lý Mic Voice", "Xóa tin sai phạm"],
                  denied: ["Sửa Hồ sơ Máy chủ"],
                },
                {
                  title: "Thành viên (Member)",
                  badge: "Tiêu chuẩn",
                  badgeColor: "text-[#b7aabe] bg-white/5",
                  desc: "Tham gia nhắn tin, gửi tệp và trò chuyện trong phòng thoại.",
                  allowed: ["Gửi tin nhắn & tệp", "Tham gia Phòng thoại"],
                  denied: ["Tạo Kênh / Phòng", "Sửa Hồ sơ Máy chủ"],
                },
              ].map((r, i) => (
                <div
                  key={i}
                  className="p-5 rounded-2xl bg-[#1c172c] border border-white/10 space-y-3 shadow-md"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-bold text-xs text-[#f4effa]">
                      {r.title}
                    </span>
                    <span
                      className={`text-xs px-2.5 py-0.5 rounded-full font-semibold ${r.badgeColor}`}
                    >
                      {r.badge}
                    </span>
                  </div>
                  <p className="text-xs text-[#b7aabe] leading-relaxed">
                    {r.desc}
                  </p>
                  <div className="grid grid-cols-2 gap-2 pt-2.5 border-t border-white/10">
                    {r.allowed.map((p, idx) => (
                      <div
                        key={idx}
                        className="text-xs text-[#f4effa] flex items-center gap-1.5"
                      >
                        <Check className="w-3 h-3 text-[#a2d9d3] shrink-0" />{" "}
                        {p}
                      </div>
                    ))}
                    {r.denied.map((p, idx) => (
                      <div
                        key={idx}
                        className="text-xs text-[#b7aabe]/60 line-through flex items-center gap-1.5"
                      >
                        <X className="w-3 h-3 text-red-400/70 shrink-0" /> {p}
                      </div>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
