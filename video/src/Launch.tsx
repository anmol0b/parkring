import React from "react";
import { Series } from "remotion";
import { BREAKING_DURATION, Breaking } from "./scenes/Breaking";
import { Built } from "./scenes/Built";
import { Losses } from "./scenes/Losses";
import { Origin } from "./scenes/Origin";
import { Outro } from "./scenes/Outro";
import { Results } from "./scenes/Results";
import { Title } from "./scenes/Title";

/** Scene lengths in frames at 30 fps. Edit pacing here. */
export const SCENES = [
  { name: "title", frames: 120, C: Title },
  { name: "origin", frames: 240, C: Origin },
  { name: "built", frames: 180, C: Built },
  { name: "breaking", frames: BREAKING_DURATION, C: Breaking },
  { name: "results", frames: 390, C: Results },
  { name: "losses", frames: 180, C: Losses },
  { name: "outro", frames: 182, C: Outro },
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
