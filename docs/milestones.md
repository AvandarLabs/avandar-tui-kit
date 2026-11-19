# Milestones

The ordered plan from today (an umbrella crate with no code, plus the docs) to
all eight building blocks in use by the source apps. Each milestone is meant
to be specific enough to write its implementation plan from.

The designs are settled in [`specs/architecture.md`](specs/architecture.md),
[`specs/ratatui-mouse.md`](specs/ratatui-mouse.md) and
[`decisions.md`](decisions.md). This page orders the work and says what ends
each piece of it. It does not reopen a design. "M3" means Milestone 3. The
mouse spec's own rules, also named M1 to M8, are always called "rule M1" and
so on here.

## Summary

| # | Milestone | Builds | Adopted by | Size | Depends on |
| --- | --- | --- | --- | --- | --- |
| 0 | Test rig | The test layers, the capability probe, CI | none: it measures terminals | Large | none |
| 1 | `ratatui-mouse` | `ratatui-mouse` | dif | Large | M0 |
| 2 | Publishing to crates.io | release-plz, the `ratatui-core` switch | dif moves to the published crate | Small | M1 |
| 3 | `avandar-tui-term` | `avandar-tui-term`, the probe's permanent home | dif, huckleberry-cli | Large | M2 |
| 4 | Widgets: style | `avandar-tui-widgets` with `style` | huckleberry-cli, dif | Medium | M3 |
| 5 | `avandar-tui-charts` | `avandar-tui-charts` | huckleberry-cli | Medium | M4 |
| 6 | `avandar-tui-tasks` | `avandar-tui-tasks` | huckleberry-cli, dif | Medium | M2 |
| 7 | `avandar-tui-pty` | `avandar-tui-pty` | dif | Large | M3, M4 |
| 8 | `avandar-tui-agent` | `avandar-tui-agent` | dif | Large | M7 |
| 9 | `avandar-tui-view` | `avandar-tui-view` | dif, huckleberry-cli | Medium | M4 |
| 10 | Widgets: layout and command | `layout`, `command`, `input::line` | dif, huckleberry-cli | Large | M4 |
| 11 | Widgets: pick and tree | `pick`, `tree` | huckleberry-cli | Large | M10 |
| 12 | Widgets: input | the rest of `input` | huckleberry-cli, dif | Large | M11 |
| 13 | brain adopts the toolkit | Only what brain's adoption shows is missing | brain | Large | M3 and M7 to start, M8 to M12 to finish, and brain's own upgrade |

Every one of the eight crates has an adopting app by the end of M12. M13
brings in the last source app. Releases move from git tags to crates.io in
**M2** (see [Releases](#releases)).

## The source apps

| App | Repository | Where the UI code is |
| --- | --- | --- |
| dif | `AvandarLabs/avandar` | `agent-skills/public-skills/skills/diff-review/scripts/dif/`. Paths for dif below are relative to this directory, as in the inventory. |
| brain | `jpsyx/brain` | The repository root |
| huckleberry-cli | `jpsyx/huckleberry-cli` | The repository root |
| quickstarter | `AvandarLabs/quickstarter` | `templates/capabilities/rust-tui/` |

## How to read a milestone

Each milestone gives why it sits where it does, its goal, its scope (by crate
and module, with the inventory elements it covers), what is out of scope,
what it depends on, exit criteria that can be checked, its risks and the open
questions from the specs it settles, and a size.

| Size | Means |
| --- | --- |
| Small | Configuration and a few changes, no new terminal behavior |
| Medium | One crate or module family of pure logic or rendering, tested in layers 1 and 2, with one or two adoptions |
| Large | Terminal input and output tested in every layer, or many modules, or several adoptions |

## What every crate milestone includes

These apply to M1 and M3 to M12, and are not repeated in each.

1. **Spec first.** The milestone starts by writing the crate's spec in
   `docs/specs/`, in the shape of `ratatui-mouse.md`: goals, non-goals,
   modules, the public API as signatures, settings, a test plan mapped to the
   layers in `testing.md`, how the first app adopts it, and open questions
   with recommendations. The crate is created after its spec is committed.
   Every open question has an answer, in the spec or in `decisions.md`, before
   the release that carries the crate. M1's spec exists already.
2. **The crate's layers pass.** The layers the architecture spec's testing
   table gives the crate pass, run by whoever made the change: layers 5 and 6
   locally, layer 7 on the Windows runners.
3. **The workspace stays clean.** `just check`, the cross-target lint and CI
   on all three platforms pass, including the build at Rust 1.88 and one
   feature-check row per combination: no features, each feature alone, every
   feature.
4. **Dependencies are declared once.** Each new dependency goes into
   `[workspace.dependencies]` with its one-line reason. The umbrella gains the
   crate's feature, its pass-through features and its glue rows from the
   architecture spec.
5. **Docs are current.** `docs/architecture.md` (crate table, layout tree,
   dependencies table), `decisions.md` for any new decision, `terminals.md`
   for any new measurement, and the crate's own docs, which say how it
   degrades wherever a configuration lacks something.
6. **A release, then an adoption.** The crate ships in a release (see
   [Releases](#releases)). The named app then follows "How an app adopts a
   crate" in the architecture spec: it replaces its module with the part,
   deletes its local copy and the tests the crate now owns, and its own tests
   pass. The adoption is a change in that app's repository, and the milestone
   is done when that change is merged there.

## Milestone 0: Test rig

**Why first.** No crate that touches the terminal is done until it passes in
every configuration, and the measurements the specs rest on still have gaps.
The rig and the probe close both before any crate code exists.

**Goal.** Build every test layer from `testing.md` in the repository, and use
the capability probe to measure every open entry in `terminals.md`.

### Scope

| Piece | What is built |
| --- | --- |
| `rig/probe` | An unpublished workspace crate (`publish = false`, added to the workspace members). Its library holds the probe's pure reply parsers (mode reports, the version reply, the color reply, the cursor position) with unit tests. Its example, `capability_probe`, is the program the harnesses drive. |
| `rig/xterm` | The Node harness: the browser build of xterm.js at the version Orca ships (6.0.0 today) with the same Unicode 11 add-on, a headless browser driven through Playwright, and node-pty for the pseudo-terminal (ConPTY on Windows). The lockfile is committed. |
| `rig/tmux` | The stock configuration (an empty file) and the full one (`mouse on`, `set-clipboard on`, `allow-passthrough on`, `extended-keys on`, `focus-events on`). The harness starts every tmux run in a private server with its own socket and one of these files. |
| `rig/iterm2` | The layer 6 driver: start the macOS virtual machine with VNC and a shared folder, open iTerm2 on a command, send pointer and key events over VNC (vncdotool), read the log from the shared folder, take a screenshot on failure, stop the machine. |
| `rig/linux` | The container image for Linux runs: Rust 1.88, Node, the headless browser's system libraries, and tmux 3.3, the supported minimum, so the floor is tested somewhere. CI's Linux jobs use the same image. |
| `rig/expected` | The probe's expected answers, one file per configuration. They are the machine form of the tables in `terminals.md` and change together with them. |
| `.github/workflows/` | CI, below |
| `justfile` | One recipe per layer, below |

The probe follows the architecture spec's rules for example programs: a
`--log` file of JSON lines, a ready line and a ready file written after its
modes, a documented exit key and a documented panic key, and no clock in its
decisions. Its sections:

- Identity: the version reply and the environment.
- The mouse-mode slot, and what the mouse spec's setup and teardown
  sequences (its rules M1 and M5) leave set.
- Mouse events with each button and modifier.
- Keys: Option or Alt with a letter, Alt+Enter, Shift+Enter, Ctrl+Enter,
  function keys.
- The kitty keyboard query, push and pop.
- Bracketed paste, and how a paste arrives.
- Focus events and synchronized output.
- The background color query, with the time the reply took. It keeps
  listening for two seconds.
- OSC 52, plain and wrapped for tmux.
- Cell size.
- Widths: one sample per row of the width table, per rule of the portable-text
  rewrite, and per glyph the specs and inventory plan to draw (the ellipsis,
  the middle dot, the menu chevron, block eighths, braille, box drawing).

| Layer | Recipe | One command that |
| --- | --- | --- |
| 1 | `just test` | Exists already |
| 1 | `just linux <command>` | Runs a command in the Linux container, for example `just linux cargo test --workspace --all-features` |
| 2 | `just lint-targets` | Runs clippy for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` |
| 3 | `just xterm <program> [scenario]` | Runs the harness |
| 4 | `just xterm-tmux <config> <program> [scenario]` | Runs the harness inside a private tmux server, `config` being `stock` or `full` |
| 5 | `just orca <program>` | Runs a program in a live Orca terminal through Orca's command line. The terminal is created without focus, so a person's typing cannot land in it. |
| 6 | `just iterm2 <program> [scenario]` | Runs the virtual machine driver |
| 7 | `just windows` | Shows the Windows jobs of the current branch's latest CI run and downloads their logs |

CI, on every pull request and every pushed branch:

| Job | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Format check | | Yes | |
| Clippy, tests, docs | Yes | Yes | Yes |
| Build and test at Rust 1.88 with every feature | Yes | Yes | Yes |
| Feature checks, one command per row | | Yes | |
| xterm.js harness | Yes | Yes | Yes |
| xterm.js harness in stock and full tmux | Yes | Yes | |
| Event logs uploaded | On failure | On failure | Always |

What M0 measures. Every "Verify" in `terminals.md`, the Windows unknowns the
specs list, and the width samples the text width policy needs:

| # | Entry | Configuration | How the rig measures it |
| --- | --- | --- | --- |
| 1 | Wheel direction | iTerm2 | Wheel events over VNC, with the guest's natural-scrolling setting off and on. The probe logs the direction it received. |
| 2 | Wheel direction against Orca's own setting | Orca | Not measurable by an agent: Orca's command line cannot move a pointer. Recorded as such. |
| 3 | Shift with a click, and Alt with a click and with a drag (mouse spec question 1) | xterm.js on Linux and Windows | The harness sends each. The probe logs whether it arrived. |
| 4 | Native selection while an app has the mouse | iTerm2 | A drag over VNC with each modifier, then the guest's clipboard is read |
| 5 | Option or Alt with a letter | iTerm2 | Key events over VNC, with iTerm2's Option key at its default and at "Esc+". The bytes received. |
| 6 | Alt+Enter, Shift+Enter, Ctrl+Enter | iTerm2 | Key events over VNC, with the kitty flags pushed and without |
| 7 | OSC 52 | iTerm2 | The probe writes a unique string with clipboard access off (the default) and on. The guest's clipboard is read through the virtual machine's command channel. |
| 8 | OSC 52 | xterm.js in Orca | The probe writes a unique string in a live Orca terminal, and the agent reads the clipboard of the machine that shows Orca's window. Where an agent cannot read that clipboard, the row says so. |
| 9 | Mouse in stock tmux | tmux in xterm.js and in iTerm2 | Injected click, drag and wheel. The probe logs none of them. |
| 10 | Mouse in full tmux | tmux in xterm.js and in iTerm2 | Injected events after the probe asks for the mouse, logged at the expected cells |
| 11 | Background color reply in tmux inside iTerm2: late or missing (architecture question 6) | iTerm2 | The probe's two-second listen logs when the reply came, or that none did |
| 12 | OSC 52 in stock and full tmux | tmux in both terminals | Plain and wrapped forms. tmux's buffer and the outer terminal's clipboard are read back. |
| 13 | iTerm2's tmux integration (`tmux -CC`) | iTerm2 | The whole probe in an integration window, with both tmux configurations |
| 14 | Which tracking mode the pseudo-console asks for, and whether bare moves arrive (mouse spec question 2) | xterm.js on the Windows runner | The probe turns mouse input on through crossterm. The harness reads xterm.js's tracking mode and moves the pointer with no button down. |
| 15 | Whether the background color reply crosses the pseudo-console (architecture question 5) | xterm.js on the Windows runner | The probe's color section, with the reply's delay |
| 16 | Whether a paste arrives as one burst (architecture question 5) | xterm.js on the Windows runner | A harness paste. The probe logs whether its key events came in one read. |
| 17 | Every other row of the terminal tables, again on Windows | xterm.js on the Windows runner | The whole probe through the pseudo-console |
| 18 | Agents installed as `.cmd` shims (architecture question 4) | The Windows runner | A Windows-only test in `rig/probe` runs a stub `.cmd` through `cmd.exe /D /S /C` with arguments that hold spaces, quotes, `%`, `^`, `&` and `!`, and records what arrived |
| 19 | The rainbow flag alone, marked "to be confirmed" in the text width table | Every configuration | A width sample |
| 20 | The glyphs the toolkit plans to draw | Every configuration | A width sample per glyph, so the allowed set of the text width policy is measured, not assumed |

### Out of scope

- Every building block crate and its lab program. Each comes with its crate.
- Testing toolkit behavior. The probe measures terminals.
- The best-effort terminals named in `terminals.md`.
- A local Windows machine, iTerm2 in CI, and any comparison of pixels.

### Depends on

- The repository on GitHub with Actions turned on, and permission to push a
  branch, since layer 7 runs only there (see "Windows is tested on CI runners
  only" in `decisions.md`).
- An Apple Silicon Mac with the virtual machine prepared once (iTerm2
  installed and its first-run prompts answered), a container runtime, and
  Orca for layer 5.

### Exit criteria

- Every row of the measurement table has a value in `terminals.md`, or a
  sentence saying why an agent cannot measure it and what the toolkit does
  regardless. No other "Verify" is left.
- `terminals.md` gains a Windows column for xterm.js through the
  pseudo-console, and its tmux table records tmux 3.3 beside 3.8 wherever the
  two differ.
- The probe's output matches `rig/expected` in all thirteen configurations:
  xterm.js on macOS, Linux and Windows; stock and full tmux in xterm.js on
  macOS and Linux; iTerm2 alone, in stock tmux, in full tmux and in
  `tmux -CC`; Orca alone and in tmux.
- Thirty consecutive identical runs of the probe in layers 3 and 4, on macOS
  and in the container, and ten in layer 6 from a cold start of the virtual
  machine. No fixed sleep anywhere in the rig.
- Layer 5 reports the xterm.js version the harness pins.
- CI is green on all three platforms on a pushed branch, and `just windows`
  downloads the Windows logs.
- `just --list` shows a recipe for each layer, and each is one command.
- Docs: `testing.md` (every piece in its status table built, how to run each
  layer, the virtual machine's one-time preparation as a list of settings, the
  reliability counts), `terminals.md`, `architecture.md` (the layout gains
  `rig/` and `.github/`), `decisions.md` (the probe lives in the rig until
  `avandar-tui-term` exists; tmux 3.3 is tested in the Linux container), and
  the README's developing section.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Architecture question 5 (the color reply and paste on Windows) | Measured here (rows 15, 16). The design is proved with toolkit code in M3. |
| Architecture question 6 (tmux inside iTerm2) | Measured here (row 11). No design change. M3 asserts the late-reply handling. |
| Architecture question 4 (`.cmd` shims) | Measured here (row 18). Proved with toolkit code in M7 and M8. |
| Mouse question 1 (Alt as a second selection modifier) | The measurement is here (row 3). The yes or no is M1's. |
| Mouse question 2 (Windows tracking mode) | Measured here (row 14). Asserted by M1's scenario X1 on the runners. |
| Mouse questions 3 and 4 (iTerm2 wheel direction, native selection modifier) | Measured here (rows 1, 4). M1 documents them. |
| Getting Option into the guest over VNC: a VNC client's Alt can arrive as Command | The driver finds the key that arrives as Option before rows 5 and 6 are measured |
| node-pty is a native module and needs a prebuilt binary or build tools on the Windows runner | Settled while CI is built |
| The hosted runner's pseudo-console may differ from a desktop Windows | Every Windows measurement records the runner image and the Windows build it ran on |
| Orca can move to a new xterm.js | Layer 5 compares versions. A mismatch re-pins the harness and reruns the probe. |
| Which add-ons Orca loads besides Unicode 11 (a clipboard add-on changes row 8's harness counterpart) | Found from Orca before the harness pins its add-ons |

**Size: large.** A Node harness, a virtual machine driver, a container, CI on
three platforms, and twenty measurements across thirteen configurations.

## Milestone 1: `ratatui-mouse`

**Why here.** Its spec is the one already written, it depends on no other
crate, and dif can adopt it with no upgrade first.

**Goal.** Build `ratatui-mouse` to its spec and have dif use it from the
`v0.1.0` tag.

### Scope

- `crates/ratatui-mouse` as the spec lays it out: `modes` (with the platform
  split in one file), `hit`, `clicks`, `content`, `surface`, `words`,
  `selection`, `highlight`, `passthrough`, `select_mode`, `controller` (split
  into submodules by gesture kind), `settings`, `outcome`, `error`, and
  `vt100` behind the feature of that name.
- The `mouse_lab` example, as the spec's test plan describes it.
- The harness, tmux and virtual machine scenarios X1 to X14 for `mouse_lab`.
- In the root manifest: `unicode-segmentation` 1, `vt100` 0.16, and
  `ratatui-mouse` with a path and a version. `clippy.toml` disallows calling
  `unicode-width` and ratatui's `Span::width` and `Line::width`, since this is
  the first crate that measures text.
- The umbrella's `mouse` feature.
- The first release: `CHANGELOG.md`, and the checklist of judgments in
  `testing.md` (edge-scroll interval, its maximum, the wheel step, whether the
  highlight reads well), which the architecture spec's release steps refer to
  and which does not exist yet.

Inventory elements: dif's mouse modes, pane hit-testing and pointer clamping,
multi-click tracker, word and line finders, selection model with highlight
and text extraction, and drag-to-copy controller. brain's wheel routing by
pane, as hit areas. New: pass-through and select mode.

dif's adoption, in the spec's order:

1. `HitAreas<Target>` and the controller with pass-through off, leaving every
   existing behavior and test intact. The poll timeout becomes the smaller of
   50 ms and the time to `next_deadline()`, and `tick` runs on each pass.
2. The deletions: `src/tui/multi_click.rs` and `src/tui/selection.rs` go, and
   `src/tui/app/select.rs` becomes a `SurfaceSet<Target>` (a `Vt100Surface`
   for the agent pane and the log view, dif's own surface over the `words`
   functions for its markdown views). Teardown writes the full sequence of
   the spec's rule M5, and a panic hook writes it too.
3. Pass-through on, and a "Select text" palette command that starts select
   mode in the focused pane.

`src/clipboard.rs` stays until M3.

### Out of scope

- The spec's non-goals, drag-to-resize included (see [Not planned](#not-planned)).
- The switch to `ratatui-core` (M2).
- The order of the mouse step among the other terminal modes, and the
  clipboard (M3).
- Hit-area registration from widgets (M10), and a pane reporting that output
  arrived (M7).
- brain's adoption (M13).

### Depends on

M0.

### Exit criteria

- Layer 1: every row of the spec's layer 1 table, on macOS, in the Linux
  container and on the Windows runner. The shared surface-rules check runs
  against `LineSurface` and `Vt100Surface`.
- Layer 2: the `modes` platform split lints clean for the Windows and Linux
  targets.
- Layers 3 and 7: X1 to X14 in the harness on macOS, Linux and Windows. X3
  runs on macOS and Linux. X11 asserts the behavior M0 measured on each
  platform.
- Layer 4: X1, X2 and X13 in stock tmux, and X1 to X14 in full tmux, on macOS
  and Linux.
- Layer 5: the probe's mouse section in a live Orca terminal matches
  `rig/expected`, then `mouse_lab` passes X1 with `--query-modes`, and X13.
- Layer 6: X1 to X10 and X12 to X14 in iTerm2, alone and inside both tmux
  configurations, plus the spec's three iTerm2 checks.
- Feature checks: no features, `vt100`, every feature.
- A person has done the release checklist, and any default it moved changed
  before the tag.
- `v0.1.0` is tagged on `main`.
- dif depends on `ratatui-mouse` at `v0.1.0` with `vt100`, all three adoption
  steps are merged, and dif's tests pass after each.
- Docs: the crate's docs carry the keyboard equivalents table and every
  documented limit (stock tmux, Shift on xterm.js for Linux and Windows, any
  motion served as button motion, the Windows tracking mode, iTerm2's native
  selection modifier). `architecture.md` gains the crate's row, its place in
  the layout and the `unicode-segmentation` and `vt100` rows. `terminals.md`
  records X11 and the Windows tracking mode. `decisions.md` records the answer
  to mouse question 1.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Mouse question 1: Alt as a second selection modifier | The maintainer's yes or no, given M0's measurement, before the tag, since it widens a settled decision. Recorded in `decisions.md`, and the `selection_modifiers` default follows it. |
| Mouse question 2: Windows tracking mode | X1 on the Windows runner asserts what M0 measured. If it is 1003, the crate's docs say so. |
| Mouse question 3: wheel direction | The crate reports the direction the terminal sends and never flips it. Stated in the crate's docs. |
| Mouse question 4: iTerm2's native selection modifier | M0's answer goes in the crate's docs for users |
| Mouse question 5: `ratatui-core` | M2 |
| Mouse question 6: clearing a pane's selection when it prints | M7 |
| `vt100` 0.16 may not expose everything `Vt100Surface` needs (the scrollback length, wrapped flags across scrollback, the program's mouse request) | Solved inside the surface, without forking `vt100`. If that is impossible, it is a spec change, raised before the code. |
| Step 1 of dif's adoption must change no behavior | dif's existing tests are the check |
| X7 depends on edge-scroll ticks on a loaded runner | It waits for the logged selection to reach a known history line, never for time |

**Size: large.** Fifteen modules, the full seven-layer plan with fourteen
scenarios in five places, and an adoption in another repository.

## Milestone 2: Publishing to crates.io

**Why here.** `decisions.md` ("Git tags first, crates.io later") says
publishing starts when the first building block has a real user, which is
exactly how M1 ends.

**Goal.** Publish the workspace to crates.io through release-plz and move
dif from the git tag to the published crate.

### Scope

- `ratatui-mouse` depends on `ratatui-core` instead of `ratatui`, declared once
  in the root manifest. `mouse_lab` keeps `ratatui` as a dev-dependency.
- Package metadata for `ratatui-mouse` and `avandar-tui-kit`: description,
  readme, keywords, categories, and docs.rs settings that build every feature
  and show each feature's name on the items behind it.
- release-plz configuration and its workflow: one version for the workspace,
  changelogs from Conventional Commits (taking over steps 3 and 4 of the
  release in the architecture spec), and publishing in dependency order.
- A packaging check (`cargo publish --dry-run`) for each crate in CI.
- The first publish: `ratatui-mouse` and `avandar-tui-kit`.
- dif depends on the published version.

### Out of scope

- Publishing a crate that does not exist yet, or a placeholder to hold a name.
- Versioning crates apart.
- 1.0.

### Depends on

M1. A person adds the registry token as a repository secret and merges the
first release pull request.

### Exit criteria

- Both crates are on crates.io at the workspace's one version, and docs.rs has
  built both with the `vt100` and `mouse` feature labels.
- `cargo tree -p ratatui-mouse` shows `ratatui-core` and not `ratatui` among
  its normal dependencies.
- Layers 1, 2, 3 and 7 pass after the switch. It changes no terminal
  behavior, so layers 4 to 6 are not rerun.
- The packaging check passes in CI for each crate.
- dif builds from crates.io and its tests pass.
- Docs: the architecture spec's release section and "How an app adopts a
  crate" use crates.io versions instead of tags, `decisions.md` records that
  publishing started and that `ratatui-mouse` keeps the workspace version,
  the README tells an app how to depend on the published crates, and
  `testing.md`'s release steps name release-plz.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Mouse question 5: depend on `ratatui-core` | Here. If a type the crate uses is not in `ratatui-core`, the crate keeps `ratatui` for it and its spec says why. |
| Architecture question 3: does `ratatui-mouse` keep the workspace version on crates.io | Here: yes, until it has users outside the toolkit. Recorded in `decisions.md`. |
| A crate name is taken on crates.io | Checked before anything else. A rename is the maintainer's decision, recorded in `decisions.md`. |
| A published version is permanent | Publish only a commit whose layers are green |
| The first published version, given the existing `v0.1.0` tag | Chosen by release-plz from the commits since that tag, by the semver rules |

**Size: small.** Configuration, metadata and one dependency swap, with no new
behavior.

## Milestone 3: `avandar-tui-term`

**Why here.** dif's next parts are the terminal guard and the clipboard,
brain's first part is the guard, and this crate is the probe's permanent
home. Its only workspace dependency is `ratatui-mouse`, behind a feature.

**Goal.** Build `avandar-tui-term`, move the capability probe into it, and
have dif and huckleberry-cli borrow and give back the terminal through it.

### Scope

- `docs/specs/avandar-tui-term.md`, then `crates/avandar-tui-term` with the
  architecture spec's modules: `guard` (ordered steps for raw mode, the
  screen, paste, focus, keyboard flags and the mouse, each recorded so a
  partial start rolls back; idempotent restore that keeps going on errors and
  runs on drop; a full-screen profile and an inline-prompt profile), `panic`,
  `screen` (stdout, stderr, or the controlling terminal: `/dev/tty`, and
  `CONOUT$` on Windows), `keyboard`, `input` (including joining a Windows
  burst into one paste), `colors` (the query, the reply parser over crossterm
  events, the 200 ms wait, the two-second armed window, the 0.179 luminance
  threshold), `clipboard` (local tools per platform, OSC 52 plain and wrapped,
  its own small base64 encoder), and `environment`.
- The `mouse` feature: the guard's mouse step calls `ratatui-mouse`'s setup
  and teardown.
- Example programs: `capability_probe`, moved from `rig/probe` and rewritten
  over this crate's parts, and `term_lab`. `rig/probe` leaves the workspace:
  its parsers move into this crate or into the example, and its `.cmd`
  measurement stays recorded in `terminals.md` until M7 and M8 assert it with
  toolkit code.
- The umbrella's `term` feature, and its `mouse` with `term` glue.

Inventory elements: brain's terminal guard with recorded capabilities,
ordered startup and shutdown lifecycle, drawing on the controlling terminal,
and kitty flags in the guard. dif's terminal setup and teardown, clipboard,
and scattered environment checks. huckleberry-cli's terminal session owner on
stderr, scoped raw-mode guard, once-only panic hook, and bracketed paste.
quickstarter's terminal guard and panic hook.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| dif | Setup and teardown in `src/tui/mod.rs`, and the panic hook from M1 | The full-screen guard with the mouse step, and `panic` |
| dif | `src/clipboard.rs` and its environment checks | `clipboard` and `environment`, which add Linux and Windows routes |
| huckleberry-cli | The session owner in `src/tui/shell.rs` | The full-screen guard drawing on stderr, `input` for key-or-paste waits and draining with the force-quit chord, and `panic` |
| huckleberry-cli | `src/prompt/terminal.rs` | The inline-prompt profile |

dif keeps the bracketed paste step off until M7. It does not handle paste
events today, and turning the step on before the pane can forward a paste
would drop pastes.

### Out of scope

- The theme. This crate returns a color, and M4 turns it into a palette.
- brain's adoption (M13), and quickstarter's template (later, outside this
  plan).

### Depends on

M2. M1 for the `mouse` feature.

### Exit criteria

- Layers 1 to 7, with what the architecture spec's testing table asserts: the
  probe's answers match `rig/expected` in all thirteen configurations; every
  mode the guard set is reset after a clean exit, an error and a panic; each
  clipboard route emits its bytes; the background reply is parsed, and a late
  one is swallowed and returned as a color.
- The Windows rows of the platform policy for this crate are proved on the
  runner: the console output handle, kitty flags reported as `Unsupported`,
  paste joined from a burst, focus, presses only, and `clip.exe` fed UTF-16.
- Feature checks: no features, `mouse`, every feature.
- dif and huckleberry-cli adoptions merged, with their tests passing.
- Docs: the spec, `terminals.md` for anything the crate's own runs measured,
  and the crate's docs for the Windows behaviors of question 5.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Architecture question 5 (the color reply and paste on Windows) | Built as designed and asserted on the runner. No reply means the unknown palette is the documented Windows behavior. A paste that is not one burst is documented as arriving key by key. |
| Architecture question 6 (tmux inside iTerm2) | No design change. `term_lab` in layer 6 asserts that a late reply never reaches the app as keys. |
| Linux CI has no clipboard tool or display | The choice of route is a pure function tested on all three platforms. The real command runs where its tool exists. |
| crossterm's raw mode and events with the console output handle on Windows | Proved on the runner before the screen module is final |
| huckleberry-cli's lesson that clearing after a resume blocks on a cursor query | Carried into the guard's spec |

**Size: large.** Every module touches the terminal on three platforms, so
all seven layers and the crate's rows of the platform table apply, and two
apps adopt it.

## Milestone 4: Widgets: style

**Why here.** Three crates (charts, pty and view) need the `style` module
before they can start, it is huckleberry-cli's first part, and with
`avandar-tui-term` released the theme can detect the background.

**Goal.** Create `avandar-tui-widgets` with its `style` module, the one width
function and the theme, and have huckleberry-cli and dif draw through it.

### Scope

- `docs/specs/avandar-tui-widgets.md`, written in parts as the module
  families are built: here the crate-level part (features, dependencies,
  conventions) and the `style` section.
- `crates/avandar-tui-widgets` with `style::theme` (eleven roles, four
  palettes, picking one from a background color, `NO_COLOR`), `style::oklab`,
  `style::width` (`cell_width` and `text_to_portable`), `style::wrap`
  (wrapping styled spans, soft-wrap for input, measuring wrapped height) and
  `style::fit` (truncation, filename elision, fitting columns to a width).
- The test that fails when `style` imports a sibling module.
- The glyph check every drawing crate runs, against the set M0 measured.
- The umbrella's `widgets` feature.

Inventory elements: huckleberry-cli's semantic theme and column-width
allocator. quickstarter's semantic theme. brain's dark palette (replaced by
roles), Oklab color math, label elision, truncation, soft-wrap for input,
and logical-to-visual row measuring. dif's style-preserving word wrap. The
character-count widths of all three apps.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| huckleberry-cli | The generic roles of `src/theme.rs` in the TUI (its own roles stay in its own map beside the theme) | `style::theme`, detected through `avandar_tui_term::colors` |
| huckleberry-cli | The column-width allocator in `src/listing/layout.rs`, and character-count widths | `style::fit` and `style::width` |
| dif | RGB colors repeated across its drawing files | Theme roles, detected through `avandar_tui_term::colors`, with `NO_COLOR` honored in the TUI |

### Out of scope

- The `layout`, `command`, `pick`, `tree` and `input` families (M10 to M12).
- The `mouse` feature: no widget state records areas yet (M10).
- A ninth crate (architecture question 1, checked in M12).

### Depends on

M3, for background detection in the adopting apps.

### Exit criteria

- Layers 1 and 2. Every portable-text rule has a probe sample, and each
  sample's measured width matches the rule.
- Feature checks: no features, every feature.
- The release checklist gains whether each palette reads well, and a person
  has judged them in a fresh iTerm2 (light), in Orca (dark), and with the
  unknown palette on both.
- huckleberry-cli and dif adoptions merged, with their tests passing.
- Docs: the spec's first parts, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Architecture question 2: an off switch for the portable-text rewrite | Here: no switch in the first release. Revisited if an adoption in M4 or M9 shows a need. |
| Architecture question 1: a ninth crate | The isolation test lands here. The decision is M12's. |
| The `Selected` role needs the background's own color | `colors` returns it. Reversed video when it is unknown. |
| A glyph the specs planned fails M0's width check | It is replaced in the spec before the code |

**Size: medium.** Five pure modules tested in layers 1 and 2, and two
adoptions.

## Milestone 5: `avandar-tui-charts`

**Why here.** It is huckleberry-cli's second part, and it needs only
`style`.

**Goal.** Build `avandar-tui-charts` and have huckleberry-cli draw its
dashboards with it.

### Scope

- `docs/specs/avandar-tui-charts.md`, then the crate's `bar`, `sparkline`,
  `stripes`, `columns`, `plot`, `table` and `format` modules.
- The dashboard panel and version modal recipes in the crate's docs, as the
  architecture spec decides.
- The umbrella's `charts` feature.

Inventory elements, all huckleberry-cli's: eighth-block bars, horizontal bars
over time, the sparkline from zero, timeline stripes, daily vertical bars
with an average line, the line plot with its staggered axis and
interpolation, the browsable data table with grouped headers, the receipt
table with its stacked fallback, and the format helpers.

Adoption: huckleberry-cli replaces the generic parts of
`src/render/format.rs`, `src/render/stripes.rs`, `src/render/trends.rs`,
`src/render/summary/chart.rs`, `src/render/summary/comparison/`, the table in
`src/render/output.rs`, and the navigation and grouped headers of
`src/render/summary/`. Its metric catalog and domain wording stay its own.

### Out of scope

The dashboard panel recipes as code, and the app's catalog of metrics.

### Depends on

M4.

### Exit criteria

- Layers 1 and 2: every chart asserted on ratatui's `TestBackend`, and the
  glyph check.
- Feature checks: no features, every feature.
- huckleberry-cli's adoption merged, with its tests passing.
- Docs: the spec, `architecture.md`, the recipes.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Series told apart only by color | The spec keeps huckleberry-cli's patterns and markers, since color never carries meaning alone |
| The data table is about 1,200 lines in the app with its catalog mixed in | The spec separates the generic navigation and headers from the catalog before the code |

**Size: medium.** Seven modules of pure rendering in layers 1 and 2, and one
adoption.

## Milestone 6: `avandar-tui-tasks`

**Why here.** It is huckleberry-cli's third part and holds dif's watchers. It
depends on no other crate, so it can also run beside M3 to M5.

**Goal.** Build `avandar-tui-tasks` and have huckleberry-cli and dif run
their background work through it.

### Scope

- `docs/specs/avandar-tui-tasks.md`, then the crate's `task`, `last_good`,
  `watch` (with the take-once slot), `debounce`, `jobs`, `input_queue`, and
  `process` behind the feature of that name (`process-wrap`).
- The `tokio` feature: async forms of `task`, `watch` and `process`.
- The detached-process pattern documented beside `process`.
- The umbrella's `tasks`, `process` and `tokio` features.

Inventory elements: huckleberry-cli's single cancellable background task,
keep-last-good data holder, and input queue with last-frame retention. dif's
latest-value watcher thread, the take-once slot of its control endpoint, and
debounced change alert. brain's debouncer, job coordinator with worker lanes
and optimistic updates, cancel-on-drop subprocess worker, and command probe.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| huckleberry-cli | `src/tui/version_task.rs`, `src/tui/facts.rs` and `src/tui/data.rs`, the queue in `src/tui/job.rs` | `task` with `tokio`, `last_good`, `input_queue` |
| dif | `src/git_watcher.rs`, `src/difit/poller.rs`, the slot in `src/tui/control.rs`, `src/tui/change_alert.rs` | `watch` and `debounce`, with the time passed in |

### Out of scope

- brain's use of `jobs` and `process` (M13).
- A command running inside a panel, and the local control endpoint, which the
  architecture spec leaves out.

### Depends on

M2.

### Exit criteria

- Layers 1, 2 and 7: real threads and real children in `cargo test` on all
  three platforms, and no child outlives its handle (a process group on Unix,
  a job object on Windows).
- `cargo tree -p avandar-tui-tasks` shows no ratatui.
- Feature checks: no features, `process`, `tokio`, every feature.
- huckleberry-cli and dif adoptions merged, with their tests passing.
- Docs: the spec, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| `jobs` and `process` have no adopter until brain | Their tests carry them until M13, and a breaking change then is a minor bump |
| The `tokio` forms must not leak a runtime choice into the thread forms | The spec keeps them in separate items behind the feature |
| Job objects on the hosted Windows runner | Proved by layer 7 |

**Size: medium.** Seven modules on threads and children, with layer 7 for
the process tree, and two adoptions.

## Milestone 7: `avandar-tui-pty`

**Why here.** It is the base of the agent crate and brain's second part. dif
adopts it now, and brain once it is on ratatui 0.30.

**Goal.** Build `avandar-tui-pty` and move dif's embedded panes onto it.

### Scope

- `docs/specs/avandar-tui-pty.md`, then the crate's `pane` (spawn, reader,
  writer and waiter threads, a writer paced by each write's settle delay,
  resize that does nothing when unchanged, scrollback, the app's own lines
  written into the screen, respawn into the same screen, kill on drop),
  `env`, `keys`, `paste`, `screen` (the trait that keeps `vt100` out of other
  crates' signatures), `render` (the screen drawn cell by cell with the
  portable-text rewrite, the cursor, the exited-child placeholder) and
  `supervise`.
- The pane reports when output arrived, so an app can clear a selection when
  it chooses.
- The `mouse` feature: a `ratatui-mouse` surface for a pane, and forwarding
  the bytes the mouse crate returns.
- `pane_lab`, with `echo_input` as its child.
- `tui-term` 0.3, `portable-pty` 0.9 and `nix` (Unix only) in the root
  manifest. The umbrella's `pty` feature and its `mouse` with `pty` glue.

Inventory elements: brain's embedded terminal pane, environment allowlist,
key events to bytes, crash-on-arrival detector, pane supervision, the
terminal part of its agent pane renderer, and the lesson about `vt100` in a
public trait. dif's embedded terminal pane, renderer, exited-child
placeholder, activity log written into a pane, restart in place, and key
events to bytes. New: forwarding a paste.

Adoption: dif replaces `src/pty_pane.rs` with `pane`, the terminal part of
`src/tui/draw.rs` and the placeholder with `render`, the encoder in
`src/tui/keymap.rs` with `keys` (which adds function keys, modified arrows,
Insert and application cursor mode), and the restart in
`src/tui/app/review_surface.rs` with respawn. dif turns the guard's bracketed
paste step on and forwards pastes with `paste`, which fixes a multi-line
paste submitting at each newline. Its selection in the agent pane moves to
the pane's own surface.

### Out of scope

- Agent launching and prompts (M8).
- brain's adoption (M13).

### Depends on

M3 (dif's paste step) and M4 (`style`). M1 for the `mouse` feature.

### Exit criteria

- Layers 1 to 4, 6 and 7, as the architecture spec's testing table says:
  keys, pastes and forwarded mouse events typed at the outer terminal arrive
  in `echo_input` as the expected bytes, a resize reaches the child, and
  killing the pane leaves no process behind on any platform.
- The platform rows for this crate are proved: a Unix pseudo-terminal and
  ConPTY, the process group signalled and then killed, the pseudo-console
  closed, the default environment allowlists, and a program found through
  `PATH` and `PATHEXT` with a `.cmd` run through `cmd.exe`.
- Feature checks: no features, `mouse`, every feature.
- dif's adoption merged, with its tests passing.
- Docs: the spec, `architecture.md`, `terminals.md` for anything measured.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Mouse question 6: clearing a pane's selection when it prints | Here: the pane reports output arrival, and the app decides |
| Architecture question 4 (`.cmd` shims), for a pane | Proved here with a stub on the runner |
| `tui-term` 0.3 holds `vt100` at 0.16 | The workspace keeps one `vt100` version, and a `tui-term` upgrade waits for one that matches |
| Late output from a replaced child can interleave (dif's lesson) | The spec's respawn rule, with a layer 1 test |
| brain swallows reader and writer errors today | The spec gives them a typed error |

**Size: large.** Threads, a pseudo-terminal on three platforms, every layer
but Orca, and an adoption.

## Milestone 8: `avandar-tui-agent`

**Why here.** Its pane transport needs M7. It moves dif's prompt typing,
launch, resume and message queue, which fixes prompts that interleave before
their submits.

**Goal.** Build `avandar-tui-agent` and move dif's agent frontend onto it.

### Scope

- `docs/specs/avandar-tui-agent.md`, final only after the `.cmd` proof, then
  the crate's `input`, `adapter`, `adapters` (Claude Code, Codex, OpenCode
  and pi), `launch`, `quote` (POSIX and Windows), `resume`, `transport`,
  `controller`, `registry` and `queue`.
- Features: `pty` (the transport over `avandar-tui-pty`),
  `resume-evidence` (`serde_json`) and `uuid`.
- A stub agent that records what it was sent and when, driven through
  `pane_lab`.
- The umbrella's `agent`, `resume-evidence` and `uuid` features, and its
  `agent` with `pty` glue.

Inventory elements: brain's input sequences, agent adapter trait, per-agent
input keys, launch command builders, launch spec, command parser, quoting,
resume evidence, transport trait, controller, agent pane key forwarding, and
registry. dif's prompt typing (replaced by brain's), agent kinds, launch
command line, quoting, session resume, and message queue.

Adoption: dif replaces the prompt typing in `src/tui/keymap.rs` with
`input` (a bracketed paste with control characters stripped, and the submit
key as its own later write), `src/session.rs` and
`src/tui/startup/llm_command.rs` with `launch`, `quote` and `resume` through
the Claude Code and Codex adapters, and `src/inject/` with `queue`. The
comment snapshot type and the prompt wording stay dif's.

### Out of scope

- Single-prompt tabs, lifecycle hooks and the session store, which the
  architecture spec leaves out.
- brain's adoption (M13).

### Depends on

M7.

### Exit criteria

- Layers 1 and 2, and layers 3 and 7 through `pane_lab`: a prompt arrives as
  one paste and the submit key as a later, separate write. No real agent runs
  in CI.
- On the Windows runner, a stub agent installed as a `.cmd` shim launches
  through `cmd.exe /D /S /C` with every argument intact.
- A test shows that no reference adapter's default launch skips the agent's
  permission prompts.
- Feature checks: no features, `pty`, `resume-evidence`, `uuid`, every
  feature.
- dif's adoption merged, with its tests passing.
- Docs: the spec, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Architecture question 4 (`.cmd` shims) | Settled here: the spec is final once the stub proof passes |
| Agents change their flags | Each reference adapter follows the installed command's `--help`, and the spec says so |
| Resume evidence breaks when an agent changes its storage format | Behind its feature. Missing evidence starts fresh. |
| Codex cannot take a caller-chosen session id | The adapter reports that a fresh Codex session cannot resume |
| The registry and the OpenCode and pi adapters have no adopter until brain | Their tests carry them until M13 |

**Size: large.** Ten modules, three features, a Windows launch path, and an
adoption that touches dif's startup.

## Milestone 9: `avandar-tui-view`

**Why here.** It needs only `style`, it fixes dif's character-count widths in
markdown and its macOS-only opener, and it is medium, so it lands before the
larger widget families.

**Goal.** Build `avandar-tui-view` and move dif's markdown views, document
cursor and opener onto it.

### Scope

- `docs/specs/avandar-tui-view.md`, then the crate's `markdown` (behind the
  feature, `pulldown-cmark` 0.13), `document`, `log`, `target` and `open`.
- The `open` crate in the root manifest. The architecture spec names no
  version, so the current one is chosen when it is added.
- The umbrella's `view` and `markdown` features.

Inventory elements: dif's markdown to ratatui text, box-drawn table, style
stack, document view with a cursor, opening what the cursor is on, opener,
and activity log when it is not a pane. brain's log viewer, live log-tail
modal, bounded log-tail export, links in text, and OS actions.
huckleberry-cli's pager.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| dif | `src/tui/markdown/` | `markdown`, measured in cells |
| dif | `src/tui/guide.rs` | `document`, its cursor column in cells |
| dif | `src/tui/open_target.rs` | `target` and `open`, which open on all three platforms |
| huckleberry-cli | The pager in `src/interactive/mod.rs` and `src/prompt/host.rs` | `document` with no cursor |

dif's own selectable surface for its markdown views, from M1, keeps working
over the document's rendered lines.

### Out of scope

brain's small markdown renderer, which dif's covers. brain's adoption (M13).

### Depends on

M4.

### Exit criteria

- Layers 1 and 2: rendering asserted on `TestBackend` with CJK and emoji, the
  glyph check, and each opener's command line asserted per platform as a pure
  function, with nothing launched.
- Feature checks: no features, `markdown`, every feature.
- dif and huckleberry-cli adoptions merged, with their tests passing.
- Docs: the spec, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| dif renders markdown every frame | The spec caches a rendering per width |
| dif's document view polls its file | Reading files stays the app's. The view takes text. |
| The system handler differs per platform from the `open` command dif calls today | Each platform's command line is asserted as a pure function, as the architecture spec's test row for this crate says |

**Size: medium.** Five modules, pure apart from the opener's thin shell,
tested in layers 1 and 2, with two adoptions.

## Milestone 10: Widgets: layout and command

**Why here.** The palette and the shortcuts are the parts most apps share
(dif, brain and quickstarter's template), and the picker and input families
build on this milestone's layout and line editor.

**Goal.** Add the `layout` and `command` families to `avandar-tui-widgets`
and move dif's palette, shortcuts and pane layout, and huckleberry-cli's
shell frame and key map, onto them.

### Scope

- The spec's `layout` and `command` sections, and `input::line`.
- `layout::rect`, `layout::split`, `layout::tabs`, `layout::modal`,
  `layout::frame` and `layout::status`.
- `command::catalog`, `command::palette`, `command::shortcuts` (with the
  parity checks an app runs in its own tests), `command::motion` and
  `command::keys`.
- `input::line`, the single-line editor shared by fields, palettes and
  pickers. The input family is otherwise M12's, but the palette is this
  editor's first user.
- The `mouse` feature: widget states that record areas register them in a
  `ratatui_mouse::HitAreas<T>`.
- `widget_lab`, for key decoding in layers 3 to 7.

Inventory elements: the centered rect of dif, brain and quickstarter, and
huckleberry-cli's modal helpers. dif's two-pane shell, view switcher, tab
strip and statusline notice. brain's split with a swappable side, tabs with
stable ids, numbered tab strip, view cycle, sub-view strip, background tabs
rule, single-owner modal host, frame helpers, three-tier status line and
error banner. huckleberry-cli's shell frame, content-sized drawer, tabbed
dashboard and modal precedence. The command vocabulary of quickstarter and
brain's catalog and target gating. brain's generic palette and renderer,
quickstarter's word filter, and dif's palette (replaced). The shortcuts
tables and parity guards of brain and quickstarter, brain's footer and help
modal, and dif's shortcuts, help modal and chips. huckleberry-cli's one-handed
key map, dif's vim grammar and brain's count prefix. brain's event
classification, key classifiers, Option-glyph decoders and layered dispatch,
dif's layered key capture, huckleberry-cli's input router and quickstarter's
key classifier. brain's query editing.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| dif | `src/tui/palette.rs`, `src/tui/draw_palette.rs` | `CommandPalette<A>` with the word filter |
| dif | `src/tui/shortcuts.rs`, `src/tui/draw_help.rs` | `shortcuts`, with the parity checks in dif's tests |
| dif | `src/tui/vim.rs`, the capture order in `src/tui/event_loop.rs` | `motion` and `keys` |
| dif | The two-pane split, `src/tui/main_diff_view.rs`, the statusline notice, `src/tui/modal_layout.rs` | `split`, `tabs` (clickable through `mouse`), `status`, `rect` |
| huckleberry-cli | The frame in `src/tui/draw/mod.rs`, the drawer in `src/render/now/layout.rs` | `frame` |
| huckleberry-cli | `src/dashboard/state.rs` and its drawing, the modal helpers | `tabs` and `rect` |
| huckleberry-cli | `src/tui/keys.rs`, the precedence in `src/tui/input.rs` | `motion` with its table as configuration, `modal` and `keys` |

### Out of scope

- Pickers, trees and the rest of input (M11, M12).
- Context-aware row wording, which stays the app's.
- quickstarter's template (later, outside this plan).

### Depends on

M4. M1 for the `mouse` feature.

### Exit criteria

- Layers 1 and 2 for every module, and layers 3 to 7 through `widget_lab`:
  chords, Option glyphs and Alt keys decode to the same motions in every
  configuration, without the kitty protocol.
- Feature checks: no features, `mouse`, every feature.
- dif's tests run the parity checks and pass. dif and huckleberry-cli
  adoptions merged.
- Docs: the spec's sections, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Option glyphs depend on the keyboard layout | The spec decodes the US layout, as brain does, and an unbound glyph types as text |
| dif's users see the palette filter change from substring to words | Noted in dif's adoption change |
| brain's catalog, target gating and modal host have no adopter until brain | Their tests carry them until M13 |

**Size: large.** Twelve modules across two families, all four keyboard
layers through `widget_lab`, and two adoptions.

## Milestone 11: Widgets: pick and tree

**Why here.** It needs M10's line editor and layout, it is the second half
of huckleberry-cli's shell, and it holds the pickers and tree brain waits
for.

**Goal.** Add the `pick` and `tree` families to `avandar-tui-widgets` and
move huckleberry-cli's menus, selection prompt and listing onto them.

### Scope

- The spec's `pick` and `tree` sections.
- `pick::menu` (submenus from clap behind `clap`), `pick::select`,
  `pick::listing`, `pick::list`, `pick::fuzzy` (behind `fuzzy`,
  `nucleo-matcher` 0.3), `pick::filter` and `pick::quick`.
- `tree` (behind `tree`, `tui-tree-widget` 0.24) and the directory walker
  (behind `tree-walk`, `walkdir` 2).
- Hit-area registration for picker rows under `mouse`.
- The umbrella's `fuzzy`, `tree`, `tree-walk` and `clap` features.

Inventory elements: huckleberry-cli's menu stack with drill-down and
breadcrumb, menu tree bound to clap, menu renderer, numbered scrolling
selection prompt, and searchable, grouped, column-aligned list view.
quickstarter's arrow-key prompts (replaced). brain's fuzzy picker,
separator-insensitive matching, grouped results with headers, match line and
picker chrome, inline filter mode, variable-height list, quick-pick, picker
with disabled rows, toggle checklist, and the seven tree elements.

Adoption: huckleberry-cli replaces the menu stack in `src/tui/state.rs`,
`src/tui/draw/menu.rs`, `src/interactive/catalog.rs` (keeping its coverage
walk as a test), `src/prompt/select/` and `src/listing/`.

### Out of scope

- brain's second fuzzy matcher, left out by decision.
- brain's adoption of the pickers and the tree (M13).

### Depends on

M10.

### Exit criteria

- Layers 1 and 2. Tree tests draw once before they navigate, since the tree
  widget reads its last render.
- Feature checks: no features, then `fuzzy`, `tree`, `tree-walk` and `clap`
  each alone, then every feature.
- huckleberry-cli's adoption merged, with its tests passing.
- Docs: the spec's sections, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Whether `pick::listing` keeps a plain-text form for output that is not a terminal, as huckleberry-cli's does | The spec's `pick` section. "Prompts are native widgets" with a plain-text fallback in `decisions.md` points to yes. |
| The fuzzy picker, the tree, quick-pick and the variable-height list have no adopter until brain | Their tests carry them until M13 |
| clap's major version must match the app's | huckleberry-cli is on clap 4, as the workspace is |

**Size: large.** Eight modules, four features, and an adoption of
huckleberry-cli's whole navigation.

## Milestone 12: Widgets: input

**Why here.** The form engine uses M11's listing as a form view, and this
milestone completes the widgets crate, which is when the ninth-crate question
can be answered.

**Goal.** Add the rest of the `input` family to `avandar-tui-widgets` and
move huckleberry-cli's prompts, dials and forms, and dif's confirm dialog,
onto it.

### Scope

- The spec's `input` section.
- `input::text` (with a masked mode), `input::compose`, `input::confirm`,
  `input::choice`, `input::dial` (the time dial behind `jiff`, `jiff` 1),
  `input::form` and `input::question`.
- The hub form and prerequisite gates documented as patterns with the form
  engine, as the architecture spec decides.
- The umbrella's `jiff` feature.

Inventory elements: huckleberry-cli's text input, secret input, question
model, typed cancel and home outcomes, dial picker, time dial, amount dial,
form engine, form control adapters, prepared command, and result hold and
retry. brain's single-line input, prefilled input with a validator,
multi-line compose, both confirms, choice dialog and captive wizard (as a
form). dif's confirm modal. quickstarter's question model.

Adoptions:

| App | Replaces | With |
| --- | --- | --- |
| huckleberry-cli | `src/prompt/text.rs`, `src/prompt/secret.rs`, its question model and error markers | `text`, `question` and outcome enums |
| huckleberry-cli | The dial core and `src/prompt/dial/volume.rs` | `dial` |
| huckleberry-cli | `src/prompt/flow/`, `src/commands/prepared/`, `src/tui/forms.rs` | `form` |
| dif | The confirm modal in `src/tui/start_review/` | `confirm`, its arrow keys in the order of its buttons |

huckleberry-cli's time dial moves only after its own move to `jiff` 1 (see
[Work in other repositories](#work-in-other-repositories)). That is not part
of this milestone's exit.

### Out of scope

The dormant option editors generated from clap, the edit-a-copy settings
menu and Ctrl-C interruption, which the architecture spec leaves out.

### Depends on

M11.

### Exit criteria

- Layers 1 and 2. A paste into a text field on Windows behaves as M3
  documented.
- Feature checks: no features, `jiff`, every feature.
- The ninth-crate check: the clean build time of an app that turns on only
  `charts` is measured with the complete widgets crate, and whether
  `avandar-tui-widgets` with no features has gained a dependency is checked.
  The answer is recorded in `decisions.md`.
- huckleberry-cli and dif adoptions merged, with their tests passing.
- Docs: the spec's section, `architecture.md`.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| Architecture question 1: a ninth crate | Here, by the check above. If a trigger fires, `style` becomes `avandar-tui-style`, re-exported at the same path. |
| The form engine and its adapters are about 1,300 lines in the app | The spec splits the engine from the adapters, and the adapters by control |
| brain's captive wizard as a form has no adopter until brain | Its tests carry it until M13 |

**Size: large.** Seven modules, the largest being the form engine, and two
adoptions.

## Milestone 13: brain adopts the toolkit

**Why here.** brain is the largest source app, and its adoption waits on its
own move to ratatui 0.30. Its first two steps, the guard and the pane, can
start as soon as that move is done and M3 and M7 are released, without
waiting for M8 to M12.

**Goal.** Move brain onto the toolkit, starting with the terminal guard and
the pane.

### Scope

In brain's repository, in this order: the guard with the mouse step (which
fixes its iTerm2 wheel bug), the panic hook and drawing on the controlling
terminal; the pane with its environment, keys, paste, renderer and
supervision; the agent crate with all four reference adapters, the
controller over the pane, the registry and resume evidence; the theme and
Oklab; layout, the palette, the catalog, shortcuts, help, motions and key
decoding; the fuzzy picker, filter, quick-pick, variable-height list and the
tree; the input family and the captive wizard as a form; the log viewer,
links and opener; and the debouncer, the job coordinator and the process
worker.

In this repository: only the changes brain's adoption shows are needed, each
with its tests, in the crate that owns it.

### Out of scope

The elements of brain the architecture spec leaves out: single-prompt tabs,
lifecycle hooks, the session store, the second fuzzy matcher, the small
markdown renderer, context-aware row wording, moving files to the trash, and
the command-line theme.

### Depends on

brain's move to ratatui 0.30 (see
[Work in other repositories](#work-in-other-repositories)), M3 and M7 to
start, and M8 to M12 to finish.

### Exit criteria

- brain depends on the toolkit from crates.io at one version through the
  umbrella, and every step above is merged in brain.
- brain's local copies and the tests the toolkit now owns are deleted. Its
  own tests pass, and its source-parsing architecture guards are updated to
  the new boundaries and pass.
- The bugs brain's inventory page lists are gone: the iTerm2 wheel, the
  missing panic hook, the unbracketed paste, the help modal's scroll limit,
  the log modal's stuck `k`, footer hints naming the wrong keys, and width
  math.
- Every toolkit change it needed is released and tested in the layers it
  touches.
- Docs: `architecture.md` and the affected specs, for whatever changed.

### Risks and open questions

| Item | Settled where |
| --- | --- |
| The parts built with no adopter (`jobs`, `process`, the registry, the OpenCode and pi adapters, the catalog, the modal host, the fuzzy picker, the tree, the captive wizard) meet their first app here | A breaking change they need is a minor bump for every crate, as the versioning rules say |
| brain needs ratatui's `unstable-rendered-line-info` feature | brain keeps it. No toolkit crate requires it. |
| brain draws on the controlling terminal | `term::screen`, proved on all three platforms in M3 |

**Size: large.** The largest source app, adopting from seven crates.

## Releases

`decisions.md` ("Git tags first, crates.io later") says apps depend on a git
tag until the first building block has a real user, and that publishing to
crates.io starts then. dif using `ratatui-mouse` at the end of M1 is that
user, so **M2 moves releases from git tags to crates.io**. The git-tag phase
is one release long.

| Milestone | Release | Done by |
| --- | --- | --- |
| M1 | Tag `v0.1.0` on `main`, covering every crate, with `CHANGELOG.md` kept by hand. dif depends on the tag. | A person, or an agent with their explicit say-so |
| M2 | The first publish to crates.io, through release-plz. dif moves to the published version. | A person adds the registry token and merges the release pull request |
| M3 to M13 | A release-plz release at the end of each milestone, before its adoption | A person merges the release pull request |

The rules do not change: one version for every crate, a breaking change in
any crate bumps the minor version of all of them before 1.0, a new ratatui or
crossterm minor version is a breaking change, and the Rust floor rises only
in a release that is breaking anyway.

## Open questions and where each is settled

| Question | Measured in | Settled in | Recorded in |
| --- | --- | --- | --- |
| Architecture 1: a ninth crate for `style` | M12 | M12 | `decisions.md` |
| Architecture 2: an off switch for the portable-text rewrite | | M4: no switch for now | The widgets spec |
| Architecture 3: `ratatui-mouse`'s version on crates.io | | M2: the workspace version | `decisions.md` |
| Architecture 4: agents installed as `.cmd` shims on Windows | M0 | M7 for panes, M8 for agents | `terminals.md`, the agent spec |
| Architecture 5: the color reply and paste through the Windows pseudo-console | M0 | M3 | `terminals.md`, the term crate's docs |
| Architecture 6: the color reply in tmux inside iTerm2 | M0 | M3 asserts it | `terminals.md` |
| Mouse 1: Alt as a second selection modifier | M0 | M1, by the maintainer | `decisions.md` |
| Mouse 2: the tracking mode the Windows pseudo-console asks for | M0 | M1 asserts it | `terminals.md` |
| Mouse 3: wheel direction in iTerm2 | M0 | M1: never flipped | The mouse crate's docs |
| Mouse 4: iTerm2's own selection modifier | M0 | M1 | The mouse crate's docs |
| Mouse 5: `ratatui-core` | | M2 | The mouse spec |
| Mouse 6: clearing a pane's selection when it prints | | M7: the pane reports output | The pty spec |

## Work in other repositories

| Repository | Work | What waits for it |
| --- | --- | --- |
| `jpsyx/brain` | Move to ratatui 0.30 and crossterm 0.29, with `tui-tree-widget` 0.24, a `tui-term` release built on ratatui 0.30, and `vt100` 0.16, keeping the `unstable-rendered-line-info` feature. This happens in brain's own repository before brain adopts any crate. | M13 |
| `jpsyx/huckleberry-cli` | Move from `jiff` 0.2 to `jiff` 1 | Its use of the time dial behind the `jiff` feature. Nothing in M12's exit. |
| `AvandarLabs/quickstarter` | Its `rust-tui` template adopts the toolkit later: the terminal guard from M3, and the palette and shortcuts from M10. When is a later decision, and this plan has no milestone for it. | Nothing here |

Each milestone's adoption is itself a change in the adopting app's
repository, and the milestone is done when that change is merged there.

## Not planned

- **Drag-to-resize for pane splits.** Wishlist. `ratatui-mouse` reports a
  drag on a divider as `Dragged`, so an app can build it, but no milestone
  adds a part for it.
- Everything in the architecture spec's "What is left out" table, for the
  reasons given there.
- The mouse spec's non-goals: hover, coordinates finer than a cell, block
  selection, scrollbars, drag and drop between targets, a clipboard in the
  mouse crate, and parsing raw input.
- Backends other than crossterm, and emulators other than `vt100`.
- Testing in the best-effort terminals, a local Windows machine, and iTerm2
  in CI.
- A width table per terminal.
- An off switch for the portable-text rewrite, until an app shows a need.
- A ninth crate, unless M12's check finds a trigger.
- Versioning crates apart, until a crate has users outside the toolkit.
- Placeholder crates published to hold names.
- A 1.0 release.
