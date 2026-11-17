# AUR-пакет `slimblade`

Здесь лежит [`PKGBUILD`](PKGBUILD) и [` .SRCINFO`](.SRCINFO) для публикации
пакета в [Arch User Repository](https://aur.archlinux.org/).

## Состояние

Имя `slimblade` в AUR свободно (проверено через RPC). Пакет собран и
проверен локально командой `makepkg` — `.pkg.tar.zst` содержит CLI, GUI,
desktop-файлы, systemd unit, udev-правило, иконку и документацию.

## Публикация

1. Добавьте SSH-ключ в аккаунт AUR (https://aur.archlinux.org/account/) и
   примите ключ хоста:

   ```bash
   ssh-keyscan aur.archlinux.org >> ~/.ssh/known_hosts
   ```

2. Склонируйте пустой AUR-репозиторий (создаётся при первой отправке):

   ```bash
   git clone ssh://aur@aur.archlinux.org/slimblade.git ~/aur/slimblade
   ```

3. Скопируйте файлы отсюда и отправьте:

   ```bash
   cp /home/iolo/kensington/aur/PKGBUILD /home/iolo/kensington/aur/.SRCINFO ~/aur/slimblade/
   cd ~/aur/slimblade
   git add PKGBUILD .SRCINFO
   git commit -m "Add slimblade 0.1.0"
   git push
   ```

   При первом пуше AUR сам создаст пакетный репозиторий.

## Обновление версии

При новом релизе:

1. Обновите `pkgver` в `PKGBUILD`.
2. Пересчитайте checksum:

   ```bash
   curl -fsSL -o /tmp/slimblade-$pkgver.tar.gz \
     https://github.com/ioloryn/slimblade/archive/refs/tags/v$pkgver.tar.gz
   sha256sum /tmp/slimblade-$pkgver.tar.gz
   ```

3. Замените `sha256sums` в `PKGBUILD`.
4. Сгенерируйте `.SRCINFO`:

   ```bash
   cd /home/iolo/kensington/aur
   makepkg --printsrcinfo > .SRCINFO
   ```

5. Проверьте сборку: `makepkg` (опционально `makepkg -s` для установки зависимостей).
6. Скопируйте оба файла в AUR-репозиторий и запушьте.

## Проверка перед публикацией

```bash
cd /home/iolo/kensington/aur
makepkg --printsrcinfo > .SRCINFO   # .SRCINFO должен совпадать с PKGBUILD
makepkg                             # успешная сборка пакета
namcap PKGBUILD                     # если установлен namcap
```
