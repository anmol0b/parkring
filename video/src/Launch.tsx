import React from "react";
import { Series } from "remotion";
import { BREAKING_DURATION, Breaking } from "./scenes/Breaking";
import { BUILT_DURATION, Built } from "./scenes/Built";
import { COLD_OPEN_DURATION, ColdOpen } from "./scenes/ColdOpen";
import { LOSSES_DURATION, Losses } from "./scenes/Losses";
import { Origin } from "./scenes/Origin";
import { OUTRO_DURATION, Outro } from "./scenes/Outro";
import { RESULTS_DURATION, Results } from "./scenes/Results";

/**
 * Scene lengths in frames at 30 fps. Terminal scenes size themselves from
 * their output timing; edit pacing in each scene or here.
 */
export const SCENES = [
  { name: "cold-open", frames: COLD_OPEN_DURATION, C: ColdOpen },
  { name: "origin", frames: 210, C: Origin },
  { name: "built", frames: BUILT_DURATION, C: Built },
  { name: "breaking", frames: BREAKING_DURATION, C: Breaking },
  { name: "results", frames: RESULTS_DURATION, C: Results },
  { name: "losses", frames: LOSSES_DURATION, C: Losses },
  { name: "outro", frames: OUTRO_DURATION, C: Outro },
] as const;

export const TOTAL_FRAMES = SCENES.reduce((sum, s) => sum + s.frames, 0);

export const Launch: React.FC = () => (
  <Series>
    {SCENES.map(({ name, frames, C }) => (
      <Series.Sequence key={name} durationInFrames={frames}>
        <C duration={frames} />
      </Series.Sequence>
    ))}
  </Series>
);
