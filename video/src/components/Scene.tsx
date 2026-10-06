import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { colors } from "../theme";
import { ramp } from "./anim";

/** Full-frame scene. Cuts are near-hard: a 3-frame fade at each end. */
export const Scene: React.FC<{ duration: number; children: React.ReactNode }> = ({ duration, children }) => {
  const frame = useCurrentFrame();
  const opacity = Math.min(ramp(frame, 0, 3), 1 - ramp(frame, duration - 3, 3));
  return <AbsoluteFill style={{ background: colors.bg, opacity }}>{children}</AbsoluteFill>;
};
