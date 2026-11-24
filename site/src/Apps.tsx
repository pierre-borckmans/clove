import { Prose, Section } from "./components";
import tuiMain from "./tui-main_screen.txt?raw";
import tuiNarrow from "./tui-narrow_screen.txt?raw";

// Colors a ratatui text snapshot at build time: frames dim, status dots and
// bars in the CLI's colors. Runs of the same class become one span.
function classOf(ch: string): string {
  const c = ch.codePointAt(0) ?? 0;
  if (c >= 0x2500 && c <= 0x257f) return "text-[#a99d8c]";
  if (c >= 0x2580 && c <= 0x259f) return "text-[#e8936a]";
  if (ch === "◆") return "text-[#e8936a]";
  if (ch === "●") return "text-[#9cc2a2]";
  if (ch === "○") return "text-term-dim";
  return "";
}

function Screen({ text }: { text: string }) {
  const runs: { cls: string; s: string }[] = [];
  for (const ch of text.trimEnd()) {
    const cls = classOf(ch);
    const last = runs.at(-1);
    if (last && last.cls === cls) last.s += ch;
    else runs.push({ cls, s: ch });
  }
  return (
    <>
      {runs.map((r, i) =>
        r.cls ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: static text runs
          <span key={i} className={r.cls}>
            {r.s}
          </span>
        ) : (
          r.s
        ),
      )}
    </>
  );
}

/** The real `clove ui` main screen, from the TUI's snapshot tests. */
export function TuiScreen() {
  const pre =
    "overflow-x-auto rounded-lg border border-term-rule bg-term px-3 py-3 font-mono leading-[1.18] text-term-ink";
  return (
    <figure className="xl:-ml-[13rem]">
      <pre className={`${pre} hidden text-[0.8rem] xl:block`}>
        <Screen text={tuiMain} />
      </pre>
      <pre className={`${pre} text-[0.62rem] sm:text-[0.72rem] xl:hidden`}>
        <Screen text={tuiNarrow} />
      </pre>
      <figcaption className="mt-3 text-[0.9rem] text-muted">
        The main screen, as rendered by the TUI's snapshot tests, with their fixture data. The
        layout drops the detail pane on narrow terminals, as here on small screens.
      </figcaption>
    </figure>
  );
}

/** Two images per shot; CSS shows the one matching the active theme. */
function Shot({
  name,
  alt,
  width,
  height,
  className = "",
}: {
  name: string;
  alt: string;
  width: number;
  height: number;
  className?: string;
}) {
  const img = "block h-auto w-full rounded-lg border border-rule";
  return (
    <div className={className}>
      <img
        src={`/desktop/${name}-light.png`}
        alt={alt}
        width={width}
        height={height}
        loading="lazy"
        className={`only-light ${img}`}
      />
      <img
        src={`/desktop/${name}-dark.png`}
        alt={alt}
        width={width}
        height={height}
        loading="lazy"
        className={`only-dark ${img}`}
      />
    </div>
  );
}

export function Desktop() {
  return (
    <Section id="app" label="the mac app" title="The same clones, in a window.">
      <Prose>
        <p>
          A macOS app built with Tauri on the same Rust core as the CLI. It has a window per repo
          with the golden image and its clones, a menu-bar popover for jumping into a clone, and a{" "}
          <code>⌘K</code> palette. No download yet: build it from source with{" "}
          <code>pnpm tauri build</code> in <code>apps/desktop</code>.
        </p>
      </Prose>
      <figure className="mt-10 xl:-ml-[13rem]">
        <div className="grid grid-cols-[minmax(0,1fr)] items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,1fr)]">
          <Shot
            name="repo"
            alt="The clove app's repo window: golden image details and a list of clones"
            width={1600}
            height={1031}
          />
          <Shot
            name="popover"
            alt="The clove menu-bar popover listing clones per repo"
            width={360}
            height={520}
            className="mx-auto w-full max-w-[16rem] md:max-w-none"
          />
        </div>
        <figcaption className="mt-3 text-[0.9rem] text-muted">
          Repo window and menu-bar popover, with sample data.
        </figcaption>
      </figure>
    </Section>
  );
}
