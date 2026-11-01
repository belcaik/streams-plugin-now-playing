# Reglas para agentes

- Usa Conventional Commits con descripciones en español, imperativo y minúscula.
- Haz commits atómicos; cada PR admite hasta 10 archivos y 400 líneas sin contar `Cargo.lock`.
- Ejecuta `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `cargo test` antes de cada commit.
- Reserva stdout para eventos JSONL y escribe diagnósticos en stderr.
- Aísla el código de plataforma con `cfg` y dependencias target-only.
- Mantén tests puros, sin reproductores ni servicios reales.
- Conserva el protocolo, la selección determinista y el contrato JSON de `now_playing`.
- Preserva las clases BEM y variables CSS públicas; el overlay admite sólo la custom property de progreso inline.
- Los estilos de usuario viven fuera del plugin, en `styles/now-playing.css` o `STREAMS_STYLES_DIR`.
