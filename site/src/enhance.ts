// Progressive enhancement for the prerendered page: copy buttons and the
// reveal-on-enter animations. Without it, the page shows everything, still.

function copyButtons() {
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-copy]")) {
    if (!navigator.clipboard) continue;
    b.hidden = false;
    const original = b.innerHTML;
    b.addEventListener("click", async () => {
      await navigator.clipboard.writeText(b.dataset.copy ?? "");
      b.textContent = "copied";
      setTimeout(() => {
        b.innerHTML = original;
      }, 1500);
    });
  }
}

/** Marks elements `.in` the first time they enter the viewport. Plays once;
 * it reads scroll position, never sets it. */
function reveals() {
  const els = document.querySelectorAll<HTMLElement>(
    "[data-reveal], .draw, .fill, .clash, .chips, .gauge",
  );
  if (!("IntersectionObserver" in window)) {
    for (const el of els) el.classList.add("in");
    return;
  }
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.classList.add("in");
        io.unobserve(e.target);
      }
    },
    { rootMargin: "0px 0px -12% 0px", threshold: 0.15 },
  );
  for (const el of els) io.observe(el);
}

/** Marks the page `scrolled` once the top nav is out of view, which brings in
 * the corner capsules. Reads scroll position, never sets it. */
function navState() {
  const top = document.querySelector("[data-topnav]");
  if (!top || !("IntersectionObserver" in window)) return;
  new IntersectionObserver(([e]) =>
    document.documentElement.classList.toggle("scrolled", !e.isIntersecting),
  ).observe(top);
}

/** Highlights the docs section being read in "on this page". Reads the
 * scroll position, never sets it. */
function tocSpy() {
  const links = [...document.querySelectorAll<HTMLAnchorElement>("[data-toc]")];
  if (!links.length) return;
  const heads = links
    .map((a) => document.getElementById(a.dataset.toc ?? ""))
    .filter((h): h is HTMLElement => !!h);
  let queued = false;
  const update = () => {
    queued = false;
    const line = window.innerHeight * 0.3;
    // The last heading above a line a third of the way down the screen; at the
    // very bottom of the page, the last one.
    let current = heads[0];
    for (const h of heads) if (h.getBoundingClientRect().top <= line) current = h;
    if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4)
      current = heads[heads.length - 1];
    for (const a of links) a.classList.toggle("active", a.dataset.toc === current?.id);
  };
  window.addEventListener(
    "scroll",
    () => {
      if (!queued) {
        queued = true;
        requestAnimationFrame(update);
      }
    },
    { passive: true },
  );
  update();
}

/** The intro video overlay: opens from the hero, closes the way it opened,
 * and pauses when closed. */
function videoDialog() {
  const button = document.querySelector<HTMLButtonElement>("[data-watch]");
  const dialog = document.querySelector<HTMLDialogElement>("[data-video-dialog]");
  if (!button || !dialog || typeof dialog.showModal !== "function") return;
  const video = dialog.querySelector("video");
  const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const close = () => {
    if (!dialog.open || dialog.classList.contains("closing")) return;
    video?.pause();
    if (reduced()) return dialog.close();
    dialog.classList.add("closing");
    dialog.addEventListener(
      "animationend",
      () => {
        dialog.classList.remove("closing");
        dialog.close();
      },
      { once: true },
    );
  };

  button.hidden = false;
  button.addEventListener("click", () => {
    dialog.showModal();
    video?.play().catch(() => {});
  });
  dialog.querySelector("[data-video-close]")?.addEventListener("click", close);
  // A click on the dimmed backdrop (the dialog itself, not its content) closes it.
  dialog.addEventListener("click", (e) => {
    if (e.target === dialog) close();
  });
  // Esc: animate out instead of the browser's instant close.
  dialog.addEventListener("cancel", (e) => {
    e.preventDefault();
    close();
  });
}

export function enhance() {
  videoDialog();
  tocSpy();
  copyButtons();
  reveals();
  navState();
}
