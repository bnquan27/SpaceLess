import { useState } from 'react'
import { ArrowDownToLine, ArrowLeft, ArrowRight, Check, ChevronRight, Maximize2, Monitor, Smartphone, X } from 'lucide-react'
import { SpaceLessMark, SpaceLogin, type DesignDirection } from '@/components/ui/space-login'

const STORAGE_KEY = 'spaceless.design-direction.v2'
export const directions = [
  { id: 'orbit', name: 'Orbit', label: 'Gần nhau giữa vũ trụ', description: 'Sắc tím chiều sâu, quỹ đạo mềm và hai nửa cân bằng. Một lời chào ấm áp giữa không gian rộng lớn.', tone: 'Ấm áp · Có chiều sâu', layout: 'Chia đôi', colors: ['#171327', '#8870e9', '#e3dcff'], detail: 'Kính mờ nhẹ, quỹ đạo tím, điểm nhấn phi hành gia.', fit: 'Cân bằng cá tính thương hiệu và sự tập trung khi đăng nhập.' },
  { id: 'deep-space', name: 'Deep Space', label: 'Giữ tinh thần bản gốc', description: 'Bầu trời đêm, ánh sáng chân trời và form ở trung tâm. Gần nhất với mẫu không gian bạn cung cấp.', tone: 'Đắm chìm · Tĩnh lặng', layout: 'Trung tâm', colors: ['#08131c', '#83d8d0', '#dc9e7c'], detail: 'Form kính mờ nổi trên nền sao và ánh sáng xanh cam.', fit: 'Dành cho hướng không gian rõ nét, ít phân tán thao tác.' },
  { id: 'signal', name: 'Signal', label: 'Cùng chung một tần số', description: 'Xanh rừng và xanh chanh. Những nhịp âm thanh và mẩu hội thoại đưa kết nối thật lên phía trước.', tone: 'Sống động · Trực tiếp', layout: 'Đảo chiều', colors: ['#13231e', '#d4ed9a', '#607b67'], detail: 'Typography mạnh, tin nhắn và nhịp âm thanh làm chủ đạo.', fit: 'Nhấn vào trò chuyện, phòng thoại và cảm giác có nhau.' },
  { id: 'daylight', name: 'Daylight', label: 'Một khoảng trời trong', description: 'Nền giấy sáng, sắc đất dịu và khoảng thở rộng. Một cách tiếp cận thân thiện cho kết nối mỗi ngày.', tone: 'Nhẹ nhàng · Gần gũi', layout: 'Chia đôi sáng', colors: ['#f5f2eb', '#a14b35', '#d8b7a1'], detail: 'Nền sáng ấm, hội thoại tinh gọn, tương phản dễ đọc.', fit: 'Phù hợp không gian ban ngày và người dùng thích giao diện sáng.' },
] as const

function readSaved(): DesignDirection | null {
  try { const value = localStorage.getItem(STORAGE_KEY); return directions.some(d => d.id === value) ? value as DesignDirection : null } catch { return null }
}

export function SignInExplorer() {
  const [saved, setSaved] = useState<DesignDirection | null>(readSaved)
  const [active, setActive] = useState<DesignDirection>(() => readSaved() ?? 'orbit')
  const [viewport, setViewport] = useState<'desktop' | 'mobile'>('desktop')
  const [expanded, setExpanded] = useState(false)
  const [status, setStatus] = useState('')
  const current = directions.find(d => d.id === active)!
  const index = directions.findIndex(d => d.id === active)

  function select(id: DesignDirection) { setActive(id); setStatus('') }
  function save() {
    try { localStorage.setItem(STORAGE_KEY, active); setSaved(active); setStatus(`Đã lưu hướng ${current.name} trên thiết bị này.`) }
    catch { setStatus('Trình duyệt không cho phép lưu. Bạn có thể tải ghi chú lựa chọn.') }
  }
  function download() {
    const file = new Blob([`# SpaceLessApp — Hướng thiết kế ${current.name}\n\n${current.label}\n\n${current.description}\n\n- Bố cục: ${current.layout}\n- Sắc thái: ${current.tone}\n- Bảng màu: ${current.colors.join(', ')}\n- Chi tiết: ${current.detail}\n- Lý do chọn: ${current.fit}\n\nStack: React + TypeScript, Rust + Tauri, LiveKit.\nĐăng nhập bằng username + password theo docs/Dac_ta_he_thong_MVP_v1.docx.\n\nĐây là ghi chú lựa chọn thiết kế, chưa phải chức năng xác thực.\n`], { type: 'text/markdown;charset=utf-8' })
    const url = URL.createObjectURL(file); const link = document.createElement('a'); link.href = url; link.download = `spaceless-${active}.md`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000)
    setStatus('Đã tải ghi chú hướng thiết kế.')
  }

  return <div className="studio">
    <header className="studio-header"><a className="studio-brand" href="/" aria-label="SpaceLessApp — Trang chọn thiết kế"><div><SpaceLessMark /></div><strong>SpaceLess<span>App</span></strong></a><span className="header-divider" /><span className="studio-title">Phòng thiết kế</span><span className="prototype-badge"><i /> Bản thử nghiệm</span><button className="download-button" onClick={download}><ArrowDownToLine size={16} /><span>Tải ghi chú</span></button></header>
    <div className="studio-body">
      <aside className="directions-sidebar"><div className="sidebar-heading"><h1>Chọn một<br />cách kết nối.</h1><p>Bốn góc nhìn cho trang đăng nhập.<br />Một tinh thần SpaceLess.</p></div>
        <div className="direction-list" role="group" aria-label="Hướng thiết kế">{directions.map((direction, i) => <button key={direction.id} className={`direction-option ${active === direction.id ? 'is-active' : ''}`} onClick={() => select(direction.id)} aria-pressed={active === direction.id}>
          <div className={`direction-thumbnail thumb-${direction.id}`} aria-hidden="true"><div className="thumb-planet" /><div className="thumb-form"><i /><i /><i /></div><span className="thumb-dot" /></div>
          <div className="direction-option-copy"><span className="option-name">{direction.name}{saved === direction.id && <Check size={13} aria-label="Đã lưu" />}</span><span className="option-subtitle">{direction.label}</span></div><span className="option-index">{String(i + 1).padStart(2, '0')}</span>
        </button>)}</div>
        <div className="reference-note"><span className="reference-line" /><p>Biến tấu từ mẫu Space Login</p><span>Giữ cảm hứng. Khám phá góc nhìn mới.</span></div>
        <div className="sidebar-footer"><SpaceLessMark /><span>Ít khoảng cách hơn.<br />Nhiều câu chuyện hơn.</span></div>
      </aside>
      <main className="workspace">
        <div className="workspace-intro"><div className="breadcrumb">Giao diện <ChevronRight size={12} /> Xác thực <ChevronRight size={12} /><span>Đăng nhập</span></div><p>Chọn cảm giác. Thử tương tác. Tìm hướng đi.</p></div>
        <div className={`preview-shell ${expanded ? 'is-expanded' : ''}`} onKeyDown={e => { if (e.key === 'Escape') setExpanded(false) }}>
          <div className="preview-toolbar"><div className="preview-caption"><span className="live-dot" />Xem trước trực tiếp</div><div className="viewport-controls" role="group" aria-label="Kích thước xem trước"><button aria-label="Xem trên desktop" aria-pressed={viewport === 'desktop'} onClick={() => setViewport('desktop')}><Monitor size={15} /><span>Desktop</span></button><button aria-label="Xem trên điện thoại" aria-pressed={viewport === 'mobile'} onClick={() => setViewport('mobile')}><Smartphone size={14} /><span>Mobile</span></button></div><button className="expand-button" onClick={() => setExpanded(v => !v)} aria-label={expanded ? 'Thu nhỏ xem trước' : 'Mở rộng xem trước'}>{expanded ? <X size={16} /> : <Maximize2 size={15} />}</button></div>
          <div className={`preview-stage ${viewport === 'mobile' ? 'mobile-stage' : ''}`}><div className="preview-viewport"><SpaceLogin direction={active} /></div></div>
          <div className="preview-bottom-bar"><span><i /> {current.name} <span className="preview-layout">/ {current.layout}</span></span><span>Form tương tác · Chưa kết nối máy chủ</span></div>
        </div>
        <div className="direction-details"><div className="direction-summary"><div className="direction-title-row"><h2>{current.name}</h2><span>{current.tone}</span></div><p>{current.description}</p></div><div className="palette"><div>{current.colors.map(color => <span key={color} style={{ background: color }} title={color} />)}</div><span>Bảng màu chủ đạo</span></div><button className={`choose-button ${saved === active ? 'is-saved' : ''}`} onClick={save}>{saved === active ? <><Check size={16} /> Đã chọn hướng này</> : <>Chọn hướng này <ArrowRight size={16} /></>}</button></div>
        <div className="workspace-footer"><span role="status">{status || current.fit}</span><div className="direction-navigation"><button onClick={() => select(directions[(index + 3) % 4].id)} aria-label="Hướng thiết kế trước"><ArrowLeft size={15} /></button><span>{index + 1} / {directions.length}</span><button onClick={() => select(directions[(index + 1) % 4].id)} aria-label="Hướng thiết kế tiếp theo"><ArrowRight size={15} /></button></div></div>
      </main>
    </div>
  </div>
}
