import React from "react";
import { useCurrentFrame } from "remotion";
import { colors, fonts } from "../theme";
import { ramp } from "./anim";

/**
 * A lower-left caption under the main shot. `then` is a second sentence that
 * appears at frame `thenAt`, in the muted color.
 */
export const Caption: React.FC<{ text: string; start?: number; then?: string; thenAt?: number }> = ({
  text,
  start = 0,
  then,
  thenAt = 0,
}) => {
  const frame = useCurrentFrame();
  return (
    <div
      style={{
        position: "absolute",
        left: 76,
        right: 76,
        bottom: 66,
        fontFamily: fonts.sans,
        fontWeight: 500,
        fontSize: 44,
        letterSpacing: -0.4,
        color: colors.text,
      }}
    >
      <span style={{ opacity: ramp(frame, start, 5) }}>{text}</span>
      {then && <span style={{ color: colors.muted, opacity: ramp(frame, thenAt, 5) }}> {then}</span>}
    </div>
  );
};
