import React from "react";
import { useCurrentFrame } from "remotion";
import { colors, fonts } from "../theme";
import { pop } from "./anim";

export const Caption: React.FC<{ text: string; start?: number; size?: number; color?: string }> = ({
  text,
  start = 0,
  size = 64,
  color = colors.text,
}) => {
  const frame = useCurrentFrame();
  const p = pop(frame, start);
  return (
    <div
      style={{
        fontFamily: fonts.sans,
        fontWeight: 800,
        fontSize: size,
        color,
        letterSpacing: -1,
        opacity: p,
        transform: `translateY(${(1 - p) * 24}px)`,
      }}
    >
      {text}
    </div>
  );
};
