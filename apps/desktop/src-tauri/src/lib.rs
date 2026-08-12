use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendStatus {
    status: &'static str,
    mock_mode_available: bool,
    credential_boundary: &'static str,
}

/// This native command is the initial UI/backend boundary. The webview never
/// imports a SQLite implementation or receives provider credentials.
#[tauri::command]
fn backend_status() -> BackendStatus {
    BackendStatus {
        status: "ok",
        mock_mode_available: true,
        credential_boundary: "native-backend",
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![backend_status])
        .run(tauri::generate_context!())
        .expect("Tauri application failed to run");
}
