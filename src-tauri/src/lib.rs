mod auth;
mod oauth_return;

use tauri_plugin_deep_link::DeepLinkExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _| {
        if args.len() == 2 && oauth_return::is_return_url(&args[1]) {
            oauth_return::focus_main_window(app);
        }
    }));
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            oauth_return::register_if_missing(app.handle());
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                if event
                    .urls()
                    .iter()
                    .any(|url| oauth_return::is_return_url(url.as_str()))
                {
                    oauth_return::focus_main_window(&handle);
                }
            });
            Ok(())
        })
        .manage(auth::AuthState::default())
        .invoke_handler(tauri::generate_handler![
            auth::auth_availability,
            auth::auth_password_login,
            auth::auth_google_login,
            auth::auth_cancel,
            auth::auth_logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SpaceLessApp");
}
