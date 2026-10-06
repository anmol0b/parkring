import React from "react";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { Terminal, line, timeline, type Line, type Session } from "../components/Terminal";
import { built } from "../data";

const COL = 24;
const base: Session = {
  command: built.session.command,
  output: built.session.output.map(([tree]) => line(tree, "text", { wait: 1 })),
};
const tl = timeline(base, 6, "built");

/** Annotations land one at a time once `tree` has printed. */
const session: Session = {
  ...base,
  output: built.session.output.map(([tree, note], i): Line => {
    const k = built.session.output.slice(0, i).filter(([, n]) => n).length;
    return {
      segs: [
        { text: tree.padEnd(COL, " ") },
        ...(note ? [{ text: `# ${note}`, tone: "theirs" as const, appear: tl.done + 4 + k * 13 }] : []),
      ],
      wait: 1,
    };
  }),
};

export const BUILT_DURATION = tl.done + 4 + 5 * 13 + 45;

export const Built: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene duration={duration}>
    <Terminal session={session} seed="built" />
    <Caption text={built.caption} start={tl.done} then={built.then} thenAt={tl.done + 4 + 3 * 13} />
  </Scene>
);
