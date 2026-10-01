import React from "react";
import { useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { pop } from "../components/anim";
import { built } from "../data";
import { colors, fonts } from "../theme";

export const Built: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  return (
    <Scene duration={duration}>
      <div style={{ display: "flex", justifyContent: "center" }}>
        <Caption text={built.caption} />
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "center", gap: 32, marginTop: 70 }}>
        {built.cards.map((card, i) => {
          const p = pop(frame, 20 + i * 12);
          return (
            <div
              key={card.name}
              style={{
                width: 540,
                height: 190,
                boxSizing: "border-box",
                padding: "34px 38px",
                background: colors.panel,
                border: `1px solid ${colors.border}`,
                borderTop: `5px solid ${colors.ours}`,
                borderRadius: 16,
                opacity: p,
                transform: `scale(${0.9 + 0.1 * p})`,
              }}
            >
              <div style={{ fontFamily: fonts.sans, fontWeight: 800, fontSize: 44, color: colors.text, whiteSpace: "nowrap" }}>
                {card.name}
              </div>
              <div style={{ fontFamily: fonts.mono, fontSize: 28, color: colors.muted, marginTop: 10 }}>
                {card.detail}
              </div>
            </div>
          );
        })}
      </div>
    </Scene>
  );
};
