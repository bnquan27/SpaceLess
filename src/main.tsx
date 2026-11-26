import React, { useCallback, useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/be-vietnam-pro/400.css";
import "@fontsource/be-vietnam-pro/500.css";
import "@fontsource/be-vietnam-pro/600.css";
import "@fontsource/be-vietnam-pro/700.css";
import { SpaceLogin } from "@/components/ui/space-login";
import { SpaceWorkspace } from "@/components/ui/space-workspace";
import { ApiClient, type Session } from "@/lib/api";
import { BackendProvider, useBackend } from "@/lib/workspace-backend";
import "./index.css";

function AppRoot() {
  const api = useRef(new ApiClient()).current;
  const [session, setSession] = useState<Session | null>(null);
  const [expired, setExpired] = useState(false);
  const clear = useCallback(() => { api.clear(); setSession(null); setExpired(true); }, [api]);
  useEffect(() => {
    if (!session) return;
    const timer = setTimeout(() => { void api.request<Session>('/auth/refresh', 'POST').then(value => { api.setToken(value.access_token); setSession(value); }).catch(clear); }, Math.min(2147483647, Math.max(1000, Date.parse(session.expires_at) - Date.now() - 60000)));
    return () => clearTimeout(timer);
  }, [api, session, clear]);
  if (session) return <BackendProvider api={api} tokenVersion={session.access_token} onExpired={clear}><LiveWorkspace onLogout={async () => { await api.request('/auth/logout', 'POST'); api.clear(); setSession(null); }}/></BackendProvider>;

  return (
    <main className="app-login">
      {expired && <p role="alert" className="fixed top-3 left-1/2 z-50 -translate-x-1/2 rounded-xl bg-[#261e3a] p-3 text-sm text-[#f4effa]">Phiên đăng nhập đã kết thúc. Vui lòng đăng nhập lại.</p>}
      <SpaceLogin
        direction="orbit"
        sessionOnly
        onSubmit={async ({ username, password }) => { const value = await api.request<Session>('/auth/login', 'POST', { username, password }); api.setToken(value.access_token); setExpired(false); setSession(value); }}
        onRegister={async data => { await api.request('/auth/register', 'POST', data); }}
        onRecover={async email => { await api.request('/auth/forgot-password', 'POST', { email }); }}
      />
    </main>
  );
}

function LiveWorkspace({ onLogout }: { onLogout: () => Promise<void> }) {
  const backend = useBackend()!;
  return <SpaceWorkspace currentUser={backend.data.profile.full_name} onLogout={() => void onLogout().catch(backend.report)}/>;
}

const root = ReactDOM.createRoot(document.getElementById("root")!);

if (window.location.pathname === "/preview") {
  root.render(<React.StrictMode><SpaceWorkspace/></React.StrictMode>);
} else if (window.location.pathname === "/design") {
  import("@/components/sign-in-explorer").then(({ SignInExplorer }) => {
    root.render(
      <React.StrictMode>
        <SignInExplorer />
      </React.StrictMode>,
    );
  });
} else {
  root.render(
    <React.StrictMode>
      <AppRoot />
    </React.StrictMode>,
  );
}
