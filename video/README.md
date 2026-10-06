# Launch video

A one-minute, 1920×1080 launch video for parkring, built with
[Remotion](https://www.remotion.dev) (React → MP4). It's cut like a screencast:
terminals, the original code in an editor, plain benchmark bars, lowercase
first-person captions.

```sh
npm install
npm run studio     # live preview in the browser
npm run render     # checks the copy, then writes out/parkring-launch.mp4
npm run still -- out/frame.png --frame=330   # one frame, for checking layout
```

* Every number, caption and line of terminal output is in `src/data.ts`, taken
  from `docs/BENCHMARKS.md`, the README and git history. Change the copy there.
  The cold open's loom failure and the passing deque run are captured from real
  runs. The other failures (the lost wakeup, Miri on the old latch, the SCQ hangs)
  were fixed long ago, so their output is reconstructed in the tools' real formats.
* No em or en dashes in on-screen text: `npm run lint:copy` fails if one appears,
  and `render` runs it first.
* Terminal scenes size themselves from their typing and output timing
  (`timeline` in `src/components/Terminal.tsx`). Other scene lengths are in
  `SCENES` in `src/Launch.tsx`.
* The only sound is a mechanical keyboard under the typing: the Kailh White
  key-down and key-up samples from anmol0b.xyz (`public/audio/kailh-white`,
  peak-normalized), played the way the site plays them: a random sample per
  key, slight pitch variation, and the release a moment after the press.
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
