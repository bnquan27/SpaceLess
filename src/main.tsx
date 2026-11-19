import React, { useEffect, useState } from 'react'
import ReactDOM from 'react-dom/client'
import '@fontsource/be-vietnam-pro/400.css'
import '@fontsource/be-vietnam-pro/500.css'
import '@fontsource/be-vietnam-pro/600.css'
import '@fontsource/be-vietnam-pro/700.css'
import { SpaceLogin } from '@/components/ui/space-login'
import { SpaceWorkspace } from '@/components/ui/space-workspace'
import { authAvailability, cancelLogin, googleLogin, logout, passwordLogin, type AuthAvailability } from '@/lib/auth'
import './index.css'

type AppSession = { kind: 'signed-out' } | { kind: 'authenticated'; username: string; fullName: string } | { kind: 'preview' }

function AppRoot() {
  const [session, setSession] = useState<AppSession>({ kind: 'signed-out' })
  const [availability, setAvailability] = useState<AuthAvailability>({ password: false, google: false, message: 'Đang kiểm tra cấu hình đăng nhập…' })
  const [logoutError, setLogoutError] = useState('')
  const [loggingOut, setLoggingOut] = useState(false)

  useEffect(() => {
    authAvailability().then(setAvailability).catch(() => setAvailability({ password: false, google: false, message: 'Không thể đọc cấu hình. Vui lòng khởi động lại ứng dụng desktop.' }))
  }, [])

  async function signOut() {
    if (loggingOut) return
    setLoggingOut(true)
    try { if (session.kind === 'authenticated') await logout() }
    catch (error) { setLogoutError(typeof error === 'string' ? error : 'Đã xóa phiên trên thiết bị. Chưa thể xác nhận đăng xuất tại máy chủ.') }
    finally { setSession({ kind: 'signed-out' }); setLoggingOut(false) }
  }

  if (session.kind !== 'signed-out') return <div className="app-workspace-preview">
    <p className="workspace-preview-notice" role="status">{session.kind === 'authenticated' ? `Đã đăng nhập bằng tài khoản ${session.username}. Không gian bên dưới dùng dữ liệu mẫu; nhắn tin và cuộc gọi chưa được kết nối.` : 'Không gian mẫu. Bạn chưa đăng nhập; dữ liệu và tương tác chỉ để xem thử.'}{loggingOut && ' Đang đăng xuất…'}</p>
    <SpaceWorkspace currentUser={session.kind === 'authenticated' ? session.fullName : 'Người xem thử'} onLogout={signOut} />
  </div>

  return <main className="app-login">
    {logoutError && <p className="app-auth-notice" role="alert">{logoutError}</p>}
    <SpaceLogin direction="orbit"
      onSubmit={availability.password ? async ({ username, password }) => {
        const profile = await passwordLogin({ username, password })
        setLogoutError(''); setSession({ kind: 'authenticated', username: profile.username, fullName: profile.full_name })
      } : undefined}
      googleAuth={{ enabled: availability.google, message: availability.message, onLogin: async () => {
        const profile = await googleLogin()
        setLogoutError(''); setSession({ kind: 'authenticated', username: profile.username, fullName: profile.full_name })
      }, onCancel: cancelLogin }}
    />
    {!availability.password && <button type="button" className="open-workspace-preview" onClick={() => { setLogoutError(''); setSession({ kind: 'preview' }) }}>Mở không gian mẫu</button>}
  </main>
}

const rootElement = document.getElementById('root')
if (!rootElement) throw new Error('Không tìm thấy phần tử gốc của ứng dụng.')
const root = ReactDOM.createRoot(rootElement)

if (window.location.pathname === '/design') {
  import('@/components/sign-in-explorer').then(({ SignInExplorer }) => {
    root.render(<React.StrictMode><SignInExplorer /></React.StrictMode>)
  })
} else {
  root.render(<React.StrictMode><AppRoot /></React.StrictMode>)
}
