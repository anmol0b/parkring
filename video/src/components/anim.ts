import { interpolate, spring } from "remotion";
import { FPS } from "../theme";

/** 0 → 1 over `duration` frames starting at `start`, clamped. */
export const ramp = (frame: number, start: number, duration: number) =>
  interpolate(frame, [start, start + duration], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

/** A gentle spring starting at `start`. */
export const pop = (frame: number, start: number) =>
  spring({ frame: frame - start, fps: FPS, config: { damping: 200, mass: 0.6 } });

/** Characters of `text` revealed at `cps` characters per second from `start`. */
export const typed = (text: string, frame: number, start: number, cps = 40) =>
  text.slice(0, Math.max(0, Math.floor(((frame - start) / FPS) * cps)));

/** Fade out over the last `duration` frames of a scene of length `total`. */
export const fadeOut = (frame: number, total: number, duration = 10) =>
  1 - ramp(frame, total - duration, duration);
