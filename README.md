# git-flex

A high-performance Git automation and cryptographic utility written in Rust. Designed for sub-second SHA-1 vanity commit mining, streaming bulk commit ingestion via `git fast-import`, and contribution matrix rasterization.

Includes both a terminal dashboard (TUI) built on Ratatui and a full-featured CLI.

---

## Key Features

- **Hardware-Accelerated Vanity Commit Miner**
  - Mines custom Git commit SHA-1 prefixes (`000000`, `deadbeef`, `1337c0de`) using hardware-accelerated SHA-1 instructions (ARM NEON / x86 SSE).
  - SHA-1 midstate precomputation: initial commit headers and body prefixes are digested once; worker threads only hash the dynamic nonce payload.
  - Zero-collision thread partitioning: each Rayon worker thread operates on a disjoint key space using dedicated thread-ID bitmasks.
  - Nonce encoding strategies:
    - *Invisible Whitespace*: Appends nonces as invisible spaces and tabs (`\x20`, `\x09`), preserving pristine commit messages and Git logs.
    - *Git Trailer*: Appends structured `Vanity-Nonce: <hex>` commit metadata trailers.

- **Streaming Commit Batch Pipeline (`fast-import`)**
  - Generates synthetic commit sequences directly through an IPC pipe to `git fast-import`.
  - Capable of streaming >50,000 commits per second without filesystem I/O bottleneck.
  - Preserves working tree integrity with non-destructive branch ref updates.

- **Contribution Heatmap Matrix Rasterizer**
  - Rasterizes 5x7 alphanumeric font glyphs onto the 7x52 GitHub contribution graph.
  - Dynamic centering: centers words across the 52-week calendar without truncation.
  - Timezone & date normalization: all generated commits are anchored at 12:00:00 UTC (noon), preventing midnight bleed into adjacent days or future dates.

- **Attribution & Co-Author Injector**
  - Injects standardized RFC 5064 / GitHub-recognized `Co-authored-by` metadata trailers into commit headers.
  - Built-in roster for multi-agent workflows and automated contributor attribution.

- **Minimalist Terminal Interface (TUI)**
  - Clean keyboard navigation with Ratatui and Crossterm.
  - Real-time 7x52 contribution preview grid.
  - Safe defaults: remote auto-push disabled by default (`Ctrl+P` / `F5` toggle).

---

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2021 or later, 1.74+)
- Git 2.30+ installed in `PATH`

### Build from Source

```bash
git clone https://github.com/lonestill/git-flex.git
cd git-flex
cargo build --release
```

The optimized binary will be located at `target/release/git-flex`.

To install system-wide:

```bash
cargo install --path .
```

---

## CLI Usage

### 1. Interactive Terminal Dashboard (TUI)

```bash
git-flex
# or explicitly:
git-flex tui
```

#### TUI Keybindings

| Key | Action |
| :--- | :--- |
| `Tab` / `BackTab` | Switch between tabs |
| `F1` - `F4` | Direct tab selection (1: Vanity, 2: Batch, 3: Heatmap, 4: Contributors) |
| `Enter` | Execute active tab operation |
| `Ctrl+P` / `F5` | Toggle remote auto-push (`origin/<branch>`) |
| `Ctrl+B` / `F6` | Toggle heatmap background fill (1 commit/day for Level-1 base) |
| `Ctrl+X` / `F7` | Wipe/clean heatmap canvas (reset branch to 1 clean commit) |
| `Esc` / `Ctrl+C` | Exit |

---

### 2. Vanity Hash Mining (`vanity`)

Mines a specific hex prefix for the specified commit (defaults to `HEAD`):

```bash
# Mine a 6-zero prefix on HEAD with invisible whitespace padding
git-flex vanity 000000

# Mine with explicit trailer syntax (Vanity-Nonce: <hex>)
git-flex vanity deadbeef --trailer

# Dry run (computes hash and benchmarks hashrate without writing ref)
git-flex vanity 1337c0de --dry-run

# Automatically force-push to origin upon successful match
git-flex vanity babe -p
```

#### Output Example

```text
git-flex: mining vanity commit with prefix '000000'...
  target prefix : 000000
  mined hash    : 000000c14b29f0e8d91a27e3d81b490f28e5a7b1
  base hash     : 9c8f12a3d0b28e5a7b19c8f12a3d0b28e5a7b14b
  hashrate      : 18.24 MH/s (4,194,304 attempts in 0.230s)
  ref updated   : refs/heads/master -> 000000c14b29f0e8d91a27e3d81b490f28e5a7b1
```

---

### 3. Commit Batch Streaming (`blast`)

Streams empty synthetic commits directly into Git's object database:

```bash
# Generate 1,000 commits on current branch
git-flex blast -n 1000

# Stream 5,000 commits into a designated target branch
git-flex blast -n 5000 --branch benchmark

# Custom commit message prefix
git-flex blast -n 2500 --message "perf: stress test cycle"
```

---

### 4. Contribution Heatmap Matrix (`draw`)

Draws text onto the GitHub contribution graph over the past 52 weeks with guaranteed Level-4 nuclear neon green intensity:

```bash
# Draw text centered in the current rolling 52-week window (defaults to 50 commits/pixel)
git-flex draw LZT -p

# Target a specific historical calendar year (1969 - 2026)
git-flex draw LZT --year 2025 --fill-bg -p

# Fill unpainted days with 1 commit/day (Level-1 uniform base) so pattern pops out in Level-4 neon green
git-flex draw LZT --fill-bg -p

# Reset canvas before painting (wipes branch to 1 clean commit first)
git-flex draw LZT --clean --fill-bg -p

# Custom intensity (e.g. 80 commits/pixel)
git-flex draw PRO --intensity 80 -p
```

---

### 5. Heatmap Canvas Reset (`clean`)

Resets branch history to a single clean root commit, wiping all generated pixel/flex commits while preserving all working tree files:

```bash
# Clean current branch locally
git-flex clean

# Clean and force-push to GitHub (clears previous contributions from profile)
git-flex clean -p
```

---

### 6. Contributor Inflation Engine (`contributors`)

Inflates the repository's official `/graphs/contributors` list with verified GitHub accounts (fetched dynamically via GitHub API + curated legends pool):

```bash
# Inject 25 real GitHub contributors into default branch
git-flex contributors -n 25 -p
```

---

### 7. Co-Author Attributions (`ai`)

Injects standardized `Co-authored-by` trailers onto the `HEAD` commit:

```bash
git-flex ai

# Target a specific commit
git-flex ai -c HEAD~1
```

---

## Technical Architecture

```text
               ┌────────────────────────────────────────────────────────┐
               │                        git-flex                        │
               └───────────┬────────────────────────────────┬───────────┘
                           │                                │
            [Interactive TUI Dashboard]           [CLI Subcommands]
                           │                                │
        ┌──────────────────┼────────────────────────────────┼──────────────────┐
        │                  │                                │                  │
 ┌──────┴──────┐    ┌──────┴──────┐                  ┌──────┴──────┐    ┌──────┴──────┐
 │   vanity    │    │   blaster   │                  │   painter   │    │  ai_squad   │
 └──────┬──────┘    └──────┬──────┘                  └──────┬──────┘    └──────┬──────┘
        │                  │                                │                  │
  SHA-1 Midstate     git fast-import                 5x7 Font Raster    RFC 5064 Git
  NEON / SSE         Buffered Stream                 12:00 UTC Noon      Trailers
  Rayon Workers      >50k commits/s                  Bounds Checking     Atomic Ref
```

### 1. SHA-1 Midstate Precomputation

Git stores commits as formatted objects:

```text
commit <byte_length>\0<commit_body_contents>
```

When mining nonces, modifying characters only at the end of the commit body means the prefix data up to the last 64-byte block boundary is immutable. `git-flex` computes the SHA-1 midstate across `[header + base_commit_body]` once, caching the 160-bit internal state `(A, B, C, D, E)`. Worker threads copy this midstate and only execute the compression function over the terminal block.

### 2. Zero-Collision Rayon Partitioning

Each worker thread computes its starting nonce space via dedicated bitwise offsets:
- For whitespace nonces, bits 0–15 encode the thread ID into space/tab permutations, guaranteeing disjoint search spaces across CPU cores.
- For trailer nonces, the first 4 hexadecimal characters store the thread ID.

---

## Testing

Run the automated test suite:

```bash
cargo test
```

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
