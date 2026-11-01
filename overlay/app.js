(() => {
  const root = document.querySelector(".st-now-playing");
  const art = root.querySelector(".st-now-playing__art");
  const title = root.querySelector(".st-now-playing__title");
  const artist = root.querySelector(".st-now-playing__artist");
  const album = root.querySelector(".st-now-playing__album");
  const source = root.querySelector(".st-now-playing__source");
  const time = root.querySelector(".st-now-playing__time");
  const statuses = ["playing", "paused", "stopped"];
  let current = null;
  let frame = 0;

  fetch("./config.json")
    .then((response) => response.ok ? response.json() : null)
    .catch(() => null);

  function formatTime(milliseconds) {
    const seconds = Math.max(0, Math.floor(milliseconds / 1000));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  }

  function render() {
    if (!current) return;
    const elapsed = current.status === "playing" ? performance.now() - current.receivedAt : 0;
    const duration = Math.max(0, Number(current.duration_ms) || 0);
    const rawPosition = current.status === "stopped" ? 0 : current.position_ms + elapsed;
    const position = Math.min(Math.max(0, rawPosition), duration || Number.MAX_SAFE_INTEGER);
    const progress = duration > 0 ? Math.min(1, Math.max(0, position / duration)) : 0;
    root.style.setProperty("--st-now-playing-progress", String(progress));
    time.textContent = duration > 0
      ? `${formatTime(position)} / ${formatTime(duration)}`
      : formatTime(position);
    if (current.status === "playing") frame = requestAnimationFrame(render);
  }

  window.streams.on("now_playing", (data, _event) => {
    if (!data || !statuses.includes(data.status)) return;
    cancelAnimationFrame(frame);
    current = {
      ...data,
      position_ms: Math.max(0, Number(data.position_ms) || 0),
      receivedAt: performance.now(),
    };
    root.classList.remove(...statuses.map((status) => `st-now-playing--${status}`));
    root.classList.add(`st-now-playing--${current.status}`);
    title.textContent = data.title || "";
    artist.textContent = data.artist || "";
    album.textContent = data.album || "";
    source.textContent = data.source || "";
    const artUrl = typeof data.art_url === "string" ? data.art_url : "";
    if (artUrl.startsWith("https://") || artUrl.startsWith("data:")) {
      art.src = artUrl;
      art.hidden = false;
    } else {
      art.removeAttribute("src");
      art.hidden = true;
    }
    render();
  });
})();
