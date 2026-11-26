// A transition between scenes: the outgoing one pushes forward, blurs and
// fades; the incoming one settles back into focus. Reads as a focus pull
// rather than two half-transparent slides on top of each other.
import type { TransitionPresentation, TransitionPresentationComponentProps } from "@remotion/transitions";
import { AbsoluteFill, Easing, interpolate } from "remotion";

type Props = Record<string, never>;

function FocusPull({ children, presentationDirection, presentationProgress }: TransitionPresentationComponentProps<Props>) {
  const p = presentationProgress;
  const entering = presentationDirection === "entering";
  // The incoming scene arrives a little after the outgoing one starts to leave.
  const t = entering ? Easing.out(Easing.cubic)(interpolate(p, [0.25, 1], [0, 1], { extrapolateLeft: "clamp" })) : Easing.in(Easing.cubic)(p);
  const opacity = entering ? t : 1 - t;
  const scale = entering ? 1.04 - 0.04 * t : 1 + 0.035 * t;
  const blur = entering ? 10 * (1 - t) : 10 * t;
  return (
    <AbsoluteFill style={{ opacity, transform: `scale(${scale})`, filter: blur > 0.05 ? `blur(${blur}px)` : undefined }}>{children}</AbsoluteFill>
  );
}

export function focusPull(): TransitionPresentation<Props> {
  return { component: FocusPull, props: {} };
}
