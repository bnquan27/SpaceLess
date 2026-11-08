import { useId, useState, type FormEvent } from 'react'
import { ArrowUpRight, Eye, EyeOff } from 'lucide-react'

type Field = 'username' | 'full_name' | 'email' | 'phone' | 'date_of_birth' | 'password'
type Errors = Partial<Record<Field, string>>

export function SpaceSignup({ onBack }: { onBack: () => void }) {
  const id = useId()
  const [errors, setErrors] = useState<Errors>({})
  const [showPassword, setShowPassword] = useState(false)
  const [notice, setNotice] = useState('')
  const today = new Date()
  const maxDate = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const data = new FormData(form)
    const value = (name: Field) => String(data.get(name) ?? '')
    const next: Errors = {}
    if (!value('username').trim()) next.username = 'Vui lòng nhập tên đăng nhập.'
    if (!value('full_name').trim()) next.full_name = 'Vui lòng nhập họ và tên.'
    const email = form.elements.namedItem('email') as HTMLInputElement
    if (!value('email').trim()) next.email = 'Vui lòng nhập email.'
    else if (email.validity.typeMismatch) next.email = 'Vui lòng nhập email hợp lệ, ví dụ ban@example.com.'
    if (!value('phone').trim()) next.phone = 'Vui lòng nhập số điện thoại.'
    if (!value('date_of_birth')) next.date_of_birth = 'Vui lòng chọn ngày sinh.'
    else if (value('date_of_birth') > maxDate) next.date_of_birth = 'Ngày sinh không thể ở tương lai.'
    const passwordLength = Array.from(value('password')).length
    if (!passwordLength) next.password = 'Vui lòng nhập mật khẩu.'
    else if (passwordLength < 12) next.password = 'Mật khẩu cần có từ 12 đến 128 ký tự.'
    setErrors(next)
    setNotice('')
    const first = Object.keys(next)[0] as Field | undefined
    if (first) { (form.elements.namedItem(first) as HTMLInputElement)?.focus(); return }
    setNotice('Thông tin đã hợp lệ. Đây là bản xem thử, tài khoản chưa được tạo.')
  }

  const fields: { name: Field; label: string; type: string; placeholder?: string; maxLength?: number; autoComplete?: string }[] = [
    { name: 'username', label: 'Tên đăng nhập', type: 'text', placeholder: 'Tên đăng nhập của bạn', maxLength: 50, autoComplete: 'username' },
    { name: 'full_name', label: 'Họ và tên', type: 'text', placeholder: 'Tên của bạn', maxLength: 100, autoComplete: 'name' },
    { name: 'email', label: 'Email', type: 'email', placeholder: 'ban@example.com', maxLength: 100, autoComplete: 'email' },
    { name: 'phone', label: 'Số điện thoại', type: 'tel', placeholder: 'Số điện thoại của bạn', maxLength: 15, autoComplete: 'tel' },
    { name: 'date_of_birth', label: 'Ngày sinh', type: 'date', autoComplete: 'bday' },
    { name: 'password', label: 'Mật khẩu', type: showPassword ? 'text' : 'password', placeholder: 'Từ 12 đến 128 ký tự', maxLength: 128, autoComplete: 'new-password' },
  ]

  return <form className="signup-form" onSubmit={submit} noValidate>
    <div className="signup-fields">{fields.map(field => <div className="field" key={field.name}>
      <label htmlFor={`${id}-${field.name}`}>{field.label}</label>
      <div className={field.name === 'password' ? 'password-field' : undefined}>
        <input id={`${id}-${field.name}`} name={field.name} type={field.type} placeholder={field.placeholder} maxLength={field.maxLength} max={field.name === 'date_of_birth' ? maxDate : undefined} autoComplete={field.autoComplete} required aria-invalid={!!errors[field.name]} aria-describedby={errors[field.name] ? `${id}-${field.name}-error` : undefined} onChange={() => { setErrors(previous => ({ ...previous, [field.name]: undefined })); setNotice('') }} />
        {field.name === 'password' && <button type="button" className="password-toggle" onClick={() => setShowPassword(value => !value)} aria-label={showPassword ? 'Ẩn mật khẩu' : 'Hiện mật khẩu'} aria-pressed={showPassword}>{showPassword ? <EyeOff size={18} /> : <Eye size={18} />}</button>}
      </div>
      {errors[field.name] && <p id={`${id}-${field.name}-error`} className="field-error" role="alert">{errors[field.name]}</p>}
    </div>)}</div>
    <p className="signup-hint">Mật khẩu cần có từ 12 đến 128 ký tự.</p>
    <button className="login-submit" type="submit">Tạo tài khoản <ArrowUpRight size={18} /></button>
    {notice && <p className="form-notice" role="status">{notice}</p>}
    <p className="signup-prompt">Đã có tài khoản? <button type="button" onClick={onBack}>Đăng nhập <ArrowUpRight size={13} /></button></p>
  </form>
}
