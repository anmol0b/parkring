import React from "react";
import { Sequence, useCurrentFrame } from "remotion";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { fadeOut, pop, ramp } from "../components/anim";
import { results, type Bar } from "../data";
import { colors, fonts } from "../theme";

const BAR_MAX = 900;

const BarRow: React.FC<{ bar: Bar; start: number }> = ({ bar, start }) => {
  const frame = useCurrentFrame();
  const grow = pop(frame, start);
  const max = Math.max(bar.ours, bar.theirs);
  const row = (who: string, value: number, color: string) => (
    <div style={{ display: "flex", alignItems: "center", gap: 20, height: 46 }}>
      <div style={{ width: 300, fontFamily: fonts.mono, fontSize: 26, color: colors.muted, textAlign: "right" }}>
        {who}
      </div>
      <div style={{ height: 38, width: (BAR_MAX * value * grow) / max, background: color, borderRadius: 6 }} />
      <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 30, color, opacity: grow }}>
        {value} {bar.unit}
      </div>
    </div>
  );
  return (
    <div style={{ opacity: ramp(frame, start, 6) }}>
      <div style={{ fontFamily: fonts.sans, fontWeight: 600, fontSize: 36, color: colors.text, marginBottom: 10 }}>
        {bar.label}
      </div>
      {row("parkring", bar.ours, colors.ours)}
      {row(bar.them, bar.theirs, colors.theirs)}
    </div>
  );
};

const BARS_PART = 210;

const Idle: React.FC = () => {
  const frame = useCurrentFrame();
  const p = pop(frame, 0);
  const meter = (label: string, latency: string, cpu: number, color: string, start: number) => {
    const g = pop(frame, start);
    return (
      <div style={{ flex: 1 }}>
        <div style={{ fontFamily: fonts.sans, fontWeight: 800, fontSize: 44, color }}>{label}</div>
        <div style={{ fontFamily: fonts.mono, fontSize: 34, color: colors.text, marginTop: 18 }}>
          wakes in <b style={{ color }}>{latency}</b>
        </div>
        <div style={{ fontFamily: fonts.mono, fontSize: 34, color: colors.text, marginTop: 30 }}>
          CPU while idle: <b style={{ color }}>{Math.round(cpu * g * 10) / 10}%</b>
        </div>
        <div style={{ marginTop: 16, height: 30, width: 640, background: colors.border, borderRadius: 6 }}>
          <div style={{ height: 30, width: 6.4 * cpu * g, background: color, borderRadius: 6 }} />
        </div>
      </div>
    );
  };
  return (
    <div style={{ opacity: p }}>
      <div style={{ fontFamily: fonts.sans, fontWeight: 600, fontSize: 40, color: colors.muted, marginBottom: 50 }}>
        {results.idle.note}
      </div>
      <div style={{ display: "flex", gap: 80 }}>
        {meter("parkring parks", results.idle.ours.latency, results.idle.ours.cpu, colors.ours, 15)}
        {meter("crossbeam + spin", results.idle.spin.latency, results.idle.spin.cpu, colors.theirs, 35)}
      </div>
    </div>
  );
};

export const Results: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  return (
    <Scene duration={duration} padding={110}>
      <Caption text={results.caption} size={60} />
      <div style={{ position: "relative", marginTop: 50, height: 700 }}>
        <Sequence durationInFrames={BARS_PART} layout="none">
          <div style={{ display: "flex", flexDirection: "column", gap: 40, opacity: fadeOut(frame, BARS_PART) }}>
            {results.bars.map((bar, i) => (
              <BarRow key={bar.label} bar={bar} start={15 + i * 30} />
            ))}
          </div>
        </Sequence>
        <Sequence from={BARS_PART} layout="none">
          <Idle />
        </Sequence>
      </div>
    </Scene>
  );
};
