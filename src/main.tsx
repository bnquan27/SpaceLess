import React from 'react'
import ReactDOM from 'react-dom/client'
import '@fontsource/be-vietnam-pro/400.css'
import '@fontsource/be-vietnam-pro/500.css'
import '@fontsource/be-vietnam-pro/600.css'
import '@fontsource/be-vietnam-pro/700.css'
import { SpaceLogin } from '@/components/ui/space-login'
import './index.css'

const root = ReactDOM.createRoot(document.getElementById('root')!)

if (window.location.pathname === '/design') {
  import('@/components/sign-in-explorer').then(({ SignInExplorer }) => {
    root.render(<React.StrictMode><SignInExplorer /></React.StrictMode>)
  })
} else {
  root.render(<React.StrictMode><main className="app-login"><SpaceLogin direction="orbit" /></main></React.StrictMode>)
}
