import React from "react";
import { Sequence, useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { Terminal } from "../components/Terminal";
import { pop, ramp, typed } from "../components/anim";
import { breaking, type Vignette } from "../data";
import { colors, fonts } from "../theme";

const VIGNETTE = 112;

const Shot: React.FC<{ v: Vignette }> = ({ v }) => {
  const frame = useCurrentFrame();
  const cmd = typed(v.command, frame, 0, 60);
  const outputOn = ramp(frame, 26, 6);
  const verdict = pop(frame, 50);
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 40 }}>
      <Terminal title={v.tool}>
        <div style={{ color: colors.muted }}>
          <span style={{ color: colors.pass }}>$ </span>
          {cmd}
        </div>
        <div style={{ color: colors.fail, opacity: outputOn, marginTop: 10, wordBreak: "break-word" }}>
          {v.output}
        </div>
      </Terminal>
      <div
        style={{
          fontFamily: fonts.sans,
          fontWeight: 600,
          fontSize: 44,
          color: colors.pass,
          opacity: verdict,
          transform: `translateY(${(1 - verdict) * 16}px)`,
          textAlign: "center",
          maxWidth: 1500,
        }}
      >
        ✓ {v.verdict}
      </div>
    </div>
  );
};

export const BREAKING_DURATION = 60 + VIGNETTE * breaking.vignettes.length;

export const Breaking: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  // The caption leads, then shrinks to a header while the vignettes play.
  const shrink = ramp(frame, 40, 15);
  return (
    <Scene duration={duration} padding={90}>
      <div
        style={{
          position: "absolute",
          top: 90 - 40 * shrink + 260 * (1 - shrink),
          left: 0,
          right: 0,
          display: "flex",
          justifyContent: "center",
        }}
      >
        <Caption text={breaking.caption} size={96 - 40 * shrink} />
      </div>
      {breaking.vignettes.map((v, i) => (
        <Sequence key={v.tool + i} from={60 + i * VIGNETTE} durationInFrames={VIGNETTE} layout="none">
          <div
            style={{
              position: "absolute",
              top: 340,
              left: 0,
              right: 0,
              display: "flex",
              justifyContent: "center",
            }}
          >
            <Shot v={v} />
          </div>
        </Sequence>
      ))}
    </Scene>
  );
};
