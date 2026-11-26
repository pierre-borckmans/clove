import { fade } from "@remotion/transitions/fade";
import { linearTiming, TransitionSeries } from "@remotion/transitions";
import { AbsoluteFill, Audio, Composition, Sequence, staticFile } from "remotion";
import durations from "./durations.json";
import script from "./script.json";
import { Agents, Cost, Empty, Fast, Features, Golden, Logo, Outro, Steps, Surfaces, Worktree } from "./scenes";
import { Backdrop } from "./ui";

const FPS = 30;
/** Voice starts this many frames into its scene. */
const LEAD = 10;
/** Frames two scenes overlap while one fades into the next. */
const XFADE = 14;
/** Breathing room after each line, in seconds, where the visual needs it. */
const TAIL: Record<string, number> = {
  worktree: 1.6,
  empty: 1.4,
  cost: 2.4,
  agents: 2.0,
  logo: 1.8,
  golden: 1.8,
  fast: 2.4,
  steps: 1.6,
  surfaces: 2.0,
  features: 2.2,
  outro: 3.4,
};
const SCENES: Record<string, () => React.JSX.Element> = {
  worktree: Worktree,
  empty: Empty,
  cost: Cost,
  agents: Agents,
  logo: Logo,
  golden: Golden,
  fast: Fast,
  steps: Steps,
  surfaces: Surfaces,
  features: Features,
  outro: Outro,
};

const lengths = script.map(({ id }) => {
  const d = (durations.durations as Record<string, number>)[id] ?? 3;
  return { id, frames: LEAD + Math.round((d + (TAIL[id] ?? 1.4)) * FPS) + XFADE };
});
const total = lengths.reduce((a, l) => a + l.frames, 0) - XFADE * (lengths.length - 1);

function Intro() {
  return (
    <AbsoluteFill>
      <Backdrop />
      <TransitionSeries>
        {lengths.flatMap(({ id, frames }, i) => {
          const S = SCENES[id];
          const scene = (
            <TransitionSeries.Sequence key={id} durationInFrames={frames} name={id}>
              <S />
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
  );
}

export function Root() {
  return <Composition id="Intro" component={Intro} durationInFrames={total} fps={FPS} width={1920} height={1080} />;
}
