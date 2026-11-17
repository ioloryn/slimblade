# SlimBlade

[![CI](https://github.com/ioloryn/slimblade/actions/workflows/ci.yml/badge.svg)](https://github.com/ioloryn/slimblade/actions/workflows/ci.yml)
[![Release](https://github.com/ioloryn/slimblade/actions/workflows/release.yml/badge.svg)](https://github.com/ioloryn/slimblade/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-orange.svg)](#)

Небольшая утилита на Rust для управления трекболом **Kensington SlimBlade**
(USB `047d:2041`) в Linux.

## Установка из релизов

Готовые пакеты и бинарники публикуются на странице
[Releases](https://github.com/ioloryn/slimblade/releases). Доступны:

| Формат | Что внутри | Для кого |
|---|---|---|
| `slimblade-VERSION-x86_64-unknown-linux-gnu.tar.gz` | `slimblade`, `slimblade-gui`, desktop-файлы, udev-правило, systemd unit | Universal / ручная установка |
| `slimblade_x86_64.deb` | то же, в структуре Debian | Debian, Ubuntu, CachyOS deb-окружения |
| `slimblade-x86_64.AppImage` | GUI + CLI, portable | Любой дистрибутив с FUSE |
| `slimblade-x86_64.pkg.tar.zst` | пакет Arch Linux | Arch, CachyOS, Manjaro |
| `slimblade-x86_64.rpm` | пакет RPM | Fedora, openSUSE |

Быстрая установка (замените `VERSION` на нужную, например `0.1.0`):

```bash
VERSION=0.1.0
BASE=https://github.com/ioloryn/slimblade/releases/download/v$VERSION

# Arch / CachyOS
curl -fLO $BASE/slimblade-$VERSION-x86_64.pkg.tar.zst
sudo pacman -U slimblade-$VERSION-x86_64.pkg.tar.zst

# Debian / Ubuntu
curl -fLO $BASE/slimblade_${VERSION}_amd64.deb
sudo apt install ./slimblade_${VERSION}_amd64.deb

# Fedora
curl -fLO $BASE/slimblade-$VERSION.x86_64.rpm
sudo dnf install ./slimblade-$VERSION.x86_64.rpm

# AppImage (универсально)
curl -fLO $BASE/SlimBlade-$VERSION-x86_64.AppImage
chmod +x SlimBlade-$VERSION-x86_64.AppImage
./SlimBlade-$VERSION-x86_64.AppImage
```

После установки выполните `slimblade doctor` и установите udev-правило
(см. [Права доступа](#права-доступа)).

Классический SlimBlade не имеет вендорного HID-интерфейса для настройки:
это обычная HID-мышь с четырьмя кнопками и вертикальным колесом. Поэтому
программа работает на уровне Linux input: забирает физическое устройство
через `evdev`, применяет переназначения и создаёт виртуальный указатель
через `uinput`.

## Возможности

- переназначение четырёх кнопок на другие кнопки мыши, клавиши и сочетания;
- `drag_lock` — нажали кнопку, началось перетаскивание, нажали ещё раз —
  отпустили (удобно для трекбола);
- запуск команды по нажатию кнопки;
- инверсия и множитель скорости прокрутки, включая дробные значения
  (накопление high-resolution wheel-событий);
- живой монитор событий для определения кнопок;
- горячая перезагрузка конфигурации по `SIGHUP`;
- автоматическое переподключение при выдёргивании/возврате USB;
- диагностика прав доступа и готовый udev-правило;
- systemd user service.

## Сборка из исходников

Системные зависимости для GUI (Tauri/WebKitGTK):

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  librsvg2-dev libayatana-appindicator3-dev libxdo-dev libssl-dev curl

# Arch / CachyOS
sudo pacman -S base-devel webkit2gtk-4.1 libayatana-appindicator \
  librsvg curl

# Fedora
sudo dnf install gcc gcc-c++ pkgconf-pkg-config webkit2gtk4.1-devel \
  librsvg2-devel libappindicator-gtk3-devel curl
```

CLI (`slimblade`) собирается без GUI-зависимостей:

```bash
cargo build --release --bin slimblade
sudo install -Dm755 target/release/slimblade /usr/local/bin/slimblade
```

Полная сборка (CLI + GUI):

```bash
cd frontend && npm install && npm run build && cd ..
cargo build --release
sudo install -Dm755 target/release/slimblade /usr/local/bin/slimblade
sudo install -Dm755 target/release/slimblade-gui /usr/local/bin/slimblade-gui
```

Или:

```bash
cargo install --path .
```

> Трей реализован через StatusNotifierItem (`ksni 0.3`, pure-Rust `zbus`),
> поэтому заголовки `libdbus-1-dev` для сборки не нужны. CLI `slimblade` можно
> собрать отдельно: `cargo build --release --bin slimblade`.

## Права доступа

Для работы нужны:

1. чтение `/dev/input/eventN` (физический трекбол);
2. запись в `/dev/uinput` (виртуальный указатель).

На большинстве десктопов `/dev/uinput` уже доступен активному пользователю
через `uaccess` ACL. Если нет — проверьте `slimblade doctor`.

Установите udev-правило:

```bash
slimblade udev-rule | sudo tee /etc/udev/rules.d/70-slimblade.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

После этого переподключите трекбол (или перезайдите в сессию).

Номер `70` важен: правило должно загружаться раньше `73-seat-late.rules`,
где udev применяет `uaccess` ACL. Правило с номером `99` добавляет тег
слишком поздно и не даёт доступа.

Альтернатива — добавить пользователя в группу `input`:

```bash
sudo usermod -aG input "$USER"
```

Но `uaccess` предпочтительнее: доступ получает только активная сессия.

## Быстрый старт

```bash
slimblade init        # создать ~/.config/slimblade/config.toml
slimblade doctor      # проверить устройство и права
slimblade list        # показать найденные указатели
slimblade monitor     # нажимать кнопки и смотреть события
slimblade run         # запустить демон
```

`monitor` удобно держать открытым в одном терминале, а конфиг править в
другом. Для применения изменений без остановки демона:

```bash
kill -HUP "$(pgrep -x slimblade)"
```

Если меняются **имена клавиш**, которых не было в исходной конфигурации,
нужен перезапуск: набор возможностей виртуального устройства фиксируется
при старте. Изменения кнопок/скорости/инверсии применяются на лету.

## Конфигурация

Файл по умолчанию: `$XDG_CONFIG_HOME/slimblade/config.toml`
(обычно `~/.config/slimblade/config.toml`). Пример:

```toml
[device]
vendor = 0x047d
product = 0x2041
# name = "Slimblade"
# path = "/dev/input/event17"

[scroll]
natural = false
speed = 1.0

[buttons.left]
action = "mouse"
button = "left"

[buttons.right]
action = "mouse"
button = "right"

[buttons.middle]
action = "mouse"
button = "middle"

[buttons.side]
action = "mouse"
button = "side"
```

### Действия

```toml
# Другая кнопка мыши
[buttons.side]
action = "mouse"
button = "forward"      # left, right, middle, side/back, extra/forward

# Сочетание клавиш (удерживается, пока удерживается кнопка)
[buttons.side]
action = "keys"
keys = ["ctrl", "shift", "t"]

# Drag lock: первое нажатие начинает перетаскивание, второе — заканчивает
[buttons.middle]
action = "drag_lock"
button = "left"

# Команда при нажатии (запускается через /bin/sh -c)
[buttons.left]
action = "exec"
command = "notify-send SlimBlade 'Hello'"

# Полностью отключить кнопку
[buttons.side]
action = "none"
```

Имена клавиш: `a`…`z`, `0`…`9`, `f1`…`f24`, `ctrl`, `alt`, `shift`,
`super`, `enter`, `esc`, `tab`, `backspace`, `delete`, `home`, `end`,
`pageup`, `pagedown`, `up`, `down`, `left`, `right`, `space`, `minus`,
`equal`, `comma`, `dot`, `slash`, `volumeup`, `volumedown`, `mute`,
`playpause` и другие. Можно писать канонические имена `KEY_A`, `KEY_F13`
и т.п.

### Прокрутка

- `natural = true` инвертирует направление колеса;
- `speed = 0.5` — медленнее, `speed = 2.0` — быстрее. Дробные значения
  работают за счёт накопления: половина «щелчка» колеса не теряется.

Учтите, что системная настройка natural scrolling (libinput) применяется
поверх, потому что демон работает с сырым evdev-потоком. Если хотите
управлять направлением только здесь, отключите системную инверсию для
этого устройства.

Виртуальный указатель `SlimBlade (managed)` — это новое отдельное
устройство в системе. Настройки указателя в GNOME/KDE (скорость,
ускорение, natural scrolling) могут применяться к нему независимо от
физического трекбола; при необходимости задайте их для нового устройства.

## Графический интерфейс (GUI)

`slimblade-gui` — окно настроек на **Tauri 2** с настоящим интерфейсом
**Material 3** на официальных веб-компонентах Google (`@material/web`):
filled/outlined/text-кнопки, карточки с elevation, чекбоксы, слайдеры,
поля ввода, ripple-эффекты и state-layer. Бэкенд остаётся на Rust.

```bash
slimblade-gui                # запустить трей и открыть окно настроек
slimblade-gui --hidden       # только трей и демон, без окна (для автозапуска)
slimblade-gui --config ~/.config/slimblade/config.toml
```

Иконка трея и демон живут в отдельном фоновом процессе (`--tray-only`,
pid-файл в `$XDG_RUNTIME_DIR/slimblade-gui.pid`). Это единственный надёжный
способ «свернуть в трей» на Wayland: композитёры не позволяют клиенту
скрыть своё окно (`Visible(false)` не работает, `set_minimized` часто
игнорируется, а некорректные размеры приводят к protocol error). Поэтому
кнопка «В трей» и закрытие окна просто завершают процесс окна, а иконка
трея остаётся и по клику открывает новое окно.

Тему «светлая/тёмная» и масштаб можно менять кнопками на панели инструментов;
настройки сохраняются в хранилище webview.

### Сборка GUI

Фронтенд лежит в `frontend/` и собирается в `dist/` через Vite; `dist/` уже
в репозитории, поэтому для обычной сборки Rust достаточно:

```bash
cargo build --release --bin slimblade-gui
```

Если вы правили фронтенд:

```bash
cd frontend && npm install && npm run build && cd ..
cargo build --release --bin slimblade-gui
```

Нужны WebKitGTK (`webkit2gtk-4.1`), GTK3 и Node.js только для пересборки
фронтенда.

Особенности:

- полный выход — через меню трея или кнопку «Выход» (останавливает и демон);
- демон управляется фоновым процессом: кнопки запуска/остановки и «Применить»
  отправляют ему сигналы;
- «Применить» сохраняет `config.toml` и перезапускает демон;
- при старте GUI автоматически запускает демон, если не найден другой
  экземпляр (`SlimBlade (managed)`);
- сохранение из GUI **перезаписывает** `config.toml` без комментариев — для
  ручного редактирования с комментариями используйте CLI (`slimblade init`,
  правка файла, `SIGHUP`).

### Автозапуск

Установите desktop-файлы из `packaging/`:

```bash
sudo install -Dm644 packaging/slimblade-gui.desktop /usr/share/applications/slimblade-gui.desktop
install -Dm644 packaging/slimblade-gui-autostart.desktop ~/.config/autostart/slimblade-gui.desktop
```

В GNOME для отображения трея обычно требуется расширение AppIndicator /
KStatusNotifierItem. В KDE/Plasma StatusNotifierItem поддерживается штатно,
в связке Umbriel + Noctalia трей предоставляется оболочкой Noctalia.

Если трей не подключается, точную причину можно посмотреть в строке «Трей»
окна настроек и в разделе «Журнал» — туда пишется ошибка регистрации SNI.

## systemd (пользовательский сервис)

```bash
mkdir -p ~/.config/systemd/user
cp packaging/slimblade.service ~/.config/systemd/user/
# при необходимости поправьте ExecStart
systemctl --user daemon-reload
systemctl --user enable --now slimblade
systemctl --user status slimblade
journalctl --user -u slimblade -f
```

## Как это устроено

1. Демон находит событийный узел по `vendor/product/name` из
   `/proc/bus/input/devices`.
2. `EVIOCGRAB` забирает физическое устройство, чтобы исходные события не
   попадали в систему.
3. События читаются кадрами до `SYN_REPORT`.
4. Кнопки заменяются действиями из конфига; движение мяча и остальные оси
   проходят без изменений; колесо масштабируется и при необходимости
   инвертируется.
5. Результат пишется в виртуальное uinput-устройство
   `SlimBlade (managed)`.
6. При выходе/ошибке все удерживаемые кнопки отпускаются, чтобы не
   оставить «зажатую» кнопку.

## Диагностика

```bash
slimblade doctor
slimblade check
slimblade list --all
slimblade monitor --device /dev/input/event17
```

Частые проблемы:

- `Permission denied` на `/dev/input/eventN` — не установлено udev-правило.
- `cannot open /dev/uinput` — нет прав на uinput; см. `doctor`.
- `cannot grab ... EBUSY` — устройство уже захвачено другим экземпляром
  `slimblade`, `input-remapper` или похожей программой.
- После смены `product` для SlimBlade Pro проверьте ID через
  `slimblade list --all` и укажите его в `[device]`.

## Совместимость

Основная цель — SlimBlade Trackball `047d:2041`. Утилита не завязана жёстко
на эту модель: подойдёт любой Linux-указатель с evdev-кнопками и колесом,
если задать `vendor/product/name` в конфиге. Для устройств без колеса
настройки прокрутки просто не используются.

## Участие в разработке

1. Форкните репозиторий и создайте ветку от `main`.
2. Внесите изменения; для GUI пересоберите фронтенд (`frontend/`).
3. Проверьте сборку: `cargo check --all-targets && cargo test`.
4. Откройте Pull Request с описанием изменений.

CI на каждый push и PR прогоняет `cargo fmt --check`, `cargo clippy` и
сборку обоих бинарников. Релизы создаются автоматически при push тега `v*`
(например, `git tag v0.1.0 && git push origin v0.1.0`).

## Лицензия

MIT. См. [LICENSE](LICENSE).
