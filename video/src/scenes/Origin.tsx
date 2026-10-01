import React from "react";
import { useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { pop, ramp } from "../components/anim";
import { origin } from "../data";
import { colors, fonts } from "../theme";

export const Origin: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const code = pop(frame, 12);
  const highlight = ramp(frame, 60, 15);
  return (
    <Scene duration={duration} padding={100}>
      <Caption text={origin.caption} />
      <div style={{ display: "flex", gap: 60, marginTop: 50, alignItems: "flex-start" }}>
        <div
          style={{
            flex: "0 0 1080px",
            background: colors.panel,
            border: `1px solid ${colors.border}`,
            borderRadius: 16,
            padding: "24px 30px",
            opacity: code,
            transform: `translateY(${(1 - code) * 30}px)`,
          }}
        >
          <div style={{ fontFamily: fonts.mono, fontSize: 20, color: colors.muted, marginBottom: 14 }}>
            {origin.file}
          </div>
          {origin.code.map((line, i) => {
            const bad = origin.badLines.includes(i);
            return (
              <div
                key={i}
                style={{
                  fontFamily: fonts.mono,
                  fontVariantLigatures: "none",
                  fontSize: 24,
                  lineHeight: 1.55,
                  whiteSpace: "pre",
                  color: bad ? colors.text : colors.muted,
                  background: bad ? `rgba(255,123,84,${0.22 * highlight})` : "transparent",
                  borderLeft: `4px solid ${bad ? `rgba(255,123,84,${highlight})` : "transparent"}`,
                  paddingLeft: 12,
                }}
              >
                {line}
              </div>
            );
          })}
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 40, marginTop: 30 }}>
          {origin.facts.map((fact, i) => {
            const p = pop(frame, 85 + i * 35);
            return (
              <div
                key={fact}
                style={{
                  fontFamily: fonts.sans,
                  fontWeight: 600,
                  fontSize: 42,
                  lineHeight: 1.25,
                  color: colors.fail,
                  opacity: p,
                  transform: `translateX(${(1 - p) * 30}px)`,
                }}
              >
                {fact}
              </div>
            );
          })}
        </div>
      </div>
    </Scene>
  );
};
