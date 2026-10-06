import React from "react";
import { interpolate, useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { ramp } from "../components/anim";
import { origin } from "../data";
import { colors, fonts } from "../theme";

const FONT = 24;
const ROW = FONT * 1.5;
/** When each bad line gets its highlight and note, in frames. */
const MARKS: Record<number, number> = { 43: 40, 47: 78 };

/** The first version of the code in an editor, with the two lines that made it wrong. */
export const Origin: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const push = interpolate(frame, [0, duration], [1, 1.035]);
  return (
    <Scene duration={duration}>
      <div
        style={{
          position: "absolute",
          top: 56,
          left: 72,
          width: 1776,
          height: 720,
          background: colors.panel,
          border: `1px solid ${colors.border}`,
          borderRadius: 12,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            height: 48,
            display: "flex",
            alignItems: "stretch",
            justifyContent: "space-between",
            background: colors.bg,
            borderBottom: `1px solid ${colors.border}`,
            fontFamily: fonts.mono,
            fontSize: 20,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              padding: "0 22px",
              background: colors.panel,
              borderRight: `1px solid ${colors.border}`,
              color: colors.text,
            }}
          >
            {origin.file}
          </div>
          <div style={{ display: "flex", alignItems: "center", paddingRight: 22, color: colors.dim }}>{origin.rev}</div>
        </div>
        <div
          style={{
            padding: "12px 0",
            transform: `scale(${push})`,
            transformOrigin: "30% 70%",
            fontFamily: fonts.mono,
            fontSize: FONT,
            lineHeight: `${ROW}px`,
            fontVariantLigatures: "none",
            whiteSpace: "pre",
          }}
        >
          {origin.code.map(([n, code]) => {
            const mark = MARKS[n];
            const sweep = mark === undefined ? 0 : ramp(frame, mark, 12);
            const note = origin.notes[n];
            const noteChars = mark === undefined ? 0 : Math.max(0, Math.floor((frame - mark - 10) * 1.6));
            return (
              <div key={n} style={{ display: "flex", position: "relative" }}>
                <div
                  style={{
                    position: "absolute",
                    left: 0,
                    top: 0,
                    bottom: 0,
                    width: `${sweep * 100}%`,
                    background: "rgba(255,123,84,0.14)",
                    borderLeft: sweep > 0 ? `3px solid ${colors.fail}` : undefined,
                  }}
                />
                <div style={{ width: 84, paddingRight: 24, textAlign: "right", color: sweep > 0 ? colors.text : colors.dim }}>
                  {n}
                </div>
                <div style={{ position: "relative", color: colors.text }}>
                  {n === origin.folded ? (
                    <>
                      {code.slice(0, -2)}
                      <span style={{ color: colors.muted, background: colors.border, borderRadius: 4, padding: "0 6px" }}>
                        {"⋯"}
                      </span>
                      {"}"}
                    </>
                  ) : (
                    code
                  )}
                  {note && <span style={{ color: colors.fail }}>{`    // ${note}`.slice(0, noteChars + 4)}</span>}
                </div>
              </div>
            );
          })}
        </div>
      </div>
      <div style={{ position: "absolute", top: 812, left: 76, fontFamily: fonts.mono, fontSize: 26, lineHeight: 1.6 }}>
        {origin.facts.map((fact, i) => (
          <div key={fact} style={{ color: colors.muted, opacity: ramp(frame, 125 + i * 28, 5) }}>
            {fact}
          </div>
        ))}
      </div>
      <Caption text={origin.caption} start={4} then={origin.then} thenAt={MARKS[43]} />
    </Scene>
  );
};
