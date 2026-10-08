import React from "react";
import { useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { ramp } from "../components/anim";
import { losses } from "../data";
import { colors, fonts } from "../theme";

export const LOSSES_DURATION = 205;

export const Losses: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  return (
    <Scene duration={duration}>
      <div style={{ position: "absolute", top: 250, left: 76, display: "flex", flexDirection: "column", gap: 70 }}>
        {losses.lines.map((l, i) => (
          <div key={l.text} style={{ opacity: ramp(frame, 12 + i * 45, 5) }}>
            <div style={{ fontFamily: fonts.sans, fontWeight: 500, fontSize: 62, letterSpacing: -0.8, color: colors.text }}>
              {l.text}
            </div>
            <div style={{ fontFamily: fonts.mono, fontSize: 28, color: colors.muted, marginTop: 14 }}>{l.detail}</div>
          </div>
        ))}
      </div>
      <Caption text={losses.caption} start={2} />
    </Scene>
  );
};
