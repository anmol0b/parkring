import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";

const inter = loadInter("normal", { weights: ["400", "500"], subsets: ["latin"] });
const mono = loadMono("normal", { weights: ["400", "700"], subsets: ["latin", "latin-ext"] });

export const fonts = {
  sans: inter.fontFamily,
  mono: mono.fontFamily,
};

/** GitHub-dark surfaces, and the README charts' Okabe-Ito palette. */
export const colors = {
  bg: "#0d1117",
  panel: "#161b22",
  border: "#30363d",
  text: "#e6edf3",
  muted: "#8b949e",
  dim: "#6e7681",
  ours: "#3a9bd9", // the charts' #0072B2, lifted for contrast on dark
  theirs: "#E69F00",
  fail: "#ff7b54",
  pass: "#3fb98a",
};

export const FPS = 30;
