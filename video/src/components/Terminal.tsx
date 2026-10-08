import React from "react";
import { Audio, Sequence, random, staticFile, useCurrentFrame } from "remotion";
import { colors, fonts, FPS } from "../theme";
import { blink, keystrokes, ramp, typedCount } from "./anim";

export type Tone = "text" | "muted" | "dim" | "fail" | "pass" | "ours" | "theirs";
/** `appear` holds a segment back until that frame (an annotation added in the edit). */
export type Seg = { text: string; tone?: Tone; bold?: boolean; appear?: number };
/**
 * One line of output. `wait` is the pause in frames before it prints, `hit`
 * highlights it as the line that matters, and `stream` prints it a character at
 * a time at that many characters a second (a progress line).
 */
export type Line = { segs: Seg[]; wait?: number; hit?: boolean; stream?: number };
/** `recalled` puts the whole command on the line at once, as if from shell history. */
export type Session = { before?: Line[]; command: string; output: Line[]; recalled?: boolean; cwd?: string };

export const line = (text: string, tone: Tone = "text", opts: Omit<Line, "segs"> = {}): Line => ({
  segs: [{ text, tone }],
  ...opts,
});

const FONT = 31;
const LINE_H = 1.55;
const CHAR_W = FONT * 0.6;
const PAD_X = 36;
const PAD_Y = 26;
const BAR = 44;

export const PROMPT_CWD = "~/parkring";

/** Frame timings for a session whose command starts typing at `start`. */
export const timeline = (s: Session, start: number, seed: string) => {
  const keys = s.recalled ? [start] : keystrokes(s.command, start, seed);
  const enter = (keys.at(-1) ?? start) + (s.recalled ? 14 : 7);
  let t = enter + 3;
  const at: number[] = [];
  const ends: number[] = [];
  for (const l of s.output) {
    t += l.wait ?? 1;
    at.push(t);
    if (l.stream) t += Math.round((text(l).length * FPS) / l.stream);
    ends.push(t);
  }
  return { keys, enter, at, ends, done: t + 6 };
};

const text = (l: Line) => l.segs.map((s) => s.text).join("");

const Prompt: React.FC<{ cwd: string }> = ({ cwd }) => (
  <>
    <span style={{ color: colors.ours }}>{cwd}</span>
    <span style={{ color: colors.muted }}> $ </span>
  </>
);

const Cursor: React.FC<{ on: boolean }> = ({ on }) => (
  <span style={{ background: on ? colors.muted : "transparent", color: "transparent" }}>{" "}</span>
);

/** Renders the segments of a line, showing only its first `chars` characters. */
const Segs: React.FC<{ segs: Seg[]; chars?: number }> = ({ segs, chars = Infinity }) => {
  const frame = useCurrentFrame();
  let left = chars;
  return (
    <>
      {segs.map((s, i) => {
        const shown = s.text.slice(0, Math.max(0, left));
        left -= s.text.length;
        return (
          <span
            key={i}
            style={{
              color: colors[s.tone ?? "text"],
              fontWeight: s.bold ? 700 : 400,
              opacity: s.appear === undefined ? 1 : ramp(frame, s.appear, 6),
            }}
          >
            {shown}
          </span>
        );
      })}
    </>
  );
};

const KEY_SAMPLES = 5;

/** One key: a random down sample, then a random up sample when the key is let go. */
const Key: React.FC<{ at: number; seed: string; heavy?: boolean }> = ({ at, seed, heavy = false }) => {
  const pick = (what: string) => 1 + Math.floor(random(`${seed}-${what}`) * KEY_SAMPLES);
  // Heavier keys (space, enter) play a little slower and lower, and are held longer.
  const rate = (heavy ? 0.88 : 1) * (0.97 + random(`${seed}-rate`) * 0.06);
  const hold = (heavy ? 3 : 2) + Math.round(random(`${seed}-hold`));
  return (
    <>
      <Sequence from={at} durationInFrames={4} layout="none">
        <Audio src={staticFile(`audio/kailh-white/down${pick("down")}.wav`)} volume={heavy ? 0.6 : 0.5} playbackRate={rate} />
      </Sequence>
      <Sequence from={at + hold} durationInFrames={4} layout="none">
        <Audio src={staticFile(`audio/kailh-white/up${pick("up")}.wav`)} volume={heavy ? 0.45 : 0.4} playbackRate={rate} />
      </Sequence>
    </>
  );
};

/** Mechanical keyboard sounds for a typed command: every key, then enter. */
export const KeySounds: React.FC<{ keys: number[]; enter: number; seed: string; command: string }> = ({
  keys,
  enter,
  seed,
  command,
}) => (
  <>
    {keys.map((f, i) => (
      <Key key={i} at={f} seed={`${seed}-${i}`} heavy={command[i] === " "} />
    ))}
    <Key at={enter} seed={`${seed}-enter`} heavy />
  </>
);

/**
 * A terminal window that fills the top of the frame. The command is typed by
 * hand, output prints in bursts the way cargo prints it, and older lines
 * scroll off the top.
 */
export const Terminal: React.FC<{
  session: Session;
  start?: number;
  seed: string;
  top?: number;
  height?: number;
  /** Shrink the window to the rows the session ends with, so a short session isn't a big empty box. */
  fit?: boolean;
}> = ({ session, start = 6, seed, top = 56, height: maxHeight = 790, fit = false }) => {
  const frame = useCurrentFrame();
  const tl = timeline(session, start, seed);
  const typed = session.recalled ? (frame >= start ? Infinity : 0) : typedCount(tl.keys, frame);
  const typing = frame < tl.enter;
  const cwd = session.cwd ?? PROMPT_CWD;

  const width = 1920 - 2 * 72;
  const cols = Math.floor((width - 2 * PAD_X) / CHAR_W);
  const rowsOf = (len: number) => Math.max(1, Math.ceil(len / cols));
  // Rows on screen once the session is over: history, command, every output line, the next prompt.
  const finalRows =
    (session.before ?? []).reduce((n, l) => n + rowsOf(text(l).length), 0) +
    rowsOf(cwd.length + 3 + session.command.length) +
    session.output.reduce((n, l) => n + rowsOf(text(l).length), 0) +
    1;
  const rowH = FONT * LINE_H;
  const height = fit ? Math.min(maxHeight, Math.ceil(BAR + 6 + PAD_Y + finalRows * rowH + 8)) : maxHeight;
  const fitted = fit && height < maxHeight;
  const maxRows = fitted ? finalRows : Math.floor((height - BAR - 2 * PAD_Y) / rowH);
  // A shrunk window sits in the middle of the space the full-size one would take.
  const y = fitted ? top + Math.round((maxHeight - height) / 2) : top;

  type Row = { key: string; rows: number; node: React.ReactNode };
  const rows: Row[] = [];
  const push = (key: string, len: number, node: React.ReactNode) =>
    rows.push({ key, rows: rowsOf(len), node });

  session.before?.forEach((l, i) => push(`b${i}`, text(l).length, text(l) === "" ? "\u00a0" : <Segs segs={l.segs} />));
  push(
    "cmd",
    cwd.length + 3 + session.command.length,
    <>
      <Prompt cwd={cwd} />
      <span style={{ color: colors.text }}>{session.command.slice(0, typed)}</span>
      {typing && <Cursor on={frame < 8 || blink(frame)} />}
    </>,
  );
  session.output.forEach((l, i) => {
    if (frame < tl.at[i]) return;
    const chars = l.stream ? Math.floor(((frame - tl.at[i]) * l.stream) / FPS) : Infinity;
    const hit = l.hit ? ramp(frame, tl.at[i] + 4, 10) : 0;
    push(
      `o${i}`,
      text(l).length,
      <div
        style={{
          background: l.segs[0]?.tone === "pass" ? `rgba(63,185,138,${0.16 * hit})` : `rgba(255,123,84,${0.16 * hit})`,
          margin: "0 -12px",
          padding: "0 12px",
        }}
      >
        {text(l) === "" ? "\u00a0" : <Segs segs={l.segs} chars={chars} />}
      </div>,
    );
  });
  if (frame >= tl.done) {
    push(
      "next",
      cwd.length + 3,
      <>
        <Prompt cwd={cwd} />
        <Cursor on={blink(frame)} />
      </>,
    );
  }

  // Scroll: drop rows off the top until the rest fit.
  let total = rows.reduce((n, r) => n + r.rows, 0);
  while (total > maxRows && rows.length > 1) total -= rows.shift()!.rows;

  return (
    <div
      style={{
        position: "absolute",
        top: y,
        left: 72,
        width,
        height,
        background: colors.panel,
        border: `1px solid ${colors.border}`,
        borderRadius: 12,
        overflow: "hidden",
      }}
    >
      <div style={{ height: BAR, display: "flex", alignItems: "center", gap: 9, paddingLeft: 18 }}>
        {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
          <div key={c} style={{ width: 13, height: 13, borderRadius: 7, background: c, opacity: 0.85 }} />
        ))}
      </div>
      <div
        style={{
          padding: `${PAD_Y}px ${PAD_X}px`,
          paddingTop: 6,
          fontFamily: fonts.mono,
          fontSize: FONT,
          lineHeight: LINE_H,
          fontVariantLigatures: "none",
          whiteSpace: "pre-wrap",
          wordBreak: "break-all",
          color: colors.text,
        }}
      >
        {rows.map((r) => (
          <div key={r.key}>{r.node}</div>
        ))}
      </div>
      <KeySounds keys={tl.keys} enter={tl.enter} seed={seed} command={session.command} />
    </div>
  );
};
