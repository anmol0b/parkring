import React from "react";
import { Composition } from "remotion";
import { Launch, TOTAL_FRAMES } from "./Launch";
import { FPS } from "./theme";

export const Root: React.FC = () => (
  <Composition id="Launch" component={Launch} durationInFrames={TOTAL_FRAMES} fps={FPS} width={1920} height={1080} />
);
