# now-playing

Publica la sesión multimedia activa del sistema como eventos `now_playing` y muestra un overlay transparente para OBS. Detecta fuentes que expongan una sesión del SO, como Spotify, Firefox/Chromium y VLC. En macOS v1 sólo integra Spotify y Music.

## Compatibilidad

- **Linux:** reproductores MPRIS mediante el D-Bus de sesión; zbus es Rust puro y no necesita `libdbus`.
- **Windows 10 1809+:** sesiones GSMTC, incluida la miniatura convertida a data URI.
- **macOS:** Spotify y Music mediante JXA/Apple Events. Puede solicitar permiso de Automatización. No detecta navegadores; MediaRemote está restringido desde macOS 15.4 y no se usa.

## Instalación

Añade el repositorio a `config.toml` de streams-tools para instalar el plugin cuando falte:

```toml
[plugins]
install = ["https://github.com/belcaik/streams-plugin-now-playing"]
```

También puedes instalarlo desde la línea de comandos:

```sh
streams-tools plugin add https://github.com/belcaik/streams-plugin-now-playing
```

Ambos métodos descargan el ZIP apropiado para la plataforma y verifican su SHA-256. Para fijar la versión usa `https://github.com/belcaik/streams-plugin-now-playing@v0.1.0`.

## Fuente de navegador OBS

Añade un Browser Source con URL `http://127.0.0.1:7777/p/now-playing/`. El fondo es transparente. Un tamaño inicial razonable es 420×160 píxeles, pero puedes ajustarlo a tu escena y a `--st-now-playing-width`.

## Configuración

No edites `plugin.toml`: una actualización reemplaza la carpeta del plugin. Declara sólo tus cambios en `[plugins.config.now-playing]` dentro de `config.toml` de streams-tools. El host hace deep merge sobre los defaults `[config]` del manifiesto; las tablas se mezclan recursivamente y los arrays se reemplazan completos.

| Clave | Tipo | Default | Comportamiento |
|---|---|---|---|
| `poll_ms` | entero | `1000` | `0` equivale a 1000; se limita a 100–60000 ms. |
| `priority` | lista | `spotify, firefox, chromium, vlc, music` | Orden de preferencia dentro del mismo estado. |
| `ignore` | lista | `[]` | Excluye fuentes aunque estén en `priority`. |

Las listas se normalizan una vez a minúsculas ASCII y sin espacios externos. Primero gana `playing` sobre `paused`, después `priority`, el orden estable de descubrimiento y finalmente el id estable. Las fuentes no listadas van detrás de las listadas.

```toml
[plugins.config.now-playing]
poll_ms = 2000
priority = ["spotify", "firefox", "vlc"]
ignore = ["chromium"]
```

## Contrato del evento

stdout contiene una línea JSON por cambio relevante; los diagnósticos van a stderr. El host añade `id`, `ts` y `platform`.

| Campo | Tipo | Default/ausencia |
|---|---|---|
| `status` | string | `playing`, `paused` o `stopped`. |
| `title`, `artist`, `album` | string | Cadena vacía. |
| `art_url` | string o null | `null`; HTTPS/data URI según plataforma. |
| `duration_ms`, `position_ms` | entero | `0`; la posición se satura a una duración positiva. |
| `source` | string | Id corto de la sesión o cadena vacía al detenerse. |

```json
{"type":"now_playing","data":{"status":"playing","title":"Song","artist":"Artist","album":"Album","art_url":"https://example/art.jpg","duration_ms":180000,"position_ms":42000,"source":"spotify"}}
{"type":"now_playing","data":{"status":"stopped","title":"","artist":"","album":"","art_url":null,"duration_ms":0,"position_ms":0,"source":""}}
```

## Estilos

No edites el plugin. Crea `styles/now-playing.css` en la raíz de streams-tools (o en `STREAMS_STYLES_DIR`) y recarga el Browser Source. El host lo sirve como `./custom.css`, la última hoja, por lo que tus reglas ganan la cascada.

| Variable en `.st-now-playing` | Default | Efecto |
|---|---|---|
| `--st-now-playing-font-family` | `system-ui, sans-serif` | Tipografía. |
| `--st-now-playing-width` | `420px` | Anchura. |
| `--st-now-playing-padding` | `16px` | Relleno. |
| `--st-now-playing-gap` | `12px` | Separación. |
| `--st-now-playing-background` | `rgba(20,20,24,.92)` | Fondo. |
| `--st-now-playing-color` | `#fff` | Texto principal. |
| `--st-now-playing-muted-color` | `#b8b8c0` | Metadatos y progreso vacío. |
| `--st-now-playing-accent-color` | `#1db954` | Progreso. |
| `--st-now-playing-art-size` | `96px` | Lado de la carátula. |
| `--st-now-playing-title-size` | `20px` | Tamaño del título. |
| `--st-now-playing-meta-size` | `14px` | Tamaño de metadatos/tiempo. |
| `--st-now-playing-progress-height` | `4px` | Alto del progreso. |
| `--st-now-playing-radius` | `12px` | Radio general. |
| `--st-now-playing-shadow` | `0 8px 30px rgba(0,0,0,.35)` | Sombra. |
| `--st-now-playing-transition-duration` | `200ms` | Duración de transiciones. |
| `--st-now-playing-stopped-opacity` | `0` | Opacidad al detenerse. |
| `--st-now-playing-stopped-transform` | `translateY(8px)` | Transformación al detenerse. |
| `--st-now-playing-progress` | `0` | Fracción 0–1; sólo la modifica JS. |

| Selector | Uso |
|---|---|
| `.st-now-playing` | Raíz. |
| `.st-now-playing--playing/--paused/--stopped` | Estado activo, nunca acumulado. |
| `.st-now-playing__art` | Carátula decorativa. |
| `.st-now-playing__title/__artist/__album/__source` | Textos. |
| `.st-now-playing__progress` | Contenedor del progreso. |
| `.st-now-playing__progress-bar` | Relleno del progreso. |
| `.st-now-playing__time` | Tiempo actual y duración. |

Ejemplo **minimal**, sin arte, fondo ni sombra:

```css
.st-now-playing {
  --st-now-playing-width: 300px;
  --st-now-playing-padding: 6px;
  --st-now-playing-background: transparent;
  --st-now-playing-shadow: none;
  --st-now-playing-title-size: 16px;
  --st-now-playing-meta-size: 12px;
}
.st-now-playing__art { display: none; }
```

Ejemplo **vinilo**, con arte circular girando sólo durante la reproducción:

```css
.st-now-playing {
  --st-now-playing-background: #17131f;
  --st-now-playing-accent-color: #d7a4ff;
  --st-now-playing-shadow: 0 10px 40px rgba(90, 40, 120, 0.5);
  --st-now-playing-radius: 18px;
}
.st-now-playing__art { border-radius: 50%; }
.st-now-playing--playing .st-now-playing__art {
  animation: st-now-playing-spin 8s linear infinite;
}
@keyframes st-now-playing-spin {
  to { transform: rotate(360deg); }
}
```

## Limitaciones

- Linux acepta arte HTTPS/data y archivos locales de hasta 1 MiB convertidos a data URI; descarta HTTP y esquemas desconocidos.
- Windows lee miniaturas de hasta 1 MiB y las cachea por pista; no inventa URLs locales.
- macOS no incluye navegadores ni carátula. Cada tick crea un `osascript`; aumentar `poll_ms` reduce ese coste. No hay timeout portable en v1.
- Títulos, artistas, tiempos y carátulas dependen de lo que publique cada reproductor. La primera consulta macOS puede requerir permiso de Automatización.

## Desarrollo

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
stat -c '%s bytes' target/release/now-playing
ldd target/release/now-playing
{ printf '%s\n' '{"type":"init","config":{},"data_dir":"."}'; sleep 15; } | \
  /usr/bin/time -v target/release/now-playing
```

Sólo se usan serde/serde_json para datos y base64 para imágenes; zbus es target-only en Linux y los bindings WinRT `windows` son target-only en Windows. macOS usa herramientas del sistema. No hay runtime async ni cliente HTTP. Los tests no requieren reproductores. Los commits usan Conventional Commits con descripciones en español, son atómicos y deben pasar fmt, Clippy y tests antes de cada commit.
