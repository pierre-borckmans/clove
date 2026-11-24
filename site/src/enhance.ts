// Progressive enhancement for the prerendered page: copy buttons, the theme
// toggle and the terminal replay. Everything here is optional.

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function copyButtons() {
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-copy]")) {
    if (!navigator.clipboard) continue;
    b.hidden = false;
    b.addEventListener("click", async () => {
      await navigator.clipboard.writeText(b.dataset.copy ?? "");
      b.textContent = "copied";
      setTimeout(() => {
        b.textContent = "copy";
      }, 1500);
    });
  }
}

function themeToggle() {
  const b = document.querySelector<HTMLButtonElement>("[data-theme-toggle]");
  if (!b) return;
  const root = document.documentElement;
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const current = () => root.dataset.theme ?? (media.matches ? "dark" : "light");
  const label = () => {
    b.textContent = current() === "dark" ? "light" : "dark";
  };
  b.hidden = false;
  label();
  media.addEventListener("change", label);
  b.addEventListener("click", () => {
    const next = current() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    try {
      localStorage.setItem("theme", next);
    } catch {}
    label();
  });
}

function replay() {
  const term = document.querySelector<HTMLElement>("[data-replay]");
  if (!term) return;
  const lines = [...term.querySelectorAll<HTMLElement>("[data-r]")];
  const original = lines.map((l) => l.innerHTML);
  const clock = term.querySelector<HTMLElement>("[data-clock]");
  const button = term.querySelector<HTMLButtonElement>("[data-replay-button]");
  let run = 0;

  async function play() {
    const id = ++run;
    const live = () => id === run;
    lines.forEach((l, i) => {
      l.innerHTML = original[i];
      l.style.visibility = "hidden";
    });
    if (clock) clock.textContent = "";
    let started = 0;
    let ticker = 0;
    const stopClock = () => cancelAnimationFrame(ticker);
    const tick = () => {
      if (clock) clock.textContent = `${((performance.now() - started) / 1000).toFixed(2)}s`;
      ticker = requestAnimationFrame(tick);
    };

    await sleep(400);
    for (const [i, l] of lines.entries()) {
      if (!live()) return stopClock();
      const kind = l.dataset.r;
      if (kind === "cmd") {
        const cmd = l.querySelector<HTMLElement>("[data-cmd]");
        const text = cmd?.textContent ?? "";
        if (cmd) cmd.textContent = "";
        const cursor = document.createElement("span");
        cursor.className = "term-cursor";
        cmd?.after(cursor);
        l.style.visibility = "visible";
        await sleep(350);
        for (const ch of text) {
          if (!live()) return;
          if (cmd) cmd.textContent += ch;
          await sleep(28 + Math.random() * 40);
        }
        await sleep(300);
        cursor.remove();
      } else if (kind === "step") {
        if (!started) {
          started = performance.now();
          tick();
        }
        const ms = Number(l.dataset.ms);
        l.innerHTML = `  <span style="color:#e8936a">◌</span> <span class="text-term-dim">${l.dataset.step}</span>\n`;
        l.style.visibility = "visible";
        await sleep(ms);
        l.innerHTML = original[i];
      } else if (kind === "show") {
        const ready = l.textContent?.includes("ready in");
        await sleep(Number(l.dataset.wait ?? 60));
        if (ready && started) {
          stopClock();
          if (clock) clock.textContent = "";
        }
        l.style.visibility = "visible";
      } else {
        await sleep(200);
        l.style.visibility = "visible";
      }
    }
  }

  if (button) {
    button.hidden = false;
    button.addEventListener("click", () => void play());
  }
  if (reduced()) return;
  const io = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        void play();
      }
    },
    { threshold: 0.4 },
  );
  io.observe(term);
}

// Full-page screenshots never scroll, so lazy images would stay blank there.
function eagerImages() {
  if (!new URLSearchParams(location.search).has("eager")) return;
  for (const img of document.querySelectorAll<HTMLImageElement>("img[loading=lazy]")) {
    img.loading = "eager";
  }
}

export function enhance() {
  eagerImages();
  copyButtons();
  themeToggle();
  replay();
}
