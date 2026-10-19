const input = document.getElementById('search-input');
const btnPlay = document.getElementById('btn-play');
const btnDownload = document.getElementById('btn-download');
const btnPause = document.getElementById('btn-pause');
const btnPrev = document.getElementById('btn-prev');
const btnNext = document.getElementById('btn-next');
const historyList = document.getElementById('history-list');
const playlistsContainer = document.getElementById('playlists-container');
const volumeSlider = document.getElementById('volume-slider');
const themeSelect = document.getElementById('theme-select');
const vinylDisc = document.getElementById('vinyl-disc');
const nowPlayingTitle = document.getElementById('now-playing-title');

const vinylEqCanvas = document.getElementById('vinyl-eq-canvas');
const eqCtx = vinylEqCanvas.getContext('2d');
let eqBars = new Array(36).fill(0.1);

let store;
let history = []; 
let playlists = [[], []]; // Теперь 2 плейлиста: 0 и 1
let downloadedTracks = []; // Скачанные песни
let isUserPaused = false;
let isAutoPlaying = false;
let currentHistoryIndex = -1;

// Контекст воспроизведения
let activePlaylistIndex = -1; // -1 (не плейлист), 0, 1 (плейлисты), 2 (скачанные)
let activePlaylistTrackIndex = -1;

async function initStore() {
    store = await window.__TAURI__.store.load('pytify-data.json', { autoSave: true });
    history = await store.get('history') || [];
    
    // Если в Store был старый формат с 3 плейлистами, обрезаем до 2
    let savedPlaylists = await store.get('playlists') || [[], [], []];
    playlists = [savedPlaylists[0] || [], savedPlaylists[1] || []];

    const savedTheme = await store.get('theme') || 'dark';
    document.documentElement.dataset.theme = savedTheme;
    themeSelect.value = savedTheme;

    const savedVolume = await store.get('volume') ?? 1.0;
    volumeSlider.value = savedVolume * 100;
    await applyVolume(savedVolume);

    await loadDownloadedTracks();
    renderHistory();
    renderPlaylists();
}

async function loadDownloadedTracks() {
    try {
        downloadedTracks = await window.__TAURI__.core.invoke('get_downloaded_tracks');
        // Добавляем в конец массив плейлистов (индекс 2)
        playlists[2] = downloadedTracks.map(t => ({ 
            query: t.path, // Для локальных файлов query - это путь
            title: t.title,
            isLocal: true 
        }));
    } catch(e) {
        console.error("Ошибка загрузки скачанных:", e);
    }
}

initStore();

async function applyVolume(vol) {
    try { await window.__TAURI__.core.invoke('set_volume', { volume: vol }); } catch (e) { console.error(e); }
}

volumeSlider.addEventListener('input', async (e) => {
    const vol = e.target.value / 100;
    await applyVolume(vol);
    await store.set('volume', vol);
});

themeSelect.addEventListener('change', async (e) => {
    document.documentElement.dataset.theme = e.target.value;
    await store.set('theme', e.target.value);
});

function setPlayingState(isPlaying) {
    if (isPlaying) {
        vinylDisc.classList.add('playing');
    } else {
        vinylDisc.classList.remove('playing');
    }
}

// Универсальная функция для игры. 
// track.isLocal === true означает, что query - это путь к файлу, и yt-dlp не нужен.
async function playTrack(track, updateHistory = true, plIdx = -1, trackIdx = -1) {
    isUserPaused = false;
    btnPlay.textContent = "⏳ Загрузка...";
    btnPlay.disabled = true;
    
    setPlayingState(false);
    nowPlayingTitle.textContent = "Загрузка...";

    try {
        let path, title, videoId;
        
        if (track.isLocal) {
            // Если это локальный файл, просто играем
            path = track.query;
            title = track.title;
            videoId = null;
            await window.__TAURI__.core.invoke('play_audio', { filePath: path });
        } else {
            // Иначе ищем в сети
            const args = { query: track.query || "" };
            const info = await window.__TAURI__.core.invoke('stream_audio', args);
            await window.__TAURI__.core.invoke('play_audio', { filePath: info.path });
            path = info.path;
            title = info.title || track.query || "Рекомендация";
            videoId = info.video_id;
        }
        
        nowPlayingTitle.textContent = title;
        setPlayingState(true);
        
        // Управление контекстом
        if (plIdx !== -1) {
            activePlaylistIndex = plIdx;
            activePlaylistTrackIndex = trackIdx;
        } else {
            activePlaylistIndex = -1;
            activePlaylistTrackIndex = -1;
        }

        if (updateHistory && videoId) {
            history = history.filter(h => h.videoId !== videoId);
            history.unshift({ query: track.query, title, videoId, timestamp: Date.now() });
            history = history.slice(0, 25);
            currentHistoryIndex = 0;
            await store.set('history', history);
            renderHistory();
        } else if (updateHistory && !videoId) {
            // Локальный файл - сохраняем как локальный
            history = history.filter(h => h.query !== track.query);
            history.unshift({ query: track.query, title, videoId: null, isLocal: true, timestamp: Date.now() });
            history = history.slice(0, 25);
            currentHistoryIndex = 0;
            await store.set('history', history);
            renderHistory();
        }
        
        btnPlay.textContent = "▶ Слушать (поток)";
    } catch (error) {
        console.error("Ошибка:", error);
        nowPlayingTitle.textContent = "Ошибка загрузки";
        btnPlay.textContent = "▶ Слушать (поток)";
    } finally {
        btnPlay.disabled = false;
    }
}

function renderHistory() {
    historyList.innerHTML = '';
    if (history.length === 0) {
        historyList.innerHTML = '<li style="color: #555; cursor: default; justify-content: center;">Пока пусто</li>';
        return;
    }
    history.forEach((item, index) => {
        const li = document.createElement('li');
        li.textContent = item.title;
        li.onclick = () => playTrack(item, false, -1, -1);
        historyList.appendChild(li);
    });
}

async function removeFromPlaylist(plIdx, itemIdx) {
    playlists[plIdx].splice(itemIdx, 1);
    await store.set('playlists', playlists);
    renderPlaylists();
}

function renderPlaylists() {
    playlistsContainer.innerHTML = '';
    
    // Рендерим все 3 колонки (0, 1 и Скачанные)
    const labels = ["Плейлист 1", "Плейлист 2", "Скачанные"];
    
    labels.forEach((label, idx) => {
        const col = document.createElement('div');
        col.className = 'playlist-col';
        col.innerHTML = `<h4>${label}</h4>`;
        
        const list = document.createElement('ul');
        list.style.border = 'none';
        list.style.maxHeight = '100px';
        
        const items = playlists[idx] || [];
        
        items.forEach((item, itemIdx) => {
            const li = document.createElement('li');
            li.textContent = item.title;
            li.onclick = (e) => {
                if (e.target.tagName !== 'BUTTON') {
                    playTrack(item, false, idx, itemIdx);
                }
            };
            
            // Для скачанных плейлист пересоздается из файлов, удалять из него нельзя
            if (idx !== 2) {
                const btnRemove = document.createElement('button');
                btnRemove.className = 'btn-remove';
                btnRemove.textContent = '✕';
                btnRemove.onclick = () => removeFromPlaylist(idx, itemIdx);
                li.appendChild(btnRemove);
            }

            list.appendChild(li);
        });
        col.appendChild(list);

        // Кнопку "Добавить" не показываем для скачанных
        if (idx !== 2) {
            const btnAdd = document.createElement('button');
            btnAdd.className = 'btn-add';
            btnAdd.textContent = '+ Добавить';
            btnAdd.onclick = async () => {
                const currentTitle = nowPlayingTitle.textContent;
                if (currentTitle === "Ничего не играет" || currentTitle === "Загрузка..." || currentTitle === "Ошибка загрузки" || currentTitle === "Поиск рекомендации...") {
                    return alert("Сейчас ничего не играет");
                }
                
                if (!items.some(i => i.title === currentTitle)) {
                    items.push({ query: input.value.trim() || currentTitle, title: currentTitle, isLocal: false });
                    await store.set('playlists', playlists);
                    renderPlaylists();
                }
            };
            col.appendChild(btnAdd);
        }
        playlistsContainer.appendChild(col);
    });
}

btnPlay.addEventListener('click', () => {
    playTrack({ query: input.value.trim(), title: input.value.trim(), isLocal: false }, true);
});

btnDownload.addEventListener('click', async () => {
    const query = input.value.trim();
    if (!query) return alert("Введите запрос");
    btnDownload.textContent = "⏳...";
    btnDownload.disabled = true;
    try {
        await window.__TAURI__.core.invoke('download_audio', { query });
        alert("Скачано в ~/Music/PyTify");
        await loadDownloadedTracks();
        renderPlaylists();
    } catch (e) {
        alert(`Ошибка: ${e}`);
    } finally {
        btnDownload.textContent = "⬇ Скачать";
        btnDownload.disabled = false;
    }
});

btnPause.addEventListener('click', async () => {
    isUserPaused = true;
    await window.__TAURI__.core.invoke('toggle_playback');
    const playing = await window.__TAURI__.core.invoke('is_playing');
    setPlayingState(playing);
});

// Умное переключение треков
async function playNextSmart(direction) {
    // Если мы в контексте плейлиста (или скачанных)
    if (activePlaylistIndex !== -1 && activePlaylistIndex !== 2) {
        const pl = playlists[activePlaylistIndex];
        if (pl.length > 0) {
            let nextIdx = activePlaylistTrackIndex + direction;
            if (nextIdx >= pl.length) nextIdx = 0;
            if (nextIdx < 0) nextIdx = pl.length - 1;
            
            activePlaylistTrackIndex = nextIdx;
            await playTrack(pl[nextIdx], false, activePlaylistIndex, nextIdx);
            return;
        }
    }
    
    // Если мы в скачанных (или плейлист пуст)
    if (activePlaylistIndex === 2) {
        const pl = playlists[2];
        if (pl.length > 0) {
            let nextIdx = activePlaylistTrackIndex + direction;
            if (nextIdx >= pl.length) nextIdx = 0;
            if (nextIdx < 0) nextIdx = pl.length - 1;
            
            activePlaylistTrackIndex = nextIdx;
            await playTrack(pl[nextIdx], false, 2, nextIdx);
            return;
        }
    }

    // Обычное радио (из истории)
    if (history.length > 0 && currentHistoryIndex !== -1) {
        const currentVideoId = history[currentHistoryIndex].videoId;
        if (currentVideoId && !history[currentHistoryIndex].isLocal) {
            // Ищем рекомендации
            const args = { query: "", radioId: currentVideoId, direction: direction };
            try {
                const info = await window.__TAURI__.core.invoke('stream_audio', args);
                await window.__TAURI__.core.invoke('play_audio', { filePath: info.path });
                nowPlayingTitle.textContent = info.title;
                setPlayingState(true);
                
                // Не добавляем в историю, чтобы не зациклиться
            } catch(e) {
                console.error("Ошибка радио:", e);
            }
        } else {
            // Если локальный, просто листаем историю
            let nextIdx = currentHistoryIndex + direction;
            if (nextIdx >= history.length) nextIdx = 0;
            if (nextIdx < 0) nextIdx = history.length - 1;
            currentHistoryIndex = nextIdx;
            await playTrack(history[nextIdx], false, -1, -1);
        }
    }
}

btnNext.addEventListener('click', () => playNextSmart(1));
btnPrev.addEventListener('click', () => playNextSmart(-1));

input.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') btnPlay.click();
});

// --- Круговой эквалайзер ---
window.__TAURI__.event.listen('eq_update', (event) => {
    let currentBars = event.payload;
    if (isUserPaused) {
        eqBars = eqBars.map(v => Math.max(0.1, v - 0.05));
    } else {
        eqBars = eqBars.map((v, i) => v + (currentBars[i % currentBars.length] - v) * 0.3);
    }
    drawVinylEq();
});

function drawVinylEq() {
    const width = vinylEqCanvas.width;
    const height = vinylEqCanvas.height;
    const centerX = width / 2;
    const centerY = height / 2;
    const baseRadius = 65; 
    
    eqCtx.clearRect(0, 0, width, height);
    
    const accentColor = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#1DB954';
    eqCtx.strokeStyle = accentColor;
    eqCtx.lineWidth = 4;
    eqCtx.lineCap = 'round';

    const numBars = 36; 
    
    for (let i = 0; i < numBars; i++) {
        const angle = (Math.PI * 2 * i) / numBars;
        const barVal = eqBars[i % eqBars.length];
        const barLength = isUserPaused ? 2 : 5 + (barVal * 25); 
        
        const startX = centerX + Math.cos(angle) * baseRadius;
        const startY = centerY + Math.sin(angle) * baseRadius;
        const endX = centerX + Math.cos(angle) * (baseRadius + barLength);
        const endY = centerY + Math.sin(angle) * (baseRadius + barLength);
        
        eqCtx.beginPath();
        eqCtx.moveTo(startX, startY);
        eqCtx.lineTo(endX, endY);
        eqCtx.stroke();
    }
}

// --- Автовоспроизведение ---
setInterval(async () => {
    if (isUserPaused || history.length === 0) return;
    if (btnPlay.disabled || btnDownload.disabled) return;
    
    // Если играет плейлист или скачанные, не вмешиваемся
    if (activePlaylistIndex !== -1) {
        return; 
    }

    if (currentHistoryIndex === -1) return;

    try {
        const playing = await window.__TAURI__.core.invoke('is_playing');
        if (!playing && !isAutoPlaying) {
            isAutoPlaying = true;
            setPlayingState(false);
            nowPlayingTitle.textContent = "Поиск рекомендации...";

            const currentVideoId = history[currentHistoryIndex].videoId;
            if (currentVideoId) {
                const args = { query: "", radioId: currentVideoId, direction: 1 };
                try {
                    const info = await window.__TAURI__.core.invoke('stream_audio', args);
                    await window.__TAURI__.core.invoke('play_audio', { filePath: info.path });
                    nowPlayingTitle.textContent = info.title;
                    setPlayingState(true);
                } catch(e) {
                    nowPlayingTitle.textContent = "Ничего не играет";
                }
            } else {
                setPlayingState(false);
                nowPlayingTitle.textContent = "Ничего не играет";
            }
            isAutoPlaying = false;
        }
    } catch (e) { }
}, 2000);