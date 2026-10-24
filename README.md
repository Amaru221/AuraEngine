# Aura Engine para Windows

Port nativo a **Windows 10/11 (x64)** de [Aura](https://github.com/antwny/aura), el motor de fondos de pantalla animados escrito en Rust para COSMIC Desktop.

Aura Engine pinta vídeos y fondos estáticos **detrás de los iconos del escritorio**, con multi-monitor real, pausa automática en juegos a pantalla completa, bandeja del sistema, atajos globales y un selector rápido (HUD) con tres estilos.

```
UI (egui)  ──►  WorkerW (capa del escritorio)  ──►  mpv --wid  ──►  vídeo
    │                    ▲
    └── named pipe ──────┘  IPC de mpv + configuración JSON
```

---

## Características

- **Vídeo animado detrás de los iconos** mediante la técnica `WorkerW`/`Progman` de Windows. Sin ventanas encima del escritorio.
- **Multi-monitor**: una instancia de mpv por pantalla, con posición, DPI y resolución independientes. Reconexión automática al conectar/desconectar pantallas o al reiniciar Explorer.
- **Fondos estáticos nativos** con `IDesktopWallpaper`, incluido restaurado del fondo original.
- **Auto-pausa inteligente**:
  - juegos/apps D3D a pantalla completa (`SHQueryUserNotificationState`),
  - escritorio cubierto por ventanas (oclusión real enumerando ventanas),
  - batería (`GetSystemPowerStatus`).
- **Audio**: volumen y mute por IPC de mpv.
- **Selector rápido (HUD)** con tres estilos: cuadrícula, cinematográfico y dock. Navegación con teclado, rueda y ratón.
- **Bandeja del sistema** con menú completo.
- **Actualizaciones automáticas** desde GitHub Releases, con verificación SHA-256 e instalación desde la propia app.
- **Temas personalizables** en Ajustes → Apariencia: Aura Oscuro, Medianoche, Claro y Personalizado, con color de acento.
- **Atajos globales** (Ctrl+Alt+…).
- **Biblioteca local** con miniaturas (mpv extrae el primer fotograma), búsqueda, filtros y favoritos.
- **Catálogos online**: Wallhaven, Bing Daily, **MotionBGS** (live wallpapers 4K/1080p) y Pexels (clave opcional).
- **Tema dinámico**: extrae el color dominante del fondo y adapta la interfaz; opcionalmente tiñe el acento de Windows (best-effort).
- **CLI completa** e **instancia única** mediante named pipe.
- **Auto-descarga de mpv** portable desde GitHub si no está instalado.
- **Rendimiento**: arranque/parada de mpv en segundo plano, detección de oclusión en un hilo aparte y miniaturas cacheadas (la UI nunca se bloquea).

---

## Descargas

Cada release publica tres artefactos:

- `aura-vX-windows-x64-setup.exe` — **instalador recomendado**: menú Inicio, acceso directo opcional, inicio con Windows opcional, entrada en Aplicaciones instaladas y desinstalador.
- `aura-vX-windows-x64.exe` — ejecutable portable.
- `aura-vX-windows-x64.zip` — paquete portable con mpv, `install.ps1`/`uninstall.ps1`, documentación y certificado público.

https://github.com/Amaru221/AuraEngine/releases

Los paquetes incluyen mpv, así que funcionan sin conexión. El `.exe` portable también descarga mpv automáticamente en el primer uso si no lo encuentra.

---

## Instalación

### Opción A — Instalador (recomendada)

Ejecuta `aura-vX-windows-x64-setup.exe`. El asistente permite:

- Instalar solo para el usuario actual (sin UAC) o para todos los usuarios.
- Crear un acceso directo en el escritorio (marcado por defecto).
- Iniciar Aura con Windows (marcado por defecto).
- Confiar en el certificado autofirmado de este build (opcional).
- Elegir el idioma (español/inglés).

El instalador crea el grupo **Aura Engine** en el menú Inicio (busca "Aura Engine"), registra la app en **Configuración → Aplicaciones → Aplicaciones instaladas** y añade un desinstalador que puede borrar también la configuración, la caché y los fondos descargados.

Instalación silenciosa:

```powershell
.\aura-v1.0.12-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

### Opción B — Portable

```powershell
cd .\dist\aura-v1.0.12-windows-x64
.\aura.exe
```

Funciona sin instalar: `mpv.exe` se detecta junto al ejecutable. La configuración se guarda en `%APPDATA%\Aura`.

### Opción C — Script de PowerShell (sin instalador)

El ZIP incluye el instalador clásico para despliegues desatendidos:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -TrustCertificate
```

Instala en `%LOCALAPPDATA%\Aura\bin` y crea un acceso directo en el menú Inicio.

Opciones: `-NoStart`, `-NoShortcut`, `-DownloadMpv`.

### Opción D — Compilar desde el código

Requisitos: **Rust stable (MSVC)** y **Visual Studio Build Tools** con el SDK de Windows.

```powershell
git clone https://github.com/Amaru221/AuraEngine.git AuraEngine
cd AuraEngine
cargo build --release
.\target\release\aura.exe
```

### Opción E — Generar instalador y ZIP

```powershell
.\scripts\build.ps1 -Sign            # ZIP portable
.\scripts\build-installer.ps1 -Sign  # instalador Inno Setup
```

Genera `dist\aura-v1.0.12-windows-x64.zip` y `dist\aura-v1.0.12-windows-x64-setup.exe`.
El instalador necesita [Inno Setup 6](https://jrsoftware.org/isinfo.php)
(`winget install --id JRSoftware.InnoSetup -e`).

---

## Uso

### Bandeja del sistema

Clic izquierdo: abrir Aura. Clic derecho: abrir, siguiente/anterior, pausar, selector rápido, detener y salir.

Al cerrar la ventana, Aura sigue en la bandeja (configurable).

### Administrador de tareas

- **Finalizar tarea** cierra Aura por completo (no la deja en la bandeja).
- **Cambiar a** trae de vuelta la ventana si estaba oculta.
- El botón **X** de la ventana sigue ocultándola en la bandeja (configurable en Ajustes).

### Desinstalar

**Configuración → Aplicaciones → Aplicaciones instaladas → Aura Engine → Desinstalar**.
El desinstalador cierra la app y pregunta si quieres borrar también `%APPDATA%\Aura` y
`%LOCALAPPDATA%\Aura` (configuración, caché, miniaturas y fondos descargados).

### CLI

```text
aura [comando]
aura <archivo> [--monitor \\.\DISPLAY1]

next                 Siguiente fondo de la biblioteca
prev                 Fondo anterior
apply <archivo>      Aplica un vídeo o imagen
pause | resume       Pausa / reanuda
toggle-pause         Alterna la pausa
stop                 Detiene y restaura el fondo original
switcher             Abre/cierra el selector rápido
show                 Muestra la ventana
settings             Abre Ajustes
library | explore | monitors | about   Abre esa sección
rescan               Reescanear la biblioteca
status               Estado actual
doctor               Diagnóstico (pantallas, mpv, capa WorkerW)
install-mpv          Descarga mpv portable
set-wallpaper <img> [índice]   Fondo estático (prueba/diagnóstico)
motionbgs <categoria>          Abre Explorar en una categoría de MotionBGS
search-motionbgs <texto> [--category id]    Busca en MotionBGS
download-motionbgs <texto> [--category id]  Descarga el primer resultado
                              ids: all, anime, games, 4k, superhero, nature
quit                 Cierra Aura
```

Si Aura no está en ejecución, los comandos que lo necesitan arrancan una instancia oculta en la bandeja.

### Atajos globales

| Acción | Atajo |
| --- | --- |
| Selector rápido | `Ctrl + Alt + W` (abrir/cerrar) |
| Siguiente | `Ctrl + Alt + →` |
| Anterior | `Ctrl + Alt + ←` |
| Pausa / reanudar | `Ctrl + Alt + Espacio` |
| Detener | `Ctrl + Alt + S` |

Los atajos globales se desactivan automaticamente mientras hay un juego o app a pantalla completa en primer plano y se reactivan al salir.
Dentro del HUD: flechas/WASD mover, `RePág`/`AvPág` paginar, `Enter` aplicar, `Espacio` pausa, `F` favorito, `Esc` cerrar. `Ctrl+Alt+W` vuelve a abrir/cerrar el HUD.

---

## Catálogos online

| Fuente | Contenido | Clave |
| --- | --- | --- |
| **MotionBGS** | Live wallpapers 4K y 1080p + categorías Anime, Games, 4K, Superhero y Nature | No |
| **Wallhaven** | Fondos estáticos UHD | Opcional (SFW sin clave) |
| **Bing Daily** | 8 fondos UHD del día | No |
| **Pexels** | Vídeos y fotos | Sí (gratuita) |

MotionBGS se obtiene mediante scraping de sus tarjetas HTML (igual que el Aura original) y la descarga usa los endpoints `/dl/4k/{id}/` y `/dl/hd/{id}/`.

Dentro de **Explorar → MotionBGS** hay chips de categoría:

- **Todos** (`motionbgs.com/`)
- **Anime** (`/tag:anime/`)
- **Games** (`/tag:games/`)
- **4K** (`/4k/`)
- **Superhero** (`/tag:superhero/`)
- **Nature** (`/tag:nature/`)

La búsqueda por texto es global; al escribir una búsqueda se vuelve a la categoría *Todos*.

---

## Cómo funciona

1. **Ventana anfitriona**: por cada monitor con vídeo se crea una ventana hija sin bordes y click-through.
2. **Capa del escritorio**: se envía `0x052C` a `Progman` para que Explorer cree la capa `WorkerW`; la ventana se reparenta a esa capa (en Windows 11 24H2+ suele ser un `WorkerW` hijo de `Progman`).
3. **mpv embebido**: se lanza `mpv.exe --wid=<HWND>` con `--loop-file=inf`, `--no-config` y `--input-ipc-server=\\.\pipe\aura-…`.
4. **IPC**: comandos JSON por named pipe. La conexión y el escritor corren en hilos aparte, así que la UI nunca espera a mpv.
5. **Pausa**: un hilo independiente consulta fullscreen, oclusión y batería; la UI solo aplica el último estado.
6. **Estáticos**: se aplican con `IDesktopWallpaper::SetWallpaper` una sola vez. No se recrean hosts ni se reaplican, evitando el bucle con `WorkerW`.

### Por qué no se reutilizó `mpvpaper`

`mpvpaper` depende de `wlr-layer-shell` (Wayland) y de sockets Unix. En Windows el equivalente es `mpv --wid` sobre `WorkerW`, que es exactamente lo que implementa este port.

---

## Configuración

| Ruta | Contenido |
| --- | --- |
| `%APPDATA%\Aura\config.json` | Configuración y asignaciones por monitor |
| `%LOCALAPPDATA%\Aura\bin\mpv.exe` | mpv portable descargado |
| `%LOCALAPPDATA%\Aura\cache\thumbs` | Miniaturas |
| `%LOCALAPPDATA%\Aura\wallpapers\online` | Descargas de los catálogos |
| `%LOCALAPPDATA%\Aura\logs` | `aura.log` y logs de mpv |

La configuración se guarda de forma atómica y se recupera automáticamente si se corrompe.

---

## Solución de problemas

**El fondo aparece por encima de los iconos**
Ajustes → Rendimiento → *Modo de composición* → **Progman**. Vuelve a aplicar el fondo.

**No se reproducen vídeos**
Ajustes → Motor: comprueba que mpv está detectado. Usa *Descargar mpv* o indica un `mpv.exe`. Revisa `%LOCALAPPDATA%\Aura\logs\mpv-*.log`.

**El fondo se pausa constantemente**
Desactiva *Pausar cuando el escritorio está tapado* o *Pausar en juegos a pantalla completa* en Ajustes → Rendimiento.

**La UI va lenta**
La versión 1.0.4+ evita el trabajo pesado por frame (versión de mpv cacheada, miniaturas con clave cacheada, sin clonar la biblioteca completa). Si persiste, revisa el consumo con:

```powershell
$p = Get-Process aura | Select-Object -First 1
$c = $p.CPU
Start-Sleep 6
$p.Refresh()
"CPU en 6 s: $([math]::Round($p.CPU - $c, 2))"
```

**Windows Defender / SmartScreen**
El ejecutable está firmado con un certificado autofirmado. Usa `install.ps1 -TrustCertificate` o compila desde el código.

---

## Desarrollo

```powershell
cargo check
cargo test
cargo build --release
```

Estructura:

```text
src/
  main.rs            arranque, CLI vs GUI, instancia única
  app.rs             estado, orquestación, IPC, auto-pausa
  cli.rs             interfaz de línea de comandos
  config.rs          configuración serde + rutas
  engine/
    mpv.rs           proceso mpv, IPC por named pipe, extracción de frames
    installer.rs     descarga/extracción de mpv portable
    mod.rs           motor multi-monitor
  platform/
    host.rs          ventanas WorkerW, reparentado, diagnóstico
    ipc.rs           servidor/cliente named pipe
    static_wallpaper.rs  IDesktopWallpaper
    win.rs           monitores, DPI, energía, fullscreen/oclusión, registro
  scanner/           escaneo de biblioteca y miniaturas
  online/
    mod.rs           Wallhaven, Bing, Pexels
    motionbgs.rs     scraping del catálogo MotionBGS
  theme.rs           paleta, fuentes Segoe UI/MDL2, estilos egui
  ui/                biblioteca, explorar, pantallas, ajustes, about, HUD
```

Tests unitarios cubren configuración, escalado, paleta, hash, rutas, extensiones y el parser de MotionBGS.

---

## Firma de código

El ejecutable incluido está firmado con un certificado **autofirmado** (`CN=Aura Engine (Self-Signed)`), SHA-256 y sello de tiempo de DigiCert.

```powershell
Get-AuthenticodeSignature .\aura.exe
# Status: Valid (si el certificado está en el almacén de confianza del usuario)
```

### Confiar en el certificado (solo para pruebas)

```powershell
.\install.ps1 -TrustCertificate
```

O manualmente:

```powershell
certutil -addstore -f -user Root .\certs\aura-selfsigned.cer
certutil -addstore -f -user TrustedPublisher .\certs\aura-selfsigned.cer
```

> **Importante:** una firma autofirmada **no elimina SmartScreen** en otros equipos. Solo es válida donde se importe el `.cer`. Para distribución pública necesitas un certificado emitido por una CA.

### Firma real para distribución

Desde 2023 las CA exigen que la clave privada resida en un token/HSM o servicio cloud:

| Opción | Coste | Notas |
| --- | --- | --- |
| **Azure Trusted Signing** | ~9,99 USD/mes | Validación de identidad; integración con GitHub Actions |
| **SignPath.io** | Gratis para OSS | Firma en CI para proyectos open source |
| **Certum / Sectigo / DigiCert** | 200–600 USD/año | OV/EV, token físico o HSM cloud |

```powershell
.\scripts\sign.ps1 -Path .\target\release\aura.exe -Thumbprint <THUMBPRINT>

# o con PFX (CI):
.\scripts\sign.ps1 -Path .\target\release\aura.exe -Pfx .\cert.pfx -PfxPassword $env:PFX_PASSWORD
```

### Quitar el certificado autofirmado

```powershell
Get-ChildItem Cert:\CurrentUser\My,Cert:\CurrentUser\Root,Cert:\CurrentUser\TrustedPublisher |
    Where-Object { $_.Subject -like "*Aura Engine (Self-Signed)*" } |
    Remove-Item
```

---

## Diferencias con Aura (Linux)

| Aura (Linux) | Aura Engine (Windows) |
| --- | --- |
| `libcosmic` (Wayland) | `egui`/`eframe` |
| `mpvpaper` + `wlr-layer-shell` | `WorkerW` + `mpv --wid` |
| Sockets Unix | Named pipes |
| `cosmic-randr` / `wlr-randr` | `EnumDisplayMonitors` |
| D-Bus GameMode / UPower | `SHQueryUserNotificationState` / `GetSystemPowerStatus` |
| `ksni` (StatusNotifier) | `tray-icon` |
| GSettings / COSMIC theme | `IDesktopWallpaper` + acento de Windows (opt-in) |
| Actualizador `.tar.gz` | ZIP + instalador PowerShell |

---

## Licencia

GPL-3.0-or-later. Ver [LICENSE](LICENSE), [NOTICE](NOTICE) y [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

Aura original: [antwny/aura](https://github.com/antwny/aura) (GPL-3.0-or-later).
Historial de cambios: [CHANGELOG.md](CHANGELOG.md).

mpv se distribuye bajo GPL/LGPL; si empaquetas `mpv.exe` con Aura, incluye su licencia y fuentes correspondientes.
