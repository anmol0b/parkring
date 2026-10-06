import { Easing, interpolate, random } from "remotion";
import { FPS } from "../theme";

/** 0 to 1 over `duration` frames starting at `start`, clamped and eased. */
export const ramp = (frame: number, start: number, duration: number, easing = Easing.out(Easing.cubic)) =>
  interpolate(frame, [start, start + duration], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing,
  });

/**
 * The frame each character of `text` lands on when typed by hand from `start`:
 * about `cps` characters a second, with uneven gaps and a pause after spaces.
 */
export const keystrokes = (text: string, start: number, seed: string, cps = 26): number[] => {
  const base = FPS / cps;
  const frames: number[] = [];
  let t = start;
  for (let i = 0; i < text.length; i++) {
    t += base * (0.45 + random(`${seed}-${i}`) * 1.1);
    if (text[i - 1] === " ") t += base * 0.6;
    frames.push(Math.round(t));
  }
  return frames;
};

/** How many characters are visible at `frame`, given the frames from `keystrokes`. */
export const typedCount = (frames: number[], frame: number) => frames.filter((f) => f <= frame).length;

/** A block cursor that blinks every half second. */
export const blink = (frame: number) => Math.floor(frame / 15) % 2 === 0;
