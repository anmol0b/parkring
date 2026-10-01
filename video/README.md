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

## Terminal demo (real commands, real output)

`demo.tape` records a terminal session with [VHS](https://github.com/charmbracelet/vhs)
(`brew install vhs`). Every command and line of output is real; the `#` lines are the
narration. Warm the build caches first so nothing compiles on camera, then record from
the repository root:

```sh
export CARGO_TERM_COLOR=never
CARGO_TARGET_DIR=target/demo-mutant RUSTFLAGS='--cfg loom --cfg parkring_mutant="deque_no_pop_fence"' cargo test -q -r --lib pop_racing
CARGO_TARGET_DIR=target/demo-loom RUSTFLAGS='--cfg loom' cargo test -q -r --lib deque
cargo test -q -r --workspace
vhs video/demo.tape    # writes video/out/parkring-demo.mp4 (about 80 s)
```
