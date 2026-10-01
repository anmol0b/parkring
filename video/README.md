# Launch video

A 60-second, 1920×1080 launch video for parkring, built with
[Remotion](https://www.remotion.dev) (React → MP4). Captions only, no audio.

```sh
npm install
npm run studio     # live preview in the browser
npm run render     # writes out/parkring-launch.mp4
npm run still -- out/frame.png --frame=330   # one frame, for checking layout
```

* Every number and line of copy is in `src/data.ts`, taken from `docs/BENCHMARKS.md`
  and the README. Change the copy there.
* Scene lengths (in frames at 30 fps) are in `SCENES` in `src/Launch.tsx`.
  Change the pacing there; the total must stay 1800 frames for 60 s.
* Remotion is free for individuals and companies of up to three people.
