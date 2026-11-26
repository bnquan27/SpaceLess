import { useEffect, useRef, useState } from "react";
import { User, Shield, MessageSquare, Bell, Mic, Monitor, Accessibility, Settings, Globe, LogOut, X, Search, LockKeyhole } from "lucide-react";
import { PreviewAvatar } from "./preview-images";
import { useBackend } from "@/lib/workspace-backend";

const groups = [
  { name: "Tài khoản", items: [
    { name: "Thông tin tài khoản", icon: User },
    { name: "Dữ liệu và bảo mật", icon: Shield },
    { name: "Quyền hạn tin nhắn", icon: MessageSquare },
    { name: "Thông báo", icon: Bell },
  ] },
  { name: "Trải nghiệm", items: [
    { name: "Giọng nói và video", icon: Mic },
    { name: "Hiển thị", icon: Monitor },
    { name: "Trợ năng", icon: Accessibility },
    { name: "Hệ thống", icon: Settings },
    { name: "Ngôn ngữ và thời gian", icon: Globe },
  ] },
];

export function UserSettingsView({ currentUser, onClose, onLogout }: {
  currentUser: string;
  onClose: () => void;
  onLogout?: () => void;
}) {
  const backend = useBackend();
  const [saving,setSaving] = useState(false);
  const [saved,setSaved] = useState(false);
  const [section, setSection] = useState("Thông tin tài khoản");
  const [search, setSearch] = useState("");
  const [locale, setLocale] = useState("vi-VN");
  const [timeZone, setTimeZone] = useState("Asia/Ho_Chi_Minh");
  const [hourCycle, setHourCycle] = useState("24");
  const dialogRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onClose(); }
      if (event.key !== "Tab") return;
      const controls = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input, select") || []).filter(element => element.getClientRects().length);
      const first = controls[0], last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    };
    document.addEventListener("keydown", keyboard);
    return () => { document.removeEventListener("keydown", keyboard); previous?.focus(); };
  }, [onClose]);

  const fieldClass = "mt-2 block w-full rounded-lg border border-white/15 bg-[#15121d] px-3 py-2.5 text-sm text-[#eee7f5]";
  const descriptions: Record<string, string> = {
    "Dữ liệu và bảo mật": "Quản lý thông tin cá nhân và quyền riêng tư của bạn.",
    "Quyền hạn tin nhắn": "Tin nhắn trực tiếp được gửi giữa bạn bè. Khi chặn hoặc hủy kết bạn, lịch sử chuyển sang chỉ đọc.",
    "Thông báo": "Quản lý thông báo tin nhắn và hoạt động trong kênh.",
    "Giọng nói và video": "Chọn thiết bị đầu vào, đầu ra và camera khi tham gia phòng thoại.",
    "Hiển thị": "Điều chỉnh cách hiển thị giao diện và nội dung trò chuyện.",
    "Trợ năng": "Tùy chọn hỗ trợ đọc, điều hướng bàn phím và chuyển động.",
    "Hệ thống": "Tùy chọn khởi động và hoạt động của ứng dụng trên thiết bị.",
  };

  return <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/70 p-0 sm:p-6 lg:p-10">
    <section ref={dialogRef} role="dialog" aria-modal="true" aria-label="Cài đặt người dùng" className="flex h-full max-h-[900px] w-full max-w-[1280px] overflow-hidden border border-white/10 bg-[#1b1722] text-[#f4effa] sm:rounded-2xl">
      <aside className="hidden w-64 shrink-0 flex-col overflow-y-auto bg-[#15121b] p-4 sm:flex">
        <button className="mb-5 flex items-center gap-3 rounded-lg p-2 text-left hover:bg-white/5" onClick={() => { setSection("Thông tin tài khoản"); setSearch(""); }}>
          <span className="relative h-11 w-11 shrink-0 rounded-full bg-[#30243f]"><PreviewAvatar name={currentUser} initials={currentUser.slice(0, 2)} /></span>
          <span className="min-w-0"><strong className="block truncate text-sm">{currentUser}</strong><span className="mt-1 block text-xs text-[#b7aabe]">Hồ sơ người dùng</span></span>
        </button>
        <label className="relative mb-3 block"><Search size={16} className="absolute left-3 top-3 text-[#b7aabe]"/><span className="sr-only">Tìm cài đặt</span><input value={search} onChange={event => setSearch(event.target.value)} placeholder="Tìm cài đặt" className="w-full rounded-lg border border-white/10 bg-[#100e16] py-2.5 pl-9 pr-3 text-sm placeholder:text-[#b7aabe]" /></label>
        <nav aria-label="Danh mục cài đặt" className="space-y-4">
          {groups.map(group => <div key={group.name}>
            <p className="mb-2 px-2 text-xs font-medium text-[#a99bb7]">{group.name}</p>
            <div className="space-y-1">{group.items.filter(item => item.name.toLocaleLowerCase("vi").includes(search.toLocaleLowerCase("vi"))).map(({ name, icon: Icon }) => <button key={name} aria-current={section === name ? "page" : undefined} onClick={() => setSection(name)} className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm ${section === name ? "bg-[#3a2e49] font-medium text-[#eee3ff]" : "text-[#c4b8ce] hover:bg-white/5"}`}><Icon size={18} className="shrink-0"/>{name}</button>)}</div>
          </div>)}
        </nav>
        {onLogout && <button onClick={onLogout} className="mt-5 flex items-center gap-3 border-t border-white/10 px-3 py-4 text-sm text-[#ffa8b5] hover:bg-[#542333]/30"><LogOut size={18}/>Đăng xuất</button>}
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex min-h-14 shrink-0 items-center justify-between gap-4 border-b border-white/10 px-5 sm:px-6"><span className="text-sm font-semibold">{section}</span><button ref={closeRef} onClick={onClose} aria-label="Đóng cài đặt" className="workspace-icon-button rounded-lg text-[#b7aabe] hover:bg-white/10 hover:text-white"><X size={20}/></button></header>
        <div className="border-b border-white/10 p-4 sm:hidden">
          <label className="text-xs text-[#b7aabe]">Danh mục cài đặt<select aria-label="Danh mục cài đặt" className={fieldClass} value={section} onChange={event => setSection(event.target.value)}>{groups.map(group => <optgroup key={group.name} label={group.name}>{group.items.map(item => <option key={item.name}>{item.name}</option>)}</optgroup>)}</select></label>
        </div>
        <main className="min-h-0 flex-1 overflow-y-auto px-5 py-8 sm:px-10 lg:px-16 lg:py-12">
          <div className="mx-auto max-w-2xl">
            <h1 className="text-2xl font-semibold tracking-tight">{section}</h1>
            <p className="mt-2 text-sm text-[#b7aabe]">{backend ? 'Hồ sơ được tải từ máy chủ' : 'Bản xem thử cài đặt người dùng'}</p>
            {section === "Thông tin tài khoản" ? <>
              <div className="mt-8 divide-y divide-white/10">
                {[["Tên hiển thị", currentUser], ["Tên đăng nhập", backend?.data.profile.username || "Chưa có thông tin"], ["Email", backend?.data.profile.email || "Chưa có thông tin"], ["Số điện thoại", backend?.data.profile.phone || "Chưa có thông tin"]].map(([label, value]) => <div key={label} className="flex flex-wrap items-center justify-between gap-3 py-5"><span className="text-sm font-medium">{label}</span><span className="text-sm text-[#c4b8ce]">{value}</span></div>)}
              </div>
              {backend && <form className="mt-8 space-y-4 border-t border-white/10 pt-6" onSubmit={async event => {event.preventDefault();if(saving)return;setSaving(true);setSaved(false);const values=Object.fromEntries(new FormData(event.currentTarget));try{await backend.api.request('/users/me','PATCH',values);await backend.refresh();setSaved(true);}catch(error){backend.report(error);}finally{setSaving(false);}}}><h2 className="text-lg font-semibold">Chỉnh sửa hồ sơ</h2><fieldset disabled={saving} className="space-y-4">{[{name:'full_name',label:'Họ và tên',type:'text',value:backend.data.profile.full_name},{name:'email',label:'Email mới',type:'email',value:backend.data.profile.email},{name:'phone',label:'Số điện thoại mới',type:'tel',value:backend.data.profile.phone}].map(field => <label className="block text-sm" key={field.name}>{field.label}<input name={field.name} type={field.type} required defaultValue={field.value} maxLength={100} className={fieldClass}/></label>)}</fieldset><button disabled={saving} className="rounded-xl bg-[#c8b2f1] px-4 py-2 text-sm font-semibold text-[#271737]">{saving?'Đang lưu…':'Lưu hồ sơ'}</button>{saved&&<p role="status" className="text-sm text-[#a2d9d3]">Đã lưu hồ sơ trên máy chủ.</p>}</form>}
              <section className="mt-8 border-t border-white/10 pt-8"><h2 className="flex items-center gap-2 text-xl font-semibold"><LockKeyhole size={20}/>Mật khẩu và bảo mật</h2><div className="mt-5 flex items-center justify-between gap-4"><span className="text-sm">Mật khẩu</span><button disabled title="Chưa kết nối cập nhật mật khẩu" className="rounded-lg bg-white/10 px-4 py-2 text-sm disabled:cursor-default disabled:opacity-50">Thay đổi mật khẩu</button></div></section>
            </> : section === "Ngôn ngữ và thời gian" ? <div className="mt-8 space-y-6">
              <p className="text-sm leading-6 text-[#c4b8ce]">Các lựa chọn dưới đây áp dụng cho mẫu ngày giờ trong trang này.</p>
              <label className="block text-sm font-medium">Ngôn ngữ<select aria-label="Ngôn ngữ" value={locale} onChange={event => setLocale(event.target.value)} className={fieldClass}><option value="vi-VN">Tiếng Việt</option><option value="en-US">English</option></select></label>
              <label className="block text-sm font-medium">Múi giờ<select aria-label="Múi giờ" value={timeZone} onChange={event => setTimeZone(event.target.value)} className={fieldClass}><option value="Asia/Ho_Chi_Minh">Việt Nam (UTC+07:00)</option><option value="UTC">UTC (UTC+00:00)</option><option value="Asia/Tokyo">Tokyo (UTC+09:00)</option></select></label>
              <label className="block text-sm font-medium">Định dạng giờ<select aria-label="Định dạng giờ" value={hourCycle} onChange={event => setHourCycle(event.target.value)} className={fieldClass}><option value="24">24 giờ</option><option value="12">12 giờ</option></select></label>
              <div className="rounded-xl border border-white/10 bg-[#15121d] p-4"><p className="text-xs text-[#b7aabe]">Xem trước ngày và giờ</p><p role="status" className="mt-2 text-base">{new Intl.DateTimeFormat(locale, { dateStyle: "long", timeStyle: "short", timeZone, hour12: hourCycle === "12" }).format(new Date())}</p></div>
            </div> : <div className="mt-8 space-y-5">
              <p className="text-sm leading-6 text-[#d1c7d9]">{descriptions[section]}</p>
              <div className="rounded-xl border border-white/10 bg-[#15121d] p-5 text-sm leading-6 text-[#b7aabe]">Các tùy chọn sẽ khả dụng khi tính năng này được kết nối.</div>
            </div>}
            {onLogout && <button onClick={onLogout} className="mt-8 flex items-center gap-2 rounded-lg border border-[#ffa8b5]/30 px-4 py-3 text-sm text-[#ffa8b5] hover:bg-[#542333]/30 sm:hidden"><LogOut size={18}/>Đăng xuất</button>}
          </div>
        </main>
      </div>
    </section>
  </div>;
}
