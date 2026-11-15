import React, { useState } from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/be-vietnam-pro/400.css";
import "@fontsource/be-vietnam-pro/500.css";
import "@fontsource/be-vietnam-pro/600.css";
import "@fontsource/be-vietnam-pro/700.css";
import { SpaceLogin } from "@/components/ui/space-login";
import { SpaceWorkspace } from "@/components/ui/space-workspace";
import "./index.css";

function AppRoot() {
  const [loggedInUser, setLoggedInUser] = useState<string | null>("Nguyễn Minh");
  if (loggedInUser) {
    return (
      <SpaceWorkspace
        currentUser={loggedInUser}
        onLogout={() => setLoggedInUser(null)}
      />
    );
  }

  return (
    <main className="app-login">
      <SpaceLogin
        direction="orbit"
        onSubmit={({ username }) => {
          setLoggedInUser(username);
        }}
      />
    </main>
  );
}

const root = ReactDOM.createRoot(document.getElementById("root")!);

if (window.location.pathname === "/design") {
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
