import React from "react";
import { useCurrentFrame } from "remotion";
import { Scene } from "../components/Scene";
import { pop, typed } from "../components/anim";
import { title } from "../data";
import { colors, fonts } from "../theme";

export const Title: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const name = typed(title.name, frame, 6, 14);
  const cursorOn = Math.floor(frame / 15) % 2 === 0;
  const sub = pop(frame, 50);
  return (
    <Scene duration={duration}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 36 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 200, color: colors.text }}>
          <span style={{ color: colors.ours }}>{name}</span>
          <span style={{ opacity: cursorOn ? 1 : 0, color: colors.muted }}>▍</span>
        </div>
        <div
          style={{
            fontFamily: fonts.sans,
            fontSize: 54,
            color: colors.muted,
            opacity: sub,
            transform: `translateY(${(1 - sub) * 20}px)`,
          }}
        >
          {title.tagline}
        </div>
      </div>
    </Scene>
  );
};
