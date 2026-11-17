# Готовые заявки на публикацию пакета

Статус на 2026-11-18: регистрация новых аккаунтов в AUR **приостановлена**
разработчиками из-за волны автоматически создаваемых аккаунтов и вредоносных
пакетов. Официально: https://aur.archlinux.org/register — «New account
registration is temporarily closed». Восстановления срока не объявлено.

Поэтому пакет пока можно ставить напрямую из [релиза](https://github.com/ioloryn/slimblade/releases)
или собрать локально через `aur/PKGBUILD`. Ниже — готовые тексты, когда
публикация станет возможной.

---

## 1. AUR (когда откроют регистрацию)

Файлы уже готовы: `PKGBUILD` и `.SRCINFO`. После создания аккаунта и добавления
SSH-ключа (https://aur.archlinux.org/account/):

```bash
ssh-keyscan aur.archlinux.org >> ~/.ssh/known_hosts
git clone ssh://aur@aur.archlinux.org/slimblade.git ~/aur/slimblade
cp PKGBUILD .SRCINFO ~/aur/slimblade/
cd ~/aur/slimblade
git add . && git commit -m "Add slimblade 0.1.0" && git push
```

Официальный fingerprint хоста AUR:
`SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4` (Ed25519) — уже проверен.

---

## 2. Chaotic-AUR (заявка через GitHub Issue)

Chaotic-AUR подключён в CachyOS по умолчанию. Заявка подаётся как Issue:
https://github.com/chaotic-aur/packages/issues/new/choose →
«👶 Request to add new package».

Текст для полей формы:

**Title**
```
[Request] slimblade
```

**Package**
```
https://github.com/ioloryn/slimblade
GitHub release (исходники и готовый pkg.tar.zst):
https://github.com/ioloryn/slimblade/releases/tag/v0.1.0
PKGBUILD: https://github.com/ioloryn/slimblade/blob/main/aur/PKGBUILD

Зависимости: gtk3, webkit2gtk-4.1, libsoup3, libdbus, libevdev,
hicolor-icon-theme; makedepends: rust, cargo, pkgconf, librsvg,
libayatana-appindicator.

Примечание: заявка в AUR пока невозможна — регистрация новых аккаунтов AUR
приостановлена разработчиками (https://aur.archlinux.org/register).
PKGBUILD и .SRCINFO уже готовы и проверены локальной сборкой makepkg.
```

**Purpose**
```
Пользовательская утилита для Kensington SlimBlade (и совместимых
трекболов) на Linux: переназначение четырёх кнопок, drag_lock, горячие
клавиши, запуск команд, инверсия/масштаб прокрутки. Работает через
evdev/uinput. В комплекте CLI и GUI на Tauri 2 с интерфейсом Material 3
(@material/web) и иконкой в системном трее. Полезно владельцам SlimBlade,
которые хотят настраивать трекбол без проприетарного ПО.
```

**License**
```
MIT
```

**Checklist** — отметить все три пункта.

---

## 3. Что можно сделать прямо сейчас

- Ставить из релиза: `sudo pacman -U slimblade-0.1.0-x86_64.pkg.tar.zst`.
- Собрать локально: `git clone ... && cd slimblade/aur && makepkg -si`.
- Если у вас есть друг с аккаунтом AUR — он может залить готовые файлы из `aur/`.
