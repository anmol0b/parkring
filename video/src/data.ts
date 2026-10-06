// Every number, quote and line of terminal output in the video, in one place.
// Copy is lowercase and first person, with no em or en dashes (`npm run lint:copy`).
// Sources: docs/BENCHMARKS.md, docs/DESIGN.md, docs/SCQ.md, docs/DEQUE.md,
// docs/POOL.md, the README's "What the verification found", and git 76feaa4.
import { line, PROMPT_CWD, type Line, type Session } from "./components/Terminal";

const promptLine = (cmd: string): Line => ({
  segs: [
    { text: PROMPT_CWD, tone: "ours" },
    { text: " $ ", tone: "muted" },
    { text: cmd },
  ],
});
const blank = (wait = 1): Line => line("", "text", { wait });

/** Opens cold on a real failure: the deque with its pop fence removed. Output captured from a real run. */
export const coldOpen = {
  session: {
    before: [promptLine(`export RUSTFLAGS='--cfg loom --cfg parkring_mutant="deque_no_pop_fence"'`)],
    command: "cargo test -r --lib pop_racing",
    output: [
      line("    Finished `release` profile [optimized + debuginfo] target(s) in 2.00s", "muted", { wait: 10 }),
      line("     Running unittests src/lib.rs (target/release/deps/parkring-0172fe3c5ef08229)", "muted", { wait: 3 }),
      blank(),
      line("running 1 test"),
      {
        segs: [
          { text: "test deque::loom_tests::pop_racing_two_steals_never_double_takes ... " },
          { text: "FAILED", tone: "fail", bold: true },
        ],
        wait: 18,
      },
      blank(4),
      line("failures:"),
      blank(),
      line("---- deque::loom_tests::pop_racing_two_steals_never_double_takes stdout ----"),
      blank(),
      line(
        "thread 'deque::loom_tests::pop_racing_two_steals_never_double_takes' (8124006) panicked at src/deque/loom_tests.rs:62:5:",
        "muted",
      ),
      line("assertion `left == right` failed: an element was taken twice: [0, 1, 1]", "fail", { hit: true }),
      line("  left: [0, 1, 1]"),
      line(" right: [0, 1]"),
    ],
  } satisfies Session,
  caption: "i broke this on purpose.",
};

/** The first version of `try_push`, verbatim from `git show 76feaa4:src/queue/lockfree.rs`, lines 26 to 49. */
export const origin = {
  file: "src/queue/lockfree.rs",
  rev: "76feaa4, the first version",
  code: [
    [26, "    pub fn try_push(&self, item: T) -> Result<(), T> {"],
    [27, "        let pos = self.tail.load(Ordering::Relaxed);"],
    [28, "        let slot = &self.slots[pos & (self.capacity - 1)];"],
    [29, "        let seq = slot.sequence.load(Ordering::Acquire);"],
    [30, "        if seq == pos {"],
    [31, "            match self"],
    [32, "                .tail"],
    [33, "                .compare_exchange(pos, pos + 1, Ordering::AcqRel, Ordering::Relaxed)"],
    [34, "            {"],
    [35, "                Ok(_) => {⋯}"], // lines 35 to 41 folded, as an editor would
    [42, "                Err(_) => {"],
    [43, "                    return Err(item);"],
    [44, "                }"],
    [45, "            }"],
    [46, "        } else {"],
    [47, "            return Err(item);"],
    [48, "        }"],
    [49, "    }"],
  ] as [number, string][],
  folded: 35,
  notes: {
    43: "lost a CAS race. the queue isn't full",
    47: "stale read. not full either",
  } as Record<number, string>,
  facts: [
    "4 threads pushing 8 items each into 64 slots: 65 rejected pushes",
    "capacity 3: accepted 3 pushes, returned none of them",
  ],
  caption: "this was my first version of the queue.",
  then: "it said full when it wasn't.",
};

/** `tree src -d`, annotated. */
export const built = {
  session: {
    command: "tree src -d --noreport",
    output: [
      ["src", ""],
      ["├── deque", "Chase-Lev deque, Lê et al. PPoPP 2013"],
      ["├── pool", "work-stealing pool: join, spawn, install"],
      ["├── queue", "Vyukov MPMC queue, spin then park"],
      ["│   └── scq", "SCQ, lock-free, Nikolaev DISC 2019"],
      ["├── sync", ""],
      ["│   ├── futex", "futex(2) on linux, __ulock on macOS"],
      ["│   └── wait_queue", ""],
      ["├── traits", ""],
      ["└── utils", ""],
    ] as [string, string][],
  },
  caption: "so i audited it,",
  then: "and kept going.",
};

export type Shot = { session: Session; caption: string; then?: string };

export const breaking: Shot[] = [
  {
    session: {
      before: [line("# the first parking protocol, before the rewrite", "dim"), promptLine("export RUSTFLAGS='--cfg loom'")],
      command: "cargo test -r --test loom consumer_is_not_lost",
      output: [
        line("     Running tests/loom.rs (target/release/deps/loom-5d1e0b7c2a94f318)", "muted", { wait: 8 }),
        blank(),
        line("running 1 test"),
        {
          segs: [
            { text: "test consumer_is_not_lost_when_parking_races_a_push ... " },
            { text: "FAILED", tone: "fail", bold: true },
          ],
          wait: 22,
        },
        blank(3),
        line("---- consumer_is_not_lost_when_parking_races_a_push stdout ----"),
        line(
          "thread 'consumer_is_not_lost_when_parking_races_a_push' panicked at loom-0.7.2/src/rt/execution.rs:216:13:",
          "muted",
        ),
        line("deadlock; threads = [(Id(0), Blocked), (Id(1), Terminated)]", "fail", { hit: true }),
        blank(),
        line("test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 13 filtered out", "muted"),
      ],
    },
    caption: "then i tried to break it.",
    then: "loom found a lost wakeup in my parking code.",
  },
  {
    // Captured from a real run.
    session: {
      before: [line("# the pop fence restored", "dim"), promptLine("export RUSTFLAGS='--cfg loom'")],
      command: "cargo test -r --lib deque",
      output: [
        line("    Finished `release` profile [optimized + debuginfo] target(s) in 0.09s", "muted", { wait: 8 }),
        line("     Running unittests src/lib.rs (target/release/deps/parkring-ca617838a5681863)", "muted", { wait: 3 }),
        blank(),
        line("running 6 tests"),
        line("test deque::loom_tests::stealer_outlives_worker ... ok", "text", { wait: 5 }),
        line("test deque::loom_tests::pop_racing_two_steals_never_double_takes ... ok", "text", { wait: 4 }),
        line("test deque::loom_tests::slot_reuse_across_laps ... ok", "text", { wait: 3 }),
        line("test deque::loom_tests::two_thieves_and_the_owner ... ok", "text", { wait: 6 }),
        line("test deque::loom_tests::last_element_goes_to_exactly_one_side ... ok", "text", { wait: 2 }),
        line("test deque::loom_tests::growth_concurrent_with_steal ... ok", "text", { wait: 3 }),
        blank(),
        line("test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.01s", "pass", {
          hit: true,
        }),
      ],
    },
    caption: "remove one fence and loom catches it.",
    then: "put it back and every model passes. CI checks both.",
  },
  {
    session: {
      before: [line("# the pool's latch, before it took raw pointers", "dim")],
      command: "cargo +nightly miri test --test pool",
      output: [
        line("     Running tests/pool.rs (target/miri/aarch64-apple-darwin/debug/deps/pool-8c2f61d0e4b9a7c5)", "muted", {
          wait: 10,
        }),
        blank(),
        line("running 9 tests"),
        line("test fib_matches_sequential_at_several_thread_counts ... ", "text", { wait: 14 }),
        blank(),
        line("error: Undefined Behavior: deallocating while item is strongly protected", "fail", { wait: 16, hit: true }),
        line("  = help: this indicates a potential bug in the program: it performed an invalid operation,", "muted"),
        line("          but the Stacked Borrows rules it violated are still experimental", "muted"),
        blank(),
        line("error: test failed, to rerun pass `--test pool`", "fail"),
      ],
    },
    caption: "miri: a latch was freed while still borrowed.",
    then: "fixed with raw pointers.",
  },
  {
    session: {
      before: [line("# SCQ, capacity 1, 3 producers and 3 consumers, 40 runs. H means it hung.", "dim")],
      command:
        "for i in $(seq 40); do cargo test -q -r --test regressions scq_more >/dev/null 2>&1 && printf . || printf H; done",
      recalled: true,
      output: [
        {
          // 11 hangs in 40 runs, as in the README.
          segs: "..H...H..H....H...H.HH....H...H.....H..H".split("").map((c) => ({
            text: c,
            tone: c === "H" ? ("fail" as const) : ("muted" as const),
            bold: c === "H",
          })),
          wait: 6,
          stream: 9,
        },
      ],
    },
    caption: "the SCQ paper's threshold had a hole.",
    then: "11 hangs in 40 runs. after the fix, 0 in 60.",
  },
];

export type Bar = { label: string; ours: number; theirs: number; unit: string; them: string };

export const results = {
  axis: "million items / s, Apple M4, higher is better",
  bars: [
    { label: "queue, 1 producer, 1 consumer", ours: 91, theirs: 80, unit: "", them: "crossbeam ArrayQueue" },
    { label: "deque, one thief draining", ours: 85, theirs: 70, unit: "", them: "crossbeam-deque" },
  ] satisfies Bar[],
  pool: { label: "pool, fib(32) on 8 threads", ours: "1.36 ms", theirs: "1.36 ms", them: "Rayon" },
  caption: "benchmarked against the crates you'd actually reach for.",
  idle: {
    header: ["", "wake latency", "CPU while idle"],
    rows: [
      { who: "parkring, parked", latency: "9.4 µs", cpu: 1.7 },
      { who: "std channel, parked", latency: "8.8 µs", cpu: 1.3 },
      { who: "crossbeam, spinning", latency: "0.3 µs", cpu: 100 },
    ],
    caption: "idle, it sleeps like a channel does. a spinning queue burns a core.",
  },
};

export const losses = {
  lines: [
    { text: "crossbeam is faster under contention.", detail: "8 producers and 8 consumers: 57 vs 51 million items / s" },
    { text: "SCQ is 5 to 8x slower on this machine.", detail: "it's here for the lock-free guarantee. i profiled why." },
  ],
  caption: "and where it loses.",
};

export const outro = {
  session: {
    cwd: "~/my-app",
    command: "cargo add parkring",
    output: [
      line("    Updating crates.io index", "muted", { wait: 10 }),
      line("      Adding parkring v1.0.0 to dependencies", "text", { wait: 8 }),
      line("             Features:", "muted"),
      line("             + std", "muted"),
    ],
  } satisfies Session,
  name: "parkring",
  url: "github.com/anmol0b/parkring",
  footer: "checked with loom, Miri, fuzzing and ThreadSanitizer. MIT licensed.",
};
