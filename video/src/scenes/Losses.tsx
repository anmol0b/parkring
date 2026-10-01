import React from "react";
import { useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { pop } from "../components/anim";
import { losses } from "../data";
import { colors, fonts } from "../theme";

export const Losses: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  return (
    <Scene duration={duration}>
      <Caption text={losses.caption} size={80} />
      <div style={{ display: "flex", flexDirection: "column", gap: 34, marginTop: 60 }}>
        {losses.lines.map((line, i) => {
          const p = pop(frame, 25 + i * 30);
          return (
            <div
              key={line}
              style={{
                fontFamily: fonts.sans,
                fontWeight: 600,
                fontSize: 50,
                color: colors.theirs,
                opacity: p,
                transform: `translateX(${(1 - p) * 30}px)`,
              }}
            >
              — {line}
            </div>
          );
        })}
      </div>
    </Scene>
  );
};
