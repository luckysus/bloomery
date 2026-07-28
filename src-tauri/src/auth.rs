use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub user_id: String,
    pub token: String,
    pub username: Option<String>,
    pub email: Option<String>,
}

#[derive(Default)]
pub struct AuthState {
    current: Mutex<Option<AuthSession>>,
}

impl AuthState {
    pub fn current_user_id(&self) -> Result<String, String> {
        self.current
            .lock()
            .map_err(|_| "auth state poisoned")?
            .as_ref()
            .map(|session| session.user_id.clone())
            .ok_or_else(|| "not authenticated".to_string())
    }

    pub fn current_session(&self) -> Result<AuthSession, String> {
        self.current
            .lock()
            .map_err(|_| "auth state poisoned")?
            .clone()
            .ok_or_else(|| "not authenticated".to_string())
    }
}

fn session_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("resolve app data dir failed: {err}"))?;
    fs::create_dir_all(&dir).map_err(|err| format!("create app data dir failed: {err}"))?;
    Ok(dir.join("auth-session.json"))
}

#[tauri::command]
pub fn auth_get_session(
    app: tauri::AppHandle,
    state: tauri::State<AuthState>,
) -> Result<Option<AuthSession>, String> {
    if let Some(session) = state
        .current
        .lock()
        .map_err(|_| "auth state poisoned")?
        .clone()
    {
        return Ok(Some(session));
    }
    let path = session_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|err| format!("read session failed: {err}"))?;
    let session: AuthSession =
        serde_json::from_str(&text).map_err(|err| format!("parse session failed: {err}"))?;
    *state.current.lock().map_err(|_| "auth state poisoned")? = Some(session.clone());
    Ok(Some(session))
}

#[tauri::command]
pub fn auth_save_session(
    app: tauri::AppHandle,
    state: tauri::State<AuthState>,
    session: AuthSession,
) -> Result<(), String> {
    let path = session_path(&app)?;
    let text = serde_json::to_string_pretty(&session)
        .map_err(|err| format!("encode session failed: {err}"))?;
    fs::write(path, text).map_err(|err| format!("write session failed: {err}"))?;
    *state.current.lock().map_err(|_| "auth state poisoned")? = Some(session);
    Ok(())
}

#[tauri::command]
pub fn auth_clear_session(
    app: tauri::AppHandle,
    state: tauri::State<AuthState>,
) -> Result<(), String> {
    let path = session_path(&app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| format!("remove session failed: {err}"))?;
    }
    *state.current.lock().map_err(|_| "auth state poisoned")? = None;
    Ok(())
}
