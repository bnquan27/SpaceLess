use tauri::{AppHandle, Manager};

pub fn is_return_url(value: &str) -> bool {
    value == "spacelessapp://auth/return"
}

pub fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn register_if_missing(app: &AppHandle) {
    #[cfg(windows)]
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        let missing =
            match windows_registry::CLASSES_ROOT.open("spacelessapp\\shell\\open\\command") {
                Ok(key) => key
                    .get_string("")
                    .is_ok_and(|command| registration_is_stale(&command)),
                Err(error) if error.code().0 == 0x80070002u32 as i32 => true,
                Err(_) => {
                    eprintln!("SpaceLessApp could not inspect its return link registration.");
                    false
                }
            };
        if missing && app.deep_link().register("spacelessapp").is_err() {
            eprintln!("SpaceLessApp could not register its return link.");
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn registration_is_stale(command: &str) -> bool {
    let Some((executable, arguments)) = command
        .strip_prefix('"')
        .and_then(|value| value.split_once('"'))
    else {
        return false;
    };
    let path = std::path::Path::new(executable);
    arguments.trim() == "\"%1\"" && path.is_absolute() && matches!(path.try_exists(), Ok(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_link_accepts_only_the_fixed_focus_action() {
        assert!(is_return_url("spacelessapp://auth/return"));
        for value in [
            "spacelessapp://auth/return?code=secret",
            "spacelessapp://auth/return#token",
            "spacelessapp://user@auth/return",
            "spacelessapp://auth:123/return",
            "spacelessapp://auth/return/",
            "spacelessapp://auth/login",
            "https://auth/return",
        ] {
            assert!(!is_return_url(value));
        }
    }

    #[cfg(windows)]
    #[test]
    fn registration_preserves_live_and_unrecognized_handlers() {
        let executable = std::env::current_exe().unwrap();
        let live = format!("\"{}\" \"%1\"", executable.display());
        assert!(!registration_is_stale(&live));
        let missing = executable
            .parent()
            .unwrap()
            .join("spaceless-missing-fixture.exe");
        assert!(registration_is_stale(&format!(
            "\"{}\" \"%1\"",
            missing.display()
        )));
        assert!(!registration_is_stale("\"relative.exe\" \"%1\""));
        assert!(!registration_is_stale(&format!(
            "\"{}\" --unknown \"%1\"",
            missing.display()
        )));
    }
}
