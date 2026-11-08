import { useEffect, useId, useRef, useState, type FormEvent } from 'react'
import { ArrowLeft, ArrowUpRight, Check, Eye, EyeOff, LoaderCircle, MessageCircle, Mic, MoveUpRight, Pause, Play } from 'lucide-react'
import { cn } from '@/lib/utils'

export type DesignDirection = 'orbit' | 'deep-space' | 'signal' | 'daylight'
export interface SpaceLoginProps {
  direction?: DesignDirection
  onSubmit?: (data: { username: string; password: string; rememberMe: boolean }) => Promise<void> | void
  className?: string
}

export function SpaceLessMark({ className }: { className?: string }) {
  return <svg className={className} viewBox="0 0 32 32" fill="none" aria-hidden="true"><path d="M6 19.5C.5 14 7.5 5 13 10.5l6 11C24.5 27 31.5 18 26 12.5L20 9l-8 14-6-3.5Z" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" /></svg>
}

function SpaceArtwork({ direction, paused }: { direction: DesignDirection; paused: boolean }) {
  const artwork = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(true)
  useEffect(() => {
    let intersects = true
    const update = () => setVisible(intersects && !document.hidden)
    const observer = new IntersectionObserver(([entry]) => { intersects = entry.isIntersecting; update() })
    if (artwork.current) observer.observe(artwork.current)
    document.addEventListener('visibilitychange', update)
    update()
    return () => { observer.disconnect(); document.removeEventListener('visibilitychange', update) }
  }, [])
  return <div ref={artwork} className="space-artwork" data-motion-playing={visible && !paused} aria-hidden="true">
    <div className="starfield">{Array.from({ length: direction === 'orbit' ? 160 : 42 }, (_, i) => { const x = Math.abs(Math.sin(i * 127.1 + 17) * 43758.5453) % 1; const y = Math.abs(Math.sin(i * 311.7 + 29) * 23421.631) % 1; const size = i % 17 === 0 ? 3 : i % 4 === 0 ? 2 : 1; return <i key={i} style={{ left: `${x * 100}%`, top: `${y * 96}%`, opacity: .3 + (i % 5) * .14, width: size, height: size }} /> })}</div>
    {direction === 'orbit' && <div className="meteor-field"><span className="meteor meteor-one" /><span className="meteor meteor-two" /></div>}
    <div className="orbit-line orbit-line-one" /><div className="orbit-line orbit-line-two" />
    <div className="planet" /><div className="planet-horizon" />
    <span className="art-cross cross-one">+</span><span className="art-cross cross-two">+</span>
    {(direction === 'orbit' || direction === 'deep-space') && <div className="astronaut-wrap"><img src="/assets/astronaut.png" alt="" className="astronaut" draggable={false} /><span className="astronaut-spark" /></div>}
    {(direction === 'signal' || direction === 'daylight') && <div className="conversation-art"><div className="message-line"><span className="message-avatar">L</span><div>Ở đây, cùng nhau.<span>Khoảng cách chỉ là một con số.</span></div></div><div className="voice-line"><Mic size={19} /><div className="waveform">{Array.from({ length: 29 }, (_, i) => <i key={i} style={{ height: `${8 + (i * 17 % 29)}px` }} />)}</div><span>0:12</span></div><div className="reply-line">Nghe rõ rồi, bắt đầu thôi.<Check size={15} /></div></div>}
  </div>
}

export function SpaceLogin({ direction = 'orbit', onSubmit, className }: SpaceLoginProps) {
  const id = useId()
  const [mode, setMode] = useState<'login' | 'reset' | 'register'>('login')
  const [showPassword, setShowPassword] = useState(false)
  const [loading, setLoading] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [motionPaused, setMotionPaused] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  useEffect(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => setMotionPaused(preference.matches)
    preference.addEventListener('change', update)
    return () => preference.removeEventListener('change', update)
  }, [])

  function changeMode(next: typeof mode) { setMode(next); setNotice(''); setError('') }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (loading) return
    const data = new FormData(event.currentTarget)
    setError(''); setNotice('')
    if (mode === 'login' && !String(data.get('username')).trim()) { setError('Vui lòng nhập tên đăng nhập.'); return }
    if (mode === 'reset') { setNotice('Bản xem thử: yêu cầu khôi phục chưa được gửi. Khi kết nối máy chủ, bạn sẽ nhận được email nếu đã đăng ký.'); return }
    if (!onSubmit) { setNotice('Thông tin đã hợp lệ. Đây là bản xem thử giao diện, chưa kết nối đăng nhập.'); return }
    setLoading(true)
    try {
      await onSubmit({ username: String(data.get('username')).trim(), password: String(data.get('password')), rememberMe: data.get('remember') === 'on' })
      setNotice('Đăng nhập thành công.')
    } catch {
      setError('Tài khoản hoặc mật khẩu không khớp. Vui lòng kiểm tra lại')
    } finally { setLoading(false) }
  }

  return <section className={cn('space-login', `direction-${direction}`, className)} data-motion-enabled={!motionPaused} aria-label="Đăng nhập SpaceLessApp">
    <div className="login-brand"><SpaceLessMark /><span>SpaceLess<span className="brand-app">App</span></span></div>
    <div className="login-scene">
      <SpaceArtwork direction={direction} paused={motionPaused} />
      {(direction === 'orbit' || direction === 'deep-space') && <button className="motion-toggle" onClick={() => setMotionPaused(v => !v)} aria-pressed={motionPaused}>{motionPaused ? <Play size={12} /> : <Pause size={12} />}{motionPaused ? 'Chạy chuyển động' : 'Dừng chuyển động'}</button>}
      <div className="scene-copy"><h2>Xa bao nhiêu.<br /><span>Gần bấy nhiêu.</span></h2><p>Một tin nhắn. Một cuộc gọi.<br />Một không gian dành cho chúng ta.</p></div>
      <div className="scene-caption"><span className="small-orbit" /> Không khoảng cách. Chỉ kết nối.<MoveUpRight size={15} /></div>
    </div>
    <div className="login-form-area">
      <div className="login-form-inner">
        {mode !== 'login' && <button className="back-link" onClick={() => changeMode('login')}><ArrowLeft size={15} /> Quay lại đăng nhập</button>}
        <div className="form-symbol"><MessageCircle size={24} strokeWidth={1.6} /><span /></div>
        <h2>{mode === 'login' ? 'Kết nối lại nhé.' : mode === 'reset' ? 'Tìm lại kết nối.' : 'Bắt đầu kết nối.'}</h2>
        <p className="form-intro">{mode === 'login' ? 'Đăng nhập và tiếp nối những câu chuyện.' : mode === 'reset' ? 'Nhập email đã đăng ký để khôi phục mật khẩu.' : 'Một không gian mới đang chờ bạn.'}</p>
        {mode === 'register' ? <div className="register-info"><p>Đăng ký tài khoản sẽ sớm có mặt. Bản comp hiện tại tập trung vào trải nghiệm đăng nhập.</p><button className="login-submit" onClick={() => changeMode('login')}>Về trang đăng nhập <ArrowUpRight size={18} /></button></div> : <form onSubmit={submit}>
          <div className="field"><label htmlFor={`${id}-username`}>{mode === 'reset' ? 'Email' : 'Tên đăng nhập'}</label><input id={`${id}-username`} key={mode} name={mode === 'reset' ? 'email' : 'username'} type={mode === 'reset' ? 'email' : 'text'} placeholder={mode === 'reset' ? 'ban@example.com' : 'Tên đăng nhập của bạn'} required maxLength={mode === 'reset' ? 100 : 50} autoComplete={mode === 'reset' ? 'email' : 'username'} disabled={loading} onChange={() => { setError(''); setNotice('') }} /></div>
          {mode === 'login' && <><div className="field"><label htmlFor={`${id}-password`}>Mật khẩu</label><div className="password-field"><input id={`${id}-password`} name="password" type={showPassword ? 'text' : 'password'} placeholder="Nhập mật khẩu" autoComplete="current-password" required maxLength={128} disabled={loading} onChange={() => { setError(''); setNotice('') }} /><button type="button" className="password-toggle" onClick={() => setShowPassword(v => !v)} aria-label={showPassword ? 'Ẩn mật khẩu' : 'Hiện mật khẩu'} aria-pressed={showPassword}>{showPassword ? <EyeOff size={18} /> : <Eye size={18} />}</button></div></div><div className="form-options"><label><input type="checkbox" name="remember" disabled={loading} />Ghi nhớ đăng nhập</label><button type="button" onClick={() => changeMode('reset')}>Quên mật khẩu?</button></div></>}
          {error && <p className="form-error" role="alert">{error}</p>}
          <button className="login-submit" type="submit" disabled={loading}>{loading ? <><LoaderCircle className="spin" size={18} /> Đang đăng nhập…</> : <>{mode === 'login' ? 'Đăng nhập' : 'Gửi yêu cầu khôi phục'}<ArrowUpRight size={18} /></>}</button>
          {notice && <p className="form-notice" role="status">{notice}</p>}
          {mode === 'login' && <p className="signup-prompt">Chưa có tài khoản? <button type="button" onClick={() => changeMode('register')}>Tạo tài khoản <ArrowUpRight size={13} /></button></p>}
        </form>}
        <div className="form-bottom"><span /><SpaceLessMark /><span /></div><p className="form-bottom-copy">Cuộc trò chuyện hay bắt đầu từ đây.</p>
      </div>
    </div>
    <div className="login-footer"><span>© {new Date().getFullYear()} SpaceLessApp</span><span>Không gian của bạn, kết nối của bạn.</span></div>
  </section>
}
