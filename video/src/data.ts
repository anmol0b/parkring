// Every number and quote shown in the video, in one place.
// Sources: docs/BENCHMARKS.md, docs/DESIGN.md, docs/SCQ.md, docs/DEQUE.md,
// docs/POOL.md, README "What the verification found", and git 76feaa4.

export const title = {
  name: "parkring",
  tagline: "Concurrency primitives in Rust — from the paper to proof.",
};

export const origin = {
  caption: "It started as a take-home assignment.",
  file: "src/queue/lockfree.rs  ·  original submission (abridged)",
  code: [
    "pub fn try_push(&self, item: T) -> Result<(), T> {",
    "    let pos = self.tail.load(Relaxed);",
    "    let slot = &self.slots[pos & (self.capacity - 1)];",
    "    if slot.sequence.load(Acquire) == pos {",
    "        match self.tail.compare_exchange(pos, pos + 1, ..) {",
    "            Ok(_) => { /* write, publish */ return Ok(()); }",
    "            Err(_) => return Err(item),   // lost a race ≠ full",
    "        }",
    "    } else {",
    "        return Err(item);                // stale position ≠ full",
    "    }",
    "}",
  ],
  badLines: [6, 9],
  facts: [
    "65 rejected pushes on a queue that was never more than half full",
    "capacity 3: accepted 3 items, returned 0",
  ],
};

export const built = {
  caption: "So I audited it — then built more.",
  cards: [
    { name: "Vyukov queue", detail: "spin, then park" },
    { name: "SCQ", detail: "Nikolaev, DISC 2019" },
    { name: "Chase-Lev deque", detail: "Lê et al., PPoPP 2013" },
    { name: "Work-stealing pool", detail: "join · spawn · install" },
    { name: "Futex parking", detail: "futex(2) · __ulock" },
  ],
};

export type Vignette = {
  tool: string;
  command: string;
  output: string;
  verdict: string;
};

export const breaking = {
  caption: "Then I tried to break it.",
  vignettes: [
    {
      tool: "loom",
      command: "cargo test --test loom",
      output: "deadlock; threads = [(Id(0), Blocked), (Id(1), Terminated)]",
      verdict: "Lost wakeup → protocol rebuilt on what loom can check",
    },
    {
      tool: "loom · mutant",
      command: "--cfg parkring_mutant=\"deque_no_pop_fence\"",
      output: "an element was taken twice: [0, 1, 1]",
      verdict: "Remove a fence and loom finds it. CI requires that.",
    },
    {
      tool: "Miri",
      command: "cargo miri test --test pool",
      output: "Undefined Behavior: deallocating while item is strongly protected",
      verdict: "A latch held &self as the waiter freed it → raw pointers",
    },
    {
      tool: "stress",
      command: "ScqQueue, capacity 1, 3 + 3 threads",
      output: "hung 11 times in 40 runs",
      verdict: "A hole in the paper's threshold → fixed: 0 hangs in 60",
    },
  ] satisfies Vignette[],
};

export type Bar = { label: string; ours: number; theirs: number; unit: string; them: string };

export const results = {
  caption: "Benchmarked against the crates you'd actually use.",
  bars: [
    { label: "Queue, 1 producer + 1 consumer", ours: 91, theirs: 80, unit: "M items/s", them: "crossbeam" },
    { label: "Deque, one thief draining", ours: 85, theirs: 70, unit: "M items/s", them: "crossbeam-deque" },
    { label: "Pool, fib(32) speedup on 8 threads", ours: 5.3, theirs: 5.3, unit: "×", them: "Rayon" },
  ] satisfies Bar[],
  idle: {
    ours: { latency: "9.4 µs", cpu: 1.7 },
    spin: { latency: "0.3 µs", cpu: 100 },
    note: "A parked consumer: wake latency vs CPU burned while idle",
  },
};

export const losses = {
  caption: "And where it loses.",
  lines: [
    "SCQ is lock-free, yet 5–8× slower on this machine. Profiled why.",
    "crossbeam's queue is faster under contention.",
  ],
};

export const outro = {
  badges: ["loom", "Miri", "proptest", "17 CI jobs"],
  url: "github.com/anmol0b/parkring",
  footer: "Rust · MIT OR Apache-2.0",
};
