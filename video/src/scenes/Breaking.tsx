import React from "react";
import { Sequence } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { Terminal, timeline } from "../components/Terminal";
import { breaking } from "../data";

const HOLD = 40;

const shots = breaking.map((shot, i) => {
  const seed = `break-${i}`;
  const tl = timeline(shot.session, 6, seed);
  const hit = shot.session.output.findIndex((l) => l.hit);
  // The second sentence lands with the line that matters, or when a progress line finishes.
  const thenAt = hit >= 0 ? tl.at[hit] + 8 : (tl.ends.at(-1) ?? tl.done);
  return { shot, seed, thenAt, frames: Math.max(tl.done, thenAt) + HOLD };
});

const starts = shots.map((_, i) => shots.slice(0, i).reduce((n, s) => n + s.frames, 0));

export const BREAKING_DURATION = shots.reduce((n, s) => n + s.frames, 0);

/** Four failures, each a hard cut to a new terminal. */
export const Breaking: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene duration={duration}>
    {shots.map(({ shot, seed, thenAt, frames }, i) => (
      <Sequence key={seed} from={starts[i]} durationInFrames={frames}>
        <Terminal session={shot.session} seed={seed} fit />
        <Caption text={shot.caption} start={2} then={shot.then} thenAt={thenAt} />
      </Sequence>
    ))}
  </Scene>
);
