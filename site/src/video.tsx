// The intro video, in an overlay opened from the hero. It never autoplays and
// loads nothing until opened. Until the final export is in public/video/, the
// overlay shows the poster and a note.

/** Flip to true once public/video/clove-intro.mp4 (and .vtt) are in. */
const READY = false;
const SRC = "/video/clove-intro.mp4";
const CAPTIONS = "/video/clove-intro.vtt";
const POSTER = "/video/poster.jpg";
const LENGTH = "2:50";

export function WatchButton() {
  return (
    <button
      type="button"
      data-watch
      hidden
      className="inline-flex cursor-pointer items-center gap-3 rounded-md border border-rule bg-[rgba(233,238,232,0.04)] px-4 py-3 font-mono text-[0.8rem] text-ink-2 transition-colors hover:border-bottle/50 hover:text-ink"
    >
      <span className="flex h-6 w-6 items-center justify-center rounded-full bg-bottle">
        {/* A triangle's visual centre sits left of its box centre; nudge it right. */}
        <svg width="9" height="10" viewBox="0 0 9 10" aria-hidden="true" style={{ marginLeft: 2 }}>
          <path
            d="M0.5 0.8v8.4a0.6 0.6 0 0 0 0.9 0.5l7-4.2a0.6 0.6 0 0 0 0-1L1.4 0.3a0.6 0.6 0 0 0-0.9 0.5z"
            fill="var(--paper)"
          />
        </svg>
      </span>
      Watch the intro <span className="text-dim">· {LENGTH}</span>
    </button>
  );
}

export function VideoDialog() {
  return (
    <dialog data-video-dialog className="video-dialog" aria-label="clove intro video">
      <div className="video-frame">
        {READY ? (
          <video controls preload="none" poster={POSTER} playsInline>
            <source src={SRC} type="video/mp4" />
            <track default kind="captions" srcLang="en" label="English" src={CAPTIONS} />
          </video>
        ) : (
          <div className="relative">
            <img src={POSTER} alt="" className="block w-full" />
            <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-[rgba(0,0,0,0.75)] to-transparent px-6 pt-16 pb-5 font-mono text-[0.85rem] text-ink-2">
              The intro video lands here soon.
            </div>
          </div>
        )}
      </div>
      <button type="button" data-video-close className="video-close" aria-label="Close">
        ×
      </button>
    </dialog>
  );
}
