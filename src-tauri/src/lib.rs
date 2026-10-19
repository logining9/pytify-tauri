use std::process::Command;
use tauri::{Manager, Emitter};
use rodio::{Decoder, OutputStreamBuilder, Sink};
use std::fs::File;
use std::io::BufReader;
use std::sync::Mutex;
use std::time::Duration;
use rand::{RngExt, rng};
use serde::Serialize;

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

fn get_radio_track(current_id: &str, direction: i32) -> Result<String, String> {
    let radio_url = format!("https://www.youtube.com/watch?v={}&list=RD{}", current_id, current_id);
    let output = Command::new("/usr/bin/yt-dlp")
        .arg("--extractor-args").arg("youtube:player_client=tv,mweb")
        .arg("--flat-playlist").arg("--print").arg("id")
        .arg(&radio_url)
        .output()
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

async fn download_by_id(video_id: &str, temp_dir: &std::path::Path) -> Result<(String, String), String> {
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    let title_output = Command::new("/usr/bin/yt-dlp")
        .arg("--extractor-args").arg("youtube:player_client=tv,mweb")
        .arg("--print").arg("title")
        .arg(&url)
        .output()
        .map_err(|e| format!("Ошибка получения названия: {}", e))?;
    
    let title = String::from_utf8_lossy(&title_output.stdout).trim().to_string();

    let output = Command::new("/usr/bin/yt-dlp")
        .arg("--extractor-args").arg("youtube:player_client=tv,mweb")
        .arg("-x").arg("--audio-format").arg("mp3").arg("--audio-quality").arg("0")
        .arg("--embed-metadata")
        .arg("-o").arg(format!("{}/%(title)s.%(ext)s", temp_dir.display()))
        .arg(&url)
        .output()
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

    let output = Command::new("/usr/bin/yt-dlp")
        .arg("--extractor-args").arg("youtube:player_client=tv,mweb")
        .arg("-x").arg("--audio-format").arg("mp3").arg("--audio-quality").arg("0")
        .arg("--embed-metadata").arg("--embed-thumbnail")
        .arg("-o").arg(format!("{}/%(title)s.%(ext)s", output_dir.display()))
        .arg(&url)
        .output()
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
    // Сортируем по алфавиту
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
        final_video_id = get_radio_track(&r_id, dir)?;
    } else {
        let url = if query.starts_with("http") { query } else { format!("ytsearch1:{}", query) };
        let id_output = Command::new("/usr/bin/yt-dlp")
            .arg("--extractor-args").arg("youtube:player_client=tv,mweb")
            .arg("--flat-playlist").arg("--print").arg("id")
            .arg(&url)
            .output()
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

    let (path, title) = download_by_id(&final_video_id, &temp_dir).await?;

    Ok(AudioInfo {
        path,
        video_id: final_video_id,
        title,
    })
}

#[tauri::command]
fn play_audio(file_path: String, app_handle: tauri::AppHandle, state: tauri::State<AudioState>) -> Result<(), String> {
    // Останавливаем предыдущий трек, если он играл
    if let Ok(mut sink_guard) = state.sink.lock() {
        if let Some(sink) = sink_guard.take() { sink.stop(); }
    }

    // 1. Создаем поток
    let stream = OutputStreamBuilder::open_default_stream().map_err(|e| format!("{}", e))?;
    
    // 2. СРАЗУ сохраняем поток в состояние, чтобы он не удалился!
    // (Это ключевой момент: сохраняем до создания Sink и начала воспроизведения)
    if let Ok(mut stream_guard) = state._stream.lock() {
        *stream_guard = Some(stream);
    }

    // 3. Создаем Sink, используя поток из состояния
    // (Мы берем ссылку на поток из мьютекса, чтобы не перемещать его)
    let sink = {
        let stream_guard = state._stream.lock().map_err(|e| e.to_string())?;
        let stream_ref = stream_guard.as_ref().ok_or("Поток не создан")?;
        Sink::connect_new(stream_ref.mixer())
    };

    // Применяем сохраненную громкость
    if let Ok(vol_guard) = state.current_volume.lock() {
        sink.set_volume(*vol_guard);
    }

    // 4. Открываем файл и играем
    let file = BufReader::new(File::open(&file_path).map_err(|e| format!("{}", e))?);
    let source = Decoder::new(file).map_err(|e| format!("{}", e))?;

    sink.append(source);
    sink.play();

    // 5. Сохраняем Sink в состояние
    if let Ok(mut sink_guard) = state.sink.lock() { *sink_guard = Some(sink); }

    // ... (код эквалайзера остается без изменений) ...

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
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