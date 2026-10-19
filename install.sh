#!/bin/bash

# Скрипт установки PyTify (AppImage) для Linux
# Использование: ./install.sh

set -e

# Цвета для вывода
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Установка PyTify ===${NC}"

# 1. Ищем AppImage в текущей папке
APPIMAGE=$(ls pytify-tauri_*.AppImage 2>/dev/null | head -n 1)
if [ -z "$APPIMAGE" ]; then
    echo -e "${RED}Ошибка: Файл .AppImage не найден в текущей папке.${NC}"
    echo "Пожалуйста, запустите скрипт в той же папке, куда вы скачали AppImage."
    exit 1
fi

echo -e "Найден файл: ${GREEN}$APPIMAGE${NC}"

# 2. Создаем папку для приложений пользователя
APP_DIR="$HOME/Applications"
mkdir -p "$APP_DIR"

# 3. Копируем AppImage и делаем его исполняемым
APPIMAGE_PATH="$APP_DIR/PyTify.AppImage"
cp "$APPIMAGE" "$APPIMAGE_PATH"
chmod +x "$APPIMAGE_PATH"
echo -e "Приложение скопировано в: ${GREEN}$APPIMAGE_PATH${NC}"

# 4. Ищем иконку
# В AppImage часто есть встроенная иконка, но мы можем взять её из исходников, если она есть.
# Если вы запускаете скрипт из папки проекта, можно скопировать локальную иконку.
ICON_SRC="src-tauri/icons/128x128.png"
ICON_PATH="$APP_DIR/PyTify.png"

if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$ICON_PATH"
    echo -e "Иконка скопирована из исходников проекта."
else
    # Если иконки нет, попробуем извлечь из AppImage (не всегда работает, но пробуем)
    echo "Иконка не найдена в исходниках. Используем стандартную."
    ICON_PATH="" # Оставляем пустым, ярлык будет без иконки или с системной
fi

# 5. Создаем .desktop файл
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
DESKTOP_FILE="$DESKTOP_DIR/pytify.desktop"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=PyTify
Comment=YouTube/Spotify Music Player
Exec=$APPIMAGE_PATH
Icon=$ICON_PATH
Terminal=false
Categories=AudioVideo;Audio;Player;
EOF

echo -e "Ярлык создан: ${GREEN}$DESKTOP_FILE${NC}"

# 6. Обновляем базу данных меню
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$DESKTOP_DIR"
fi

echo -e "${GREEN}=== Установка завершена! ===${NC}"
echo "Теперь вы можете найти PyTify в меню приложений или запустить его командой:"
echo -e "${BLUE}~/Applications/PyTify.AppImage${NC}"