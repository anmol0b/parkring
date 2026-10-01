import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { colors } from "../theme";
import { fadeOut, ramp } from "./anim";

/** Full-frame scene with a short fade in and out. `duration` is the scene's length in frames. */
export const Scene: React.FC<{ duration: number; children: React.ReactNode; padding?: number }> = ({
  duration,
  children,
  padding = 120,
}) => {
  const frame = useCurrentFrame();
  const opacity = Math.min(ramp(frame, 0, 8), fadeOut(frame, duration));
  return (
    <AbsoluteFill style={{ background: colors.bg, padding, justifyContent: "center", opacity }}>
      {children}
    </AbsoluteFill>
  );
};
