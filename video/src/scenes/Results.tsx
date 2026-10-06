import React from "react";
import { Sequence, useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { ramp } from "../components/anim";
import { results, type Bar } from "../data";
import { colors, fonts } from "../theme";

const NAME_W = 400;
const PX_PER_UNIT = 11; // 100 M items/s = 1100 px

const BarRow: React.FC<{ bar: Bar; start: number }> = ({ bar, start }) => {
  const frame = useCurrentFrame();
  const grow = ramp(frame, start, 22);
  const row = (who: string, value: number, color: string) => (
    <div style={{ display: "flex", alignItems: "center", height: 54 }}>
      <div style={{ width: NAME_W, paddingRight: 20, textAlign: "right", fontFamily: fonts.mono, fontSize: 26, color: colors.muted }}>
        {who}
      </div>
      <div style={{ height: 38, width: value * PX_PER_UNIT * grow, background: color, borderRadius: 2 }} />
      <div style={{ marginLeft: 14, fontFamily: fonts.mono, fontSize: 26, color: colors.text, opacity: ramp(frame, start + 16, 6) }}>
        {value}
      </div>
    </div>
  );
  return (
    <div style={{ opacity: ramp(frame, start - 4, 4) }}>
      <div style={{ marginLeft: NAME_W, fontFamily: fonts.sans, fontWeight: 500, fontSize: 32, color: colors.text, marginBottom: 8 }}>
        {bar.label}
      </div>
      {row("parkring", bar.ours, colors.ours)}
      {row(bar.them, bar.theirs, colors.theirs)}
    </div>
  );
};

const Bars: React.FC = () => {
  const frame = useCurrentFrame();
  const { pool } = results;
  return (
    <div style={{ position: "absolute", top: 150, left: 76 }}>
      <div style={{ marginLeft: NAME_W, fontFamily: fonts.mono, fontSize: 22, color: colors.dim, marginBottom: 40 }}>
        {results.axis}
      </div>
      <div style={{ position: "absolute", top: 80, bottom: 0, left: NAME_W - 1, width: 1, background: colors.border }} />
      <div style={{ display: "flex", flexDirection: "column", gap: 44 }}>
        {results.bars.map((bar, i) => (
          <BarRow key={bar.label} bar={bar} start={10 + i * 34} />
        ))}
        <div style={{ marginLeft: NAME_W, opacity: ramp(frame, 90, 5) }}>
          <div style={{ fontFamily: fonts.sans, fontWeight: 500, fontSize: 32, color: colors.text, marginBottom: 12 }}>
            {pool.label}
          </div>
          <div style={{ fontFamily: fonts.mono, fontSize: 26, color: colors.muted }}>
            <span style={{ color: colors.ours }}>parkring</span> {pool.ours}
            {"     "}
            <span style={{ color: colors.theirs }}>{pool.them}</span> {pool.theirs}
            {"     "}
            <span style={{ color: colors.text }}>a tie.</span>
          </div>
        </div>
      </div>
    </div>
  );
};

const Idle: React.FC = () => {
  const frame = useCurrentFrame();
  const { header, rows } = results.idle;
  const cell = (w: number, content: React.ReactNode, color: string = colors.text) => (
    <div style={{ width: w, color }}>{content}</div>
  );
  return (
    <div style={{ position: "absolute", top: 300, left: 76, fontFamily: fonts.mono, fontSize: 34 }}>
      <div style={{ display: "flex", color: colors.dim, fontSize: 24, marginBottom: 28 }}>
        {cell(440, header[0], colors.dim)}
        {cell(320, header[1], colors.dim)}
        {cell(700, header[2], colors.dim)}
      </div>
      {rows.map((r, i) => {
        const g = ramp(frame, 8 + i * 22, 30);
        const color = i === 0 ? colors.ours : colors.theirs;
        return (
          <div key={r.who} style={{ display: "flex", alignItems: "center", height: 100, opacity: ramp(frame, 4 + i * 22, 4) }}>
            {cell(440, r.who, color)}
            {cell(320, r.latency)}
            <div style={{ display: "flex", alignItems: "center", gap: 20 }}>
              <div style={{ width: 110, textAlign: "right", color: colors.text }}>{`${Math.round(r.cpu * g * 10) / 10}%`}</div>
              <div style={{ width: 600, height: 20, background: colors.border }}>
                <div style={{ width: 6 * r.cpu * g, height: 20, background: color }} />
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
};

const BARS_PART = 175;
export const RESULTS_DURATION = BARS_PART + 135;

export const Results: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene duration={duration}>
    <Sequence durationInFrames={BARS_PART}>
      <Bars />
      <Caption text={results.caption} start={4} />
    </Sequence>
    <Sequence from={BARS_PART}>
      <Idle />
      <Caption text={results.idle.caption} start={40} />
    </Sequence>
  </Scene>
);
