import { fade } from "@remotion/transitions/fade";
import { linearTiming, TransitionSeries } from "@remotion/transitions";
import { AbsoluteFill, Audio, Composition, interpolate, Sequence, staticFile, useCurrentFrame } from "remotion";
import durations from "./durations.json";
import script from "./script.json";
import { Again, Clash, Cost, Detect, Empty, Fast, Features, Fresh, Golden, Hacks, Internals, Intro as IntroScene, Logo, Outro, Parallel, Steps, Surfaces, Worktree } from "./scenes";
import { Backdrop, LEAD, SceneLength } from "./ui";

const FPS = 30;
/** Frames two scenes overlap while one fades into the next. */
const XFADE = 14;
/** Breathing room after each line, in seconds, where the visual needs it. */
const TAIL: Record<string, number> = {
  intro: 1.4,
  parallel: 1.0,
  clash: 1.6,
  hacks: 1.8,
  again: 1.6,
  worktree: 1.4,
  empty: 2.2,
  cost: 1.8,
  logo: 1.8,
  golden: 1.8,
  internals: 2.2,
  fast: 2.4,
  steps: 1.6,
  detect: 2.0,
  fresh: 2.0,
  surfaces: 2.0,
  features: 2.2,
  outro: 3.4,
};
const SCENES: Record<string, () => React.JSX.Element> = {
  intro: IntroScene,
  parallel: Parallel,
  clash: Clash,
  hacks: Hacks,
  again: Again,
  worktree: Worktree,
  empty: Empty,
  cost: Cost,
  logo: Logo,
  golden: Golden,
  internals: Internals,
  fast: Fast,
  steps: Steps,
  detect: Detect,
  fresh: Fresh,
  surfaces: Surfaces,
  features: Features,
  outro: Outro,
};

const lengths = script.map(({ id }) => {
  const d = (durations.durations as Record<string, number>)[id] ?? 3;
  return { id, frames: LEAD + Math.round((d + (TAIL[id] ?? 1.4)) * FPS) + XFADE };
});
const total = lengths.reduce((a, l) => a + l.frames, 0) - XFADE * (lengths.length - 1);

/** Frames of fade from and to black at the very start and end. */
const FADE_IN = 18;
const FADE_OUT = 36;

/** Where the voice speaks, in frames: [start, end) per scene. */
const speech: [number, number][] = (() => {
  let start = 0;
  return lengths.map(({ id, frames }) => {
    const d = (durations.durations as Record<string, number>)[id] ?? 3;
    const s = start + LEAD;
    start += frames - XFADE;
    return [s, s + Math.round(d * 30)] as [number, number];
  });
})();

/** Music volume: low under the voice, up in the gaps, faded in and out. */
const MUSIC_UP = 0.16;
const MUSIC_UNDER = 0.055;
const DUCK = 9;
function musicVolume(f: number): number {
  let duck = 0;
  for (const [a, b] of speech) {
    if (f >= a && f < b) {
      duck = 1;
      break;
    }
    const gap = f < a ? a - f : f - b + 1;
    duck = Math.max(duck, 1 - gap / DUCK);
  }
  const v = MUSIC_UP + (MUSIC_UNDER - MUSIC_UP) * Math.max(0, duck);
  const edge = interpolate(f, [0, 20, total - 50, total - 1], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return v * edge;
}

function Intro({ music }: { music?: string }) {
  const frame = useCurrentFrame();
  const edge = interpolate(frame, [0, FADE_IN, total - FADE_OUT, total - 1], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#000" }}>
    <AbsoluteFill style={{ opacity: edge }}>
      <Backdrop />
      {music && <Audio src={staticFile(`music/${music}.mp3`)} volume={musicVolume} />}
      <TransitionSeries>
        {lengths.flatMap(({ id, frames }, i) => {
          const S = SCENES[id];
          const scene = (
            <TransitionSeries.Sequence key={id} durationInFrames={frames} name={id}>
              <SceneLength.Provider value={frames}>
                <S />
              </SceneLength.Provider>
              <Sequence from={LEAD} durationInFrames={frames - LEAD} layout="none">
                <Audio src={staticFile(`vo/${id}.wav`)} />
              </Sequence>
            </TransitionSeries.Sequence>
          );
          return i === 0
            ? [scene]
            : [<TransitionSeries.Transition key={`${id}-in`} presentation={fade()} timing={linearTiming({ durationInFrames: XFADE })} />, scene];
        })}
      </TransitionSeries>
    </AbsoluteFill>
    </AbsoluteFill>
  );
}

export function Root() {
  const common = { component: Intro, durationInFrames: total, fps: FPS, width: 1920, height: 1080 } as const;
  return (
    <>
      <Composition id="Intro" {...common} defaultProps={{ music: "warm" }} />
      <Composition id="Intro-piano" {...common} defaultProps={{ music: "piano" }} />
      <Composition id="Intro-tech" {...common} defaultProps={{ music: "tech" }} />
      <Composition id="Intro-silent" {...common} defaultProps={{ music: "" }} />
    </>
  );
}
