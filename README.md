# Spectrum Overlay

Прозрачный аудиовизуализатор для Windows на Rust. Отображает спектр системного звука поверх других окон. При частоте дискретизации 96 кГц шкала охватывает диапазон до 48 кГц.

## Управление

Щёлкните по значку в системном трее, чтобы открыть настройки. В них можно выбрать монитор, размер окна, прозрачность и запуск при входе в Windows. Правый щелчок открывает меню для перемещения, скрытия и закрытия оверлея.

## Сборка из исходников

Для Windows x64 установите Rust stable и Visual Studio Build Tools с компонентами C++ и Windows SDK. При использовании Rust GNU сборочный скрипт также поддерживает MinGW windres. Запустите из корня проекта:

```powershell
cargo test --locked
cargo build --release --locked
```


Релизный файл для Windows — `target\release\spectrum-overlay.exe`. Чтобы получить его из текущих исходников, выполните `cargo build --release --locked`. Для распространения используйте этот файл.

Если компоновщик MinGW выдаёт ошибку `ld: cannot find` для пути с кириллицей, в PowerShell задайте каталог сборки с латинским путём, например `$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'spectrum-overlay-build'`. После сборки файл будет в `$env:CARGO_TARGET_DIR\release\spectrum-overlay.exe`.

