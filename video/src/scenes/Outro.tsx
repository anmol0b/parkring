import React from "react";
import { Sequence } from "remotion";
import { Scene } from "../components/Scene";
import { Terminal, timeline } from "../components/Terminal";
import { outro } from "../data";
import { colors, fonts } from "../theme";

const tl = timeline(outro.session, 6, "outro");
const CARD_AT = tl.done + 20;
export const OUTRO_DURATION = CARD_AT + 110;

/** Clone it, then a hard cut to the end card. */
export const Outro: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene duration={duration}>
    <Sequence durationInFrames={CARD_AT}>
      <Terminal session={outro.session} seed="outro" top={360} height={260} />
    </Sequence>
    <Sequence from={CARD_AT}>
      <div style={{ position: "absolute", left: 160, top: 360 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 120, color: colors.ours, letterSpacing: -2 }}>
          {outro.name}
        </div>
        <div style={{ fontFamily: fonts.mono, fontSize: 44, color: colors.text, marginTop: 20 }}>{outro.url}</div>
        <div style={{ fontFamily: fonts.sans, fontSize: 30, color: colors.muted, marginTop: 44 }}>{outro.footer}</div>
      </div>
    </Sequence>
  </Scene>
);
