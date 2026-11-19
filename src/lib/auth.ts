import { invoke, isTauri } from '@tauri-apps/api/core'

export type AuthAvailability = {
  password: boolean
  google: boolean
  message: string
}

export type AuthProfile = { username: string; full_name: string }

export async function authAvailability(): Promise<AuthAvailability> {
  if (!isTauri()) return { password: false, google: false, message: 'Bản xem thử trên trình duyệt. Đăng nhập thật được hỗ trợ trong ứng dụng desktop đã cấu hình máy chủ.' }
  const value: unknown = await invoke('auth_availability')
  if (typeof value !== 'object' || value === null || !('password' in value) || typeof value.password !== 'boolean' || !('google' in value) || typeof value.google !== 'boolean' || !('message' in value) || typeof value.message !== 'string') {
    throw new Error('Không thể đọc cấu hình đăng nhập.')
  }
  return { password: value.password, google: value.google, message: value.message }
}

async function login(command: 'auth_google_login' | 'auth_password_login', args?: { username: string; password: string }): Promise<AuthProfile> {
  try {
    const value: unknown = await invoke(command, args)
    if (typeof value !== 'object' || value === null || !('username' in value) || typeof value.username !== 'string' || !value.username.trim() || !('full_name' in value) || typeof value.full_name !== 'string' || !value.full_name.trim()) {
      throw new Error('Không thể xác minh hồ sơ tài khoản.')
    }
    return { username: value.username, full_name: value.full_name }
  } catch (error) {
    throw new Error(typeof error === 'string' ? error : error instanceof Error ? error.message : 'Không thể đăng nhập. Vui lòng thử lại.')
  }
}

export const googleLogin = () => login('auth_google_login')
export const passwordLogin = (args: { username: string; password: string }) => login('auth_password_login', args)
export const cancelLogin = () => invoke<void>('auth_cancel')
export const logout = () => invoke<void>('auth_logout')
