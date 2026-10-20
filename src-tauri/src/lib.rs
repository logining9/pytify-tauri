use tauri::Manager;
use rodio::{Decoder, OutputStreamBuilder, Sink};
use std::fs::File;
use std::io::BufReader;
use std::sync::Mutex;
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::fs;
use std::io::Write;
use tauri_plugin_shell::ShellExt;

#[derive(Serialize)]
struct AudioInfo {
    path: String,
    video_id: String,
    title: String,
}

#[derive(Serialize)]
struct DownloadedTrack {
    title: String,
    path: String,
}

struct AudioState {
    sink: Mutex<Option<Sink>>,
    _stream: Mutex<Option<rodio::OutputStream>>,
    current_volume: Mutex<f32>,
}

fn get_ytdlp_command(app_handle: &tauri::AppHandle) -> Result<tauri_plugin_shell::process::Command, String> {
    app_handle.shell().sidecar("yt-dlp").map_err(|e| e.to_string())
}

// Общие аргументы для всех вызовов yt-dlp (Deno + EJS + клиенты tv,web)
fn get_common_args() -> Vec<&'static str> {
    vec![
        "--js-runtimes", "deno:/usr/bin/deno",
        "--remote-components", "ejs:github",
        "--extractor-args", "youtube:player_client=tv,web", // Убрали mweb
    ]
}

async fn get_radio_track(current_id: &str, direction: i32, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let radio_url = format!("https://www.youtube.com/watch?v={}&list=RD{}", current_id, current_id);
    let mut args = get_common_args();
    args.extend(vec!["--flat-playlist", "--print", "id", &radio_url]);

    let output = get_ytdlp_command(app_handle)?
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Ошибка получения радио: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<String> = stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    
    if ids.is_empty() { return Err("Радио пустое".to_string()); }

    if let Some(pos) = ids.iter().position(|id| id == current_id) {
        let new_pos = if direction > 0 {
            if pos + 1 < ids.len() { pos + 1 } else { 0 }
        } else {
            if pos > 0 { pos - 1 } else { ids.len() - 1 }
        };
        Ok(ids[new_pos].clone())
    } else {
        Ok(ids[0].clone())
    }
}

async fn download_by_id(video_id: &str, temp_dir: &std::path::Path, app_handle: &tauri::AppHandle) -> Result<(String, String), String> {
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    // 1. Получаем название
    let mut title_args = get_common_args();
    title_args.extend(vec!["--print", "title", &url]);
    
    let title_output = get_ytdlp_command(app_handle)?
        .args(&title_args)
        .output()
        .await
        .map_err(|e| format!("Ошибка получения названия: {}", e))?;
    
    let title = String::from_utf8_lossy(&title_output.stdout).trim().to_string();

    // 2. Скачиваем аудио (ЗДЕСЬ ТОЖЕ ДОБАВЛЕНЫ АРГУМЕНТЫ DENO/EJS)
    let mut dl_args = get_common_args();
    dl_args.extend(vec![
        "-x", "--audio-format", "mp3", "--audio-quality", "0",
        "--embed-metadata",
        "-o", &format!("{}/%(title)s.%(ext)s", temp_dir.display()),
        &url
    ]);

    let output = get_ytdlp_command(app_handle)?
        .args(&dl_args)
        .output()
        .await
        .map_err(|e| format!("Ошибка yt-dlp: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let mut newest_file = None;
    let mut newest_time = std::time::SystemTime::UNIX_EPOCH;
    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mp3") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified > newest_time {
                            newest_time = modified;
                            newest_file = Some(path);
                        }
                    }
                }
            }
        }
    }

    let path = newest_file.map(|p| p.to_string_lossy().to_string()).ok_or_else(|| "Файл не найден".to_string())?;
    Ok((path, title))
}

#[tauri::command]
async fn download_audio(query: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    let home_dir = app_handle.path().home_dir().map_err(|e| e.to_string())?;
    let output_dir = home_dir.join("Music").join("PyTify");
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let url = if query.starts_with("http") { query } else { format!("ytsearch1:{}", query) };

    let mut args = get_common_args();
    args.extend(vec![
        "-x", "--audio-format", "mp3", "--audio-quality", "0",
        "--embed-metadata", "--embed-thumbnail",
        "-o", &format!("{}/%(title)s.%(ext)s", output_dir.display()),
        &url
    ]);

    let output = get_ytdlp_command(&app_handle)?
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Не удалось запустить yt-dlp: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let mut newest_file = None;
    let mut newest_time = std::time::SystemTime::UNIX_EPOCH;
    if let Ok(entries) = std::fs::read_dir(&output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mp3") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified > newest_time {
                            newest_time = modified;
                            newest_file = Some(path);
                        }
                    }
                }
            }
        }
    }
    newest_file.map(|p| p.to_string_lossy().to_string()).ok_or_else(|| "Файл не найден".to_string())
}

#[tauri::command]
fn get_downloaded_tracks(app_handle: tauri::AppHandle) -> Result<Vec<DownloadedTrack>, String> {
    let home_dir = app_handle.path().home_dir().map_err(|e| e.to_string())?;
    let dir = home_dir.join("Music").join("PyTify");
    let mut tracks = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mp3") {
                let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown").to_string();
                tracks.push(DownloadedTrack {
                    title,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
    tracks.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(tracks)
}

#[tauri::command]
async fn stream_audio(query: String, radio_id: Option<String>, direction: Option<i32>, app_handle: tauri::AppHandle) -> Result<AudioInfo, String> {
    let home_dir = app_handle.path().home_dir().map_err(|e| e.to_string())?;
    let temp_dir = home_dir.join(".cache").join("pytify-tauri").join("streams");
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    let mut final_video_id = String::new();

    if let (Some(r_id), Some(dir)) = (radio_id, direction) {
        final_video_id = get_radio_track(&r_id, dir, &app_handle).await?;
    } else {
        let url = if query.starts_with("http") { query } else { format!("ytsearch1:{}", query) };
        
        let mut id_args = get_common_args();
        id_args.extend(vec!["--flat-playlist", "--print", "id", &url]);

        let id_output = get_ytdlp_command(&app_handle)?
            .args(&id_args)
            .output()
            .await
            .map_err(|e| format!("Ошибка получения ID: {}", e))?;
        
        if id_output.status.success() {
            let id_str = String::from_utf8_lossy(&id_output.stdout).trim().to_string();
            if let Some(first_id) = id_str.lines().next() {
                final_video_id = first_id.to_string();
            }
        }
    }

    if final_video_id.is_empty() {
        return Err("Не удалось определить ID видео".to_string());
    }

    let (path, title) = download_by_id(&final_video_id, &temp_dir, &app_handle).await?;

    Ok(AudioInfo {
        path,
        video_id: final_video_id,
        title,
    })
}

#[tauri::command]
fn play_audio(file_path: String, state: tauri::State<AudioState>) -> Result<(), String> {
    if let Ok(mut sink_guard) = state.sink.lock() {
        if let Some(sink) = sink_guard.take() { sink.stop(); }
    }
    let stream = OutputStreamBuilder::open_default_stream().map_err(|e| format!("{}", e))?;
    if let Ok(mut stream_guard) = state._stream.lock() { *stream_guard = Some(stream); }
    let sink = {
        let stream_guard = state._stream.lock().map_err(|e| e.to_string())?;
        let stream_ref = stream_guard.as_ref().ok_or("Поток не создан")?;
        Sink::connect_new(stream_ref.mixer())
    };
    if let Ok(vol_guard) = state.current_volume.lock() { sink.set_volume(*vol_guard); }
    let file = BufReader::new(File::open(&file_path).map_err(|e| format!("{}", e))?);
    let source = Decoder::new(file).map_err(|e| format!("{}", e))?;
    sink.append(source);
    sink.play();
    if let Ok(mut sink_guard) = state.sink.lock() { *sink_guard = Some(sink); }
    Ok(())
}

#[tauri::command]
fn toggle_playback(state: tauri::State<AudioState>) -> Result<(), String> {
    if let Ok(sink_guard) = state.sink.lock() {
        if let Some(sink) = sink_guard.as_ref() {
            if sink.is_paused() { sink.play(); } else { sink.pause(); }
        }
    }
    Ok(())
}

#[tauri::command]
fn set_volume(volume: f32, state: tauri::State<AudioState>) -> Result<(), String> {
    let vol = volume.clamp(0.0, 1.0);
    if let Ok(mut vol_guard) = state.current_volume.lock() { *vol_guard = vol; }
    if let Ok(sink_guard) = state.sink.lock() {
        if let Some(sink) = sink_guard.as_ref() { sink.set_volume(vol); }
    }
    Ok(())
}

#[tauri::command]
fn is_playing(state: tauri::State<AudioState>) -> bool {
    if let Ok(sink_guard) = state.sink.lock() {
        if let Some(sink) = sink_guard.as_ref() { return !sink.empty() && !sink.is_paused(); }
    }
    false
}

fn integrate_appimage() {
    let appimage_path = match env::var("APPIMAGE") { Ok(path) => path, Err(_) => return };
    let home = match env::var("HOME") { Ok(h) => PathBuf::from(h), Err(_) => return };
    let apps_dir = home.join(".local/share/applications");
    let icons_dir = home.join(".local/share/icons/hicolor/128x128/apps");
    let _ = fs::create_dir_all(&apps_dir);
    let _ = fs::create_dir_all(&icons_dir);
    if let Ok(appdir) = env::var("APPDIR") {
        let icon_src = PathBuf::from(appdir).join("usr/share/icons/hicolor/128x128/apps/pytify.png");
        let icon_dest = icons_dir.join("pytify.png");
        if icon_src.exists() { let _ = fs::copy(&icon_src, &icon_dest); }
    }
    let desktop_path = apps_dir.join("pytify.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\nType=Application\nName=PyTify\nComment=YouTube/Spotify Music Player\nExec={} %U\nIcon=pytify\nTerminal=false\nCategories=AudioVideo;Audio;Player;\nStartupWMClass=pytify-tauri\n",
        appimage_path
    );
    if let Ok(mut file) = fs::File::create(&desktop_path) {
        let _ = file.write_all(desktop_content.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&desktop_path, fs::Permissions::from_mode(0o755));
        }
    }
    let _ = std::process::Command::new("update-desktop-database").arg(&apps_dir).status();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    integrate_appimage();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(AudioState { 
            sink: Mutex::new(None), 
            _stream: Mutex::new(None),
            current_volume: Mutex::new(1.0)
        })
        .invoke_handler(tauri::generate_handler![
            download_audio, stream_audio, play_audio, toggle_playback, set_volume, is_playing, get_downloaded_tracks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}