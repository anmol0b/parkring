import React from "react";
import { useCurrentFrame } from "remotion";
import { Scene } from "../components/Scene";
import { pop } from "../components/anim";
import { outro, title } from "../data";
import { colors, fonts } from "../theme";

export const Outro: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const url = pop(frame, 55);
  return (
    <Scene duration={duration}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 56 }}>
        <div style={{ display: "flex", gap: 28 }}>
          {outro.badges.map((badge, i) => {
            const p = pop(frame, 5 + i * 9);
            return (
              <div
                key={badge}
                style={{
                  fontFamily: fonts.mono,
                  fontWeight: 700,
                  fontSize: 40,
                  color: colors.pass,
                  border: `2px solid ${colors.pass}`,
                  borderRadius: 999,
                  padding: "14px 34px",
                  opacity: p,
                  transform: `scale(${0.85 + 0.15 * p})`,
                }}
              >
                ✓ {badge}
              </div>
            );
          })}
        </div>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 130, color: colors.ours, opacity: url }}>
          {title.name}
        </div>
        <div style={{ fontFamily: fonts.mono, fontSize: 52, color: colors.text, opacity: url }}>{outro.url}</div>
        <div style={{ fontFamily: fonts.sans, fontSize: 34, color: colors.muted, opacity: url }}>{outro.footer}</div>
      </div>
    </Scene>
  );
};
