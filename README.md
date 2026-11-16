# WuWa Local Tracker

[English](#english) · [Русский](#русский)

## English

A local-first Wuthering Waves convene history tracker built with Rust, Tauri 2 and SQLite. The app stores pulls on your computer and fetches new records directly from Kuro's game API. Saved history works offline. This is an independent fan project, not affiliated with Kuro Games.

### Getting started

Open Convene History in the game and let it load. In the tracker, select **Find in Steam**, then **Update**, or paste a history URL manually. The URL contains a temporary access credential: never share it. Automatic discovery scans all Steam libraries (`libraryfolders.vdf`, Flatpak/Snap, external/SD mounts, Proton `compatdata`), as well as Heroic, Lutris, Bottles, and Epic/Windows installations. For custom installations, use the manual URL field. Fetching new records requires internet; the game may only provide a limited window of history.

Back up or restore all accounts with JSON export/import. History and backups are stored locally. Beginner banners are hidden from the statistics and history, but existing records are preserved in backups.

The current counter is the number of recorded pulls since the latest 5★. If the stored history has no 5★ for that banner, the actual counter may be higher. The featured-character 50/50 guarantee is not calculated. Item names remain in the language returned by the game API. API records have no unique pull ID; repeated pulls at the same timestamp are distinguished by order.

### Build

Install Rust and [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/), then run `cargo run --locked` or `cargo build --release --locked`. Node.js is not required. Linux x86-64 packages are published in GitHub Releases: AppImage for many distributions and .deb for Debian/Ubuntu. AppImage requires FUSE support; the .deb installs its required system libraries. Windows support is planned separately.

The database is in the operating system's app-data directory under `local.wuwa.tracker/history.sqlite3` (Linux: `~/.local/share/local.wuwa.tracker/history.sqlite3`).

### Arch Linux / CachyOS

Download `wuwa-local-tracker-0.2.1-1-x86_64.pkg.tar.zst` from [Releases](https://github.com/bromhobit/wuwa-local-tracker/releases/tag/v0.2.1), then run:

```bash
sudo pacman -U ./wuwa-local-tracker-0.2.1-1-x86_64.pkg.tar.zst
```

Stock SteamOS has a read-only system partition; AppImage is the easier option there.

## Русский

Локальный трекер истории круток Wuthering Waves на Rust, Tauri 2 и SQLite. Архив хранится на компьютере; новые записи загружаются напрямую из игрового API Kuro. Сохранённая история доступна без интернета. Это независимый фанатский проект, не связанный с Kuro Games.

Откройте в игре историю круток и дождитесь загрузки. В трекере нажмите «Найти в Steam» и «Обновить» либо вставьте ссылку вручную. Ссылка содержит временный ключ доступа — никому её не передавайте. Автопоиск проверяет все библиотеки Steam (`libraryfolders.vdf`, Flatpak/Snap, внешние диски/SD-карты, Proton `compatdata`), а также Heroic, Lutris, Bottles и установки Epic/Windows; для нестандартных установок используйте ручной ввод. Для новых записей нужен интернет; игра может возвращать историю только за ограниченный период.

Экспорт и импорт JSON позволяют сохранить и восстановить все аккаунты. Баннеры новичка скрыты из интерфейса и статистики, но ранее сохранённые записи остаются в резервной копии. При неполной истории счётчик может быть ниже реального; гарантия ивентового персонажа после 50/50 пока не определяется.

Для сборки установите Rust и [зависимости Tauri 2](https://v2.tauri.app/start/prerequisites/), затем выполните `cargo run --locked` или `cargo build --release --locked`. Node.js не нужен. В GitHub Releases доступны пакеты для Linux x86-64: AppImage для разных дистрибутивов и .deb для Debian/Ubuntu. Для AppImage необходима поддержка FUSE; .deb устанавливает необходимые системные библиотеки. Поддержка Windows планируется отдельно.

### Arch Linux / CachyOS

Скачайте `wuwa-local-tracker-0.2.1-1-x86_64.pkg.tar.zst` из [релиза](https://github.com/bromhobit/wuwa-local-tracker/releases/tag/v0.2.1) и установите:

```bash
sudo pacman -U ./wuwa-local-tracker-0.2.1-1-x86_64.pkg.tar.zst
```

На обычной SteamOS системный раздел защищён от записи, поэтому удобнее использовать AppImage.


### Плагин Decky Loader для Steam Deck

`WuWa-Tracker-Decky-0.2.1.zip` в релизе — это плагин Decky Loader для Steam Deck. Он позволяет просматривать и обновлять историю круток Wuthering Waves в игровом режиме. Приложение и плагин используют общую локальную историю. Показатель 0 означает отсутствие сохранённых записей и не доказывает отсутствия круток в игре.
