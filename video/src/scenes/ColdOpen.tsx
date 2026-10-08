import React from "react";
import { Caption } from "../components/Caption";
import { Scene } from "../components/Scene";
import { Terminal, timeline } from "../components/Terminal";
import { coldOpen } from "../data";

const tl = timeline(coldOpen.session, 6, "cold");
const hitAt = tl.at[coldOpen.session.output.findIndex((l) => l.hit)];

export const COLD_OPEN_DURATION = tl.done + 45;

/** No title: the video opens on a real loom failure. */
export const ColdOpen: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene duration={duration}>
    <Terminal session={coldOpen.session} seed="cold" fit />
    <Caption text={coldOpen.caption} start={hitAt + 18} />
  </Scene>
);
