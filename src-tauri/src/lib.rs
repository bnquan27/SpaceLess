mod auth;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
