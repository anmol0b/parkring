# parkring terminal demo: real commands, real output.
# Render from the repository root:  vhs video/demo.tape
# Build caches are warmed beforehand (see video/README.md), so nothing compiles on camera.
# The off-camera `cargo` wrapper only chooses a build-cache directory per RUSTFLAGS,
# so switching between loom builds does not trigger a rebuild.

Output video/out/parkring-demo.mp4

Set Shell "bash"
Set FontSize 24
Set Width 1920
Set Height 1080
Set Padding 56
Set Theme "GitHub Dark"
Set TypingSpeed 28ms
Set WindowBar Colorful
Set WindowBarSize 44

Hide
Type `export PS1='\[\e[1;32m\]$ \[\e[0m\]' CARGO_TERM_COLOR=never`
Enter
Type `cargo() { case "$RUSTFLAGS" in *mutant*) CARGO_TARGET_DIR=target/demo-mutant command cargo "$@";; *loom*) CARGO_TARGET_DIR=target/demo-loom command cargo "$@";; *) command cargo "$@";; esac; }`
Enter
Type "clear"
Enter
Show

Type "# parkring: lock-free queues, a work-stealing deque and a thread pool in Rust."
Enter
Type "# It started as a take-home assignment. Here is how it is checked."
Enter
Sleep 2500ms

Hide
Type "clear"
Enter
Show
Type "# 1. The work-stealing deque needs two SeqCst fences. Build it without one,"
Enter
Type "#    and the loom model checker explores every interleaving to find the bug:"
Enter
Type `RUSTFLAGS='--cfg loom --cfg parkring_mutant="deque_no_pop_fence"' cargo test -q -r --lib pop_racing 2>&1 | grep -E 'twice|result'`
Sleep 400ms
Enter
Sleep 4s
Type "# Two threads took the same element. With both fences, every model passes:"
Enter
Type `RUSTFLAGS='--cfg loom' cargo test -q -r --lib deque 2>&1 | grep 'test result'`
Sleep 400ms
Enter
Sleep 4s

Hide
Type "clear"
Enter
Show
Type "# 2. A waiting thread parks on a futex instead of spinning. CPU used while idle:"
Enter
Type `cargo test -q -r --test cpu_burn -- --ignored --nocapture --test-threads=1 2>&1 | grep used | sed 's/^\.*//'`
Sleep 400ms
Enter
Sleep 5s
Type "# Microseconds over 300 ms. The last line is a spinning thread, for comparison."
Enter
Sleep 3500ms

Hide
Type "clear"
Enter
Show
Type "# 3. Benchmarked against crossbeam (million items/s, full run on an Apple M4):"
Enter
Type `awk '/### MPMC/,/### Asym/' docs/BENCHMARKS.md | grep -E 'queue \||(lockfree|crossbeam) \| (1|8) \| (1|8) '`
Sleep 400ms
Enter
Sleep 4s
Type "# Faster with one producer, slower under heavy contention. Both are in the docs."
Enter
Sleep 3500ms

Hide
Type "clear"
Enter
Show
Type "# 4. And the whole suite: unit, property, concurrency and regression tests."
Enter
Type `cargo test --workspace -r 2>&1 | awk '/^test result/ {p+=$4; f+=$6} END {print p" passed, "f" failed"}'`
Sleep 400ms
Enter
Sleep 12s
Type "# Source, design notes and every benchmark: github.com/anmol0b/parkring"
Enter
Sleep 4s
