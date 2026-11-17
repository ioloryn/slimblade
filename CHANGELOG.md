# Changelog

Все значимые изменения проекта фиксируются в этом файле.
Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
версии следуют [Semantic Versioning](https://semver.org/lang/ru/).

## [0.1.0] — 2026-11-18

### Добавлено

- CLI `slimblade`: диагностика, монитор событий, список устройств, udev-правило.
- Демон с переназначением кнопок, `drag_lock`, клавиатурными сочетаниями и
  запуском команд.
- Инверсия и дробный множитель скорости прокрутки.
- Горячая перезагрузка конфигурации по `SIGHUP` и автопереподключение устройства.
- GUI `slimblade-gui` на Tauri 2 с интерфейсом Material 3 (`@material/web`):
  светлая/тёмная темы, масштаб, схема кнопок, редактирование действий,
  диагностика и журнал.
- Отдельный фоновый процесс для трея и демона — корректное «сворачивание в трей»
  на Wayland.
- Systemd user service и desktop-файлы.
- GitHub Actions: CI (fmt, clippy, сборка) и релизы с пакетами `.tar.gz`,
  `.deb`, `.pkg.tar.zst`, `.rpm` и `.AppImage`.

[0.1.0]: https://github.com/ioloryn/slimblade/releases/tag/v0.1.0
