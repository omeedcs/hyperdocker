# Hyperdocker

**A Rust-native, content-addressed container runtime that replaces Docker's layer-based rebuild model with an incremental Merkle DAG.**

[![Crates.io](https://img.shields.io/crates/v/hd-cli.svg)](https://crates.io/crates/hd-cli)
[![CI](https://github.com/omeedtehrani/hyperdocker/actions/workflows/ci.yml/badge.svg)](https://github.com/omeedtehrani/hyperdocker/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## 499x Faster Warm Rebuilds

Change one file. See the difference.

```
Docker vs Hyperdocker — single file change, warm rebuild (median of 5 runs)

Docker         ||||||||||||||||||||||||||||||||||||||||||||||||||||  503 ms
Hyperdocker    |                                                      1 ms

                0          100         200         300         400         500 ms
```

| Metric | Docker | Hyperdocker | Speedup |
|--------|--------|-------------|---------|
| Cold build | 3,443 ms | 2.7 ms | 1,275x |
| Warm rebuild (median) | 503 ms | 1.0 ms | 499x |

> Benchmark: Python Flask app (7 deps). Docker uses a typical Dockerfile (`COPY . .` before
> `pip install`). Hyperdocker tracks changes at the file level via content-addressed DAG.
> Single source file modified between rebuilds. [Run it yourself](#run-the-demo).

### How?

Docker's unit of caching is a **layer**. Change one file and every layer after it re-executes.

Hyperdocker's unit of caching is a **content-addressed chunk**. Change one file and only that file's DAG subtree is rehashed. Everything else -- dependencies, config, templates -- is untouched.

```
  Docker: change app.py                    Hyperdocker: change app.py
  ========================                 ============================

  FROM python:3.11  [cached]               Env(flask-demo)
  COPY . .          [INVALIDATED]            +-- Pkg(python:3.11)   [ok]
  RUN pip install   [RE-RUN 3.5s]            +-- Dir(.)
  CMD ["python"..   [RE-RUN]                 |   +-- app.py         [CHANGED]
                                             |   +-- config.py      [ok]
  Everything after COPY                      |   +-- requirements.. [ok]
  re-executes.                               |   +-- templates/     [ok]
                                             +-- Build(pip install) [ok]
  Total: 503ms
                                           Only app.py rehashed: 1ms
```

---

## Why Hyperdocker

Docker changed how we ship software. But its inner loop -- the edit-build-test cycle during development -- has barely improved since 2013. The fundamental problem is Docker's layer model: every `RUN`, `COPY`, and `ADD` instruction creates an opaque filesystem layer. When you change a single source file, Docker invalidates that layer and every layer after it, then re-executes all of them from scratch.

This means:

- **Changing one line of application code** triggers a full `npm install` / `pip install` / `cargo build` because the dependency layer sits above the source layer, or vice versa.
- **Layer ordering is fragile.** Reordering Dockerfile instructions to optimize caching is a dark art. Get it wrong and you rebuild everything.
- **There is no content awareness.** Docker does not know that only `src/utils.ts` changed. It sees "the COPY context changed" and invalidates everything downstream.
- **Rebuilds are sequential.** Each layer waits for the previous one. Parallelism is impossible within the Dockerfile model.

Hyperdocker takes a fundamentally different approach. Instead of layers, it uses a **content-addressed store** (CAS) backed by a **Merkle DAG** that tracks every file, package, and build step as an individually hashed node. When a file changes, hyperdocker walks the DAG bottom-up, rehashing only the nodes whose inputs actually changed. Everything else is untouched.

```
  Docker: Layer-Based Rebuild             Hyperdocker: Merkle DAG Invalidation
  ===========================             ====================================

  +---------------------+                         +-------+
  | FROM ubuntu:22.04   | <-- always cached        | Env   |
  +---------------------+                         +---+---+
  | RUN apt-get install | <-- cached if above OK      |
  +---------------------+                     +-------+-------+
  | COPY package.json . | <-- INVALIDATED!     |               |
  +---------------------+   (file changed)   Pkg(node)    Dir(src/)
  | RUN npm install     | <-- RE-RUN!           |         /       \
  +---------------------+   (layer above      (cached)  main.rs  lib.rs
  | COPY . .            |    changed)                   CHANGED   (cached)
  +---------------------+                                 |
  | RUN npm run build   | <-- RE-RUN!             Only main.rs rehashed.
  +---------------------+                         Dir(src/) rehashed.
  | CMD ["node","app"]  |                         Env root rehashed.
  +---------------------+                         Everything else: untouched.
                                                   npm install: NOT re-run.
  Result: ~60s rebuild                             Result: ~200ms update
```

The key insight is that **most changes during development touch a tiny fraction of the dependency graph.** Hyperdocker exploits this by making the unit of caching a content-addressed chunk, not an ordered layer. If the content did not change, the hash did not change, and there is nothing to rebuild.

---

## Key Features

- **Content-addressed storage** -- Every file is split into content-defined chunks (FastCDC), hashed with BLAKE3, and deduplicated at the chunk level. Identical content is stored once, regardless of filename or path.

- **Merkle DAG engine** -- Files, directories, packages, and build steps are nodes in a directed acyclic graph. Each node's identity is derived from its content and its children's hashes. Changing a leaf rehashes only the path from that leaf to the root.

- **Bottom-up invalidation** -- When a file changes on disk, hyperdocker walks the DAG upward, rebuilding only the ancestor nodes whose child hashes changed. Siblings are left untouched.

- **FUSE-projected filesystem** -- The environment filesystem is projected from the DAG via FUSE. Files are materialized lazily from the CAS on read. An in-memory overlay captures writes without modifying the immutable DAG.

- **File watching with debouncing** -- Filesystem changes are detected via `notify`, filtered against include/exclude patterns, debounced to coalesce rapid edits, and then fed into the DAG invalidation engine.

- **Service management** -- Services are defined in `hd.toml` with watch patterns, dependency ordering, and restart policies. When a watched file changes, only the affected services are restarted.

- **OCI compatibility** -- OCI/Docker images can be used as base images. Layers are unpacked into the CAS. Dockerfiles can be translated to `hd.toml` via `hd ingest`.

- **Zstd compression** -- Chunks larger than 512 bytes are transparently compressed with zstd (level 3) on disk and decompressed on read.

- **Reference-counting garbage collection** -- Unreferenced manifests and chunks are cleaned up by the GC. Active environments hold references to keep their data alive.

- **Deterministic lockfile** -- Resolved dependencies are recorded in `hd.lock` with exact versions and artifact hashes, ensuring reproducible builds across machines.

- **Single static binary** -- Ships as `hd`, a single Rust binary with no runtime dependencies beyond FUSE.

---

## How It Works

### Content-Addressed Store (CAS)

Every piece of data in hyperdocker lives in a content-addressed store. Files are split into variable-size chunks using [FastCDC](https://github.com/nlfiedler/fastcdc-rs) (4 KB min, 16 KB target, 64 KB max), and each chunk is hashed with [BLAKE3](https://github.com/BLAKE3-team/BLAKE3). The chunk hash determines its storage path (`objects/<first-2-hex>/<remaining-62-hex>`). A **manifest** records the ordered list of chunk hashes, file size, and permissions for each file.

Because chunking is content-defined (not offset-based), inserting or deleting bytes in the middle of a file shifts chunk boundaries only locally. The chunks before and after the edit remain identical and are deduplicated automatically.

### Merkle DAG

On top of the CAS sits a Merkle DAG with five node types:

| Node Type   | Identity Derived From                                    |
|-------------|----------------------------------------------------------|
| `File`      | Path + manifest hash                                     |
| `Dir`       | Path + sorted list of (child name, child hash) pairs     |
| `Package`   | Provider + name + version + artifact hash                |
| `BuildStep` | Command + input hashes + sorted environment variables    |
| `Env`       | Name + ordered list of child hashes (root of the DAG)    |

Each node's content hash is computed deterministically from its fields. A `Dir` node's hash incorporates all its children's hashes. An `Env` node (the root) incorporates everything. This means the root hash is a cryptographic summary of the entire environment state.

### Incremental Invalidation

When a file changes:

1. The file is re-chunked and re-hashed in the CAS.
2. A new `File` node is inserted into the DAG with the updated manifest hash.
3. The invalidation engine walks upward from the changed node, finding all parent nodes.
4. Each parent is rebuilt with the updated child hash, producing a new parent hash.
5. This continues until the root `Env` node is reached and rebuilt.
6. Sibling nodes that were not affected retain their original hashes and are not touched.

The result is a new DAG root that shares the vast majority of its structure with the previous root. Only the path from the changed leaf to the root is new.

### FUSE Projection

The environment filesystem is not a traditional directory tree on disk. Instead, it is a FUSE mount that resolves paths against the DAG and serves file content from the CAS on demand. An in-memory overlay captures any writes made by running processes, so the immutable DAG is never modified directly.

### Architecture Diagram

```
+---------------------------------------------------+
|                    hd (CLI)                        |
|  init | up | down | status | exec | ingest | ...  |
+--+----+----+------+--------+------+--------+------+
   |         |              |               |
   v         v              v               v
+------+ +--------+ +------------+ +-------------+
|  hd  | |   hd   | |     hd     | |     hd      |
| spec | | engine | |   sandbox  | |     oci      |
+--+---+ +--+-----+ +-----+------+ +------+------+
   |        |              |               |
   |        v              v               |
   |   +--------+   +----------+           |
   |   |   hd   |   |    hd    |           |
   |   |  mount |   |   watch  |           |
   |   +--+-----+   +----+-----+           |
   |      |              |                 |
   v      v              v                 v
 +--------------------------------------------+
 |              hd-cas (CAS)                   |
 |  ContentStore | Manifest | Chunk | GC       |
 +--------------------------------------------+
 |           On-disk storage (~/.hd/cas)       |
 |  objects/<shard>/<hash>  (zstd-compressed)  |
 |  manifests/<shard>/<hash>                   |
 |  refs/<shard>/<hash>     (ref counts)       |
 +---------------------------------------------+
```

---

## Quick Start

### Installation

```bash
# From crates.io
cargo install hd-cli

# From source
git clone https://github.com/omeedtehrani/hyperdocker.git
cd hyperdocker
cargo install --path crates/hd-cli
```

On macOS, install macFUSE first:
```bash
brew install macfuse
```

On Linux, install FUSE:
```bash
sudo apt-get install fuse3 libfuse3-dev   # Debian/Ubuntu
sudo dnf install fuse3 fuse3-devel        # Fedora
```

### <a name="run-the-demo"></a>Run the Demo

See the speed difference for yourself. Requires Docker installed.

```bash
# From the repo root:
hd demo examples/flask-demo
```

This runs a side-by-side benchmark: Docker build vs Hyperdocker build on a Flask app, with a single file change between rebuilds. You'll see:

1. **Docker cold build** -- full image build with `--no-cache`
2. **Hyperdocker cold build** -- file ingestion + DAG compilation
3. **Visual diff** -- which files changed, color-coded
4. **5-run rebuild comparison** -- median times for both after a single file edit
5. **Results table** -- the speedup ratio

Example output:
```
Phase 5: Results
----------------------------------------------------------
Metric                           Time
----------------------------------------------------------
Docker cold build                             3443 ms
Hyperdocker cold build                         2.7 ms
Docker warm rebuild (median)                   503 ms
Hyperdocker warm rebuild (median)              1.0 ms
----------------------------------------------------------

Hyperdocker is 499x faster on warm rebuilds
```

### Initialize a Project

```bash
cd your-project
hd init
```

This creates an `hd.toml` in the current directory with a starter template.

### Configure Your Environment

Edit `hd.toml` to describe your environment:

```toml
[environment]
name = "myapp"
base = "node:20-alpine"

[dependencies]
apt = ["curl", "git"]

[dependencies.npm]
file = "package.json"

[build]
steps = ["npm install", "npm run build"]
cache = ["node_modules"]

[services.web]
command = "npm run dev"
watch = ["src/**/*.ts", "src/**/*.tsx"]
port = 3000

[files]
include = ["src", "public", "package.json", "tsconfig.json"]
exclude = [".git", "node_modules/.cache", "*.log"]
```

### Start the Environment

```bash
hd up
```

This parses `hd.toml`, compiles it into a Merkle DAG, ingests your files into the CAS, and starts your services. File watching begins automatically.

### Migrate from Docker

If you have an existing Dockerfile, translate it:

```bash
hd ingest Dockerfile
```

This generates an `hd.toml` from the Dockerfile's `FROM`, `RUN`, and `CMD` instructions. Review and customize the output.

---

## Configuration Reference

The `hd.toml` file is the single source of truth for an environment. Here is a complete reference for every section.

### `[environment]` (required)

| Key    | Type   | Description                                        |
|--------|--------|----------------------------------------------------|
| `name` | string | Name of the environment. Used as the DAG root name.|
| `base` | string | Base OCI image reference (e.g., `ubuntu:22.04`).   |

```toml
[environment]
name = "myapp"
base = "ubuntu:22.04"
```

### `[dependencies]`

Declares system and language-level dependencies. Keys are provider names; values vary by format.

**Package list** -- install specific packages from a provider:
```toml
[dependencies]
apt = ["curl", "git", "build-essential"]
```

**Version string** -- install a specific version of a runtime:
```toml
[dependencies]
node = "20.x"
python = "3.11"
```

**File reference** -- resolve dependencies from a manifest file:
```toml
[dependencies.npm]
file = "package.json"

[dependencies.pip]
file = "requirements.txt"
```

### `[build]`

| Key     | Type         | Description                                              |
|---------|--------------|----------------------------------------------------------|
| `steps` | list[string] | Ordered build commands. Each becomes a `BuildStep` node. |
| `cache` | list[string] | Directories to preserve across rebuilds.                 |

```toml
[build]
steps = [
    "npm install",
    "npm run build",
]
cache = ["node_modules", "dist"]
```

Build steps are chained: each step's DAG node includes the hash of the previous step as an input. If a step's inputs have not changed, it is skipped entirely.

### `[services.<name>]`

Define long-running processes. Each service has its own configuration block.

| Key              | Type         | Default    | Description                                     |
|------------------|--------------|------------|-------------------------------------------------|
| `command`        | string       | (required) | The command to run.                             |
| `watch`          | list[string] | `[]`       | Glob patterns. Service restarts when matched files change. |
| `port`           | integer      | (none)     | Port the service listens on.                    |
| `depends_on`     | list[string] | `[]`       | Services that must start before this one.       |
| `restart_policy` | string       | `"always"` | One of `always`, `on_failure`, `never`.         |

```toml
[services.web]
command = "npm run dev"
watch = ["src/**/*.ts", "src/**/*.tsx"]
port = 3000

[services.worker]
command = "node worker.js"
watch = ["worker.js", "lib/**"]
depends_on = ["web"]
restart_policy = "on_failure"
```

Services are started in topological order (respecting `depends_on`) and stopped in reverse order.

### `[files]`

Controls which files are tracked by the file watcher and ingested into the CAS.

| Key       | Type         | Default | Description                                   |
|-----------|--------------|---------|-----------------------------------------------|
| `include` | list[string] | `[]`    | Path prefixes to watch. Empty means all.      |
| `exclude` | list[string] | `[]`    | Glob patterns to exclude. `*.log`, `.git`, etc.|

```toml
[files]
include = ["src", "public", "package.json", "tsconfig.json"]
exclude = [".git", "node_modules/.cache", "*.log"]
```

The following paths are always excluded by default: `.git`, `node_modules/.cache`, `.DS_Store`, `target`.

### `[options]`

| Key             | Type   | Default | Description                                       |
|-----------------|--------|---------|---------------------------------------------------|
| `restart_grace` | string | `"5s"`  | Grace period before force-killing a restarting service. |

```toml
[options]
restart_grace = "5s"
```

---

## CLI Reference

The `hd` binary provides all commands for managing hyperdocker environments.

### `hd init`

Create a new `hd.toml` in the current directory with a starter template.

```bash
hd init
```

Fails if `hd.toml` already exists.

### `hd up`

Parse `hd.toml`, ingest project files, compile the environment into a Merkle DAG, and track changes.

```bash
hd up
```

This command:
1. Reads and validates `hd.toml`.
2. Opens (or creates) the CAS at `~/.hd/cas`.
3. Ingests project files into the CAS (filtered by `[files]` include/exclude patterns).
4. Resolves dependencies via registered providers.
5. Compiles the spec + file tree into a DAG and prints the root hash.
6. Saves build state to `~/.hd/state.json`.
7. On subsequent runs, compares with previous state and shows file-level diffs.
8. Registers a GC reference for the new root.

Example output on rebuild after changing `app.py`:
```
Environment 'flask-demo' built in 1.2ms
DAG root: a1b2c3d4...
Files: 4 (1.8 KB)  Services: web

Changes detected since last build:
  ~ app.py

  1 changed, 0 added, 0 removed, 3 unchanged
```

### `hd down`

Stop the running environment and all its services.

```bash
hd down
```

### `hd status`

Show the current environment configuration and service states.

```bash
hd status
```

Output includes the environment name, base image, and each service's command and watch patterns.

### `hd exec <command> [args...]`

Run a command inside the environment context.

```bash
hd exec npm test
hd exec python -m pytest
hd exec sh -c "echo hello"
```

### `hd ingest <dockerfile-path>`

Translate a Dockerfile into an `hd.toml`.

```bash
hd ingest Dockerfile
```

Parses `FROM`, `RUN`, and `CMD` instructions. `COPY`, `WORKDIR`, and `ENV` are noted but may require manual adjustment in the generated `hd.toml`.

### `hd lock`

Resolve all dependencies and write `hd.lock`.

```bash
hd lock
```

The lockfile records each dependency's provider, name, exact version, and artifact hash. It is sorted deterministically so that identical dependency sets always produce identical lockfiles.

### `hd demo [path]`

Run a side-by-side benchmark comparing Docker and Hyperdocker rebuild times.

```bash
# Use the bundled Flask demo project
hd demo examples/flask-demo

# Use your own project (must have Dockerfile + hd.toml)
hd demo /path/to/your/project
```

Requires Docker installed. Runs 5 iterations per benchmark and reports median times. See [Run the Demo](#run-the-demo) for details.

### `hd dag show`

Print the Merkle DAG tree for the current environment with colored output.

```bash
hd dag show
```

Example output:
```
DAG root: a1b2c3d4e5f6...
[ok]      Env(flask-demo)  a1b2c3d4e5f6
  [ok]      Pkg(oci/python:3.11-slim latest)  d4e5f6a1b2c3
  [ok]      Dir(.)  f6a1b2c3d4e5
    [ok]      app.py  b2c3d4e5f6a1  manifest:e5f6a1b2c3d4
    [ok]      config.py  c3d4e5f6a1b2  manifest:f6a1b2c3d4e5
  [ok]      Build(pip install --no-cache-dir...)  a1b2c3d4e5f6
```

Nodes are color-coded: green for new, yellow for changed, red for removed, dim for unchanged.

### `hd cas stats`

Show storage statistics for the content-addressed store.

```bash
hd cas stats
```

Output:
```
CAS Statistics:
  Chunks: 1,247
  Manifests: 83
```

### `hd cas gc`

Run garbage collection on the CAS. Removes unreferenced manifests and chunks.

```bash
hd cas gc
```

Output:
```
Garbage collection complete:
  Manifests removed: 12
  Chunks removed: 94
```

---

## Architecture

Hyperdocker is organized as a Cargo workspace with eight crates. Each crate has a single responsibility and well-defined boundaries.

### Crate Dependency Graph

```
                        hd-cli
                       /  |   \     \
                      /   |    \     \
                     v    v     v     v
                hd-spec  hd-mount  hd-sandbox  hd-oci
                 / |       / |        |           |  \
                /  |      /  |        |           |   \
               v   v     v   v        v           v    v
          hd-engine    hd-engine   hd-spec     hd-cas  hd-spec
              |           |                       |
              v           v                       |
           hd-cas      hd-cas                     |
                                                  |
                        hd-watch                  |
                       /   |   \                  |
                      v    v    v                 |
                 hd-cas hd-engine hd-spec         |
                    |                             |
                    +-----------------------------+
```

### Crate Descriptions

| Crate          | Purpose                                                                                 |
|----------------|-----------------------------------------------------------------------------------------|
| **hd-cas**     | Content-addressed store. BLAKE3 hashing, FastCDC chunking, zstd compression, manifests, sharded on-disk layout, reference-counting garbage collector. The foundation everything else builds on. |
| **hd-engine**  | Merkle DAG engine. Defines the five node types (`File`, `Dir`, `Package`, `BuildStep`, `Env`), the in-memory DAG with CAS persistence, bottom-up invalidation, DAG diffing, and file tree ingestion. |
| **hd-spec**    | Configuration layer. Parses `hd.toml` into `EnvSpec`, validates service dependency graphs (cycle detection), compiles specs into DAGs via the provider registry, and manages the `hd.lock` lockfile. |
| **hd-mount**   | Filesystem projection. `ProjectedFs` resolves paths against the DAG and serves content from the CAS. `Overlay` captures writes. `FuseFs` bridges to the FUSE kernel interface. `MountManager` tracks mount lifecycle. |
| **hd-watch**   | File watching. Uses `notify` with configurable poll intervals, `PathFilter` for include/exclude rules (with hardcoded defaults for `.git`, `target`, etc.), `Debouncer` for coalescing rapid changes, and `PathMap` for bidirectional path-to-hash lookups. |
| **hd-sandbox** | Process management. `ManagedProcess` wraps `std::process::Child` with lifecycle control. `Service` adds watch-pattern matching and restart policies. `Sandbox` orchestrates multiple services with topological ordering. |
| **hd-oci**     | OCI/Docker interop. Parses image references (Docker Hub, GHCR, private registries), unpacks tar/tar+gzip layers into the CAS, and translates Dockerfiles into `EnvSpec`. |
| **hd-cli**     | The `hd` binary. Clap-based CLI with subcommands for `init`, `up`, `down`, `status`, `exec`, `ingest`, `lock`, `dag show`, `cas stats`, and `cas gc`. |

### Key Dependencies

| Dependency | Version | Role                                              |
|------------|---------|---------------------------------------------------|
| blake3     | 1.6     | Cryptographic hashing (256-bit, ~3x faster than SHA-256) |
| fastcdc    | 3.1     | Content-defined chunking for deduplication         |
| zstd       | 0.13    | Transparent chunk compression                      |
| fuser      | 0.15    | FUSE filesystem in userspace                       |
| notify     | 7.0     | Cross-platform filesystem event watching           |
| clap       | 4.5     | CLI argument parsing with derive macros            |
| serde      | 1.0     | Serialization for DAG nodes, manifests, specs      |
| bincode    | 2.0     | Compact binary serialization for CAS objects       |
| rayon      | 1.10    | Data parallelism for chunking and hashing          |
| nix        | 0.29    | Unix signal handling for process management        |
| reqwest    | 0.12    | HTTP client for OCI registry communication         |
| tar/flate2 | 0.4/1.0 | OCI layer unpacking                                |

---

## Performance

### Measured Results

Benchmarked on a Flask app with 7 Python dependencies (`hd demo examples/flask-demo`):

```
Cold build (first time, no cache):
  Docker         ==============================  3,443 ms
  Hyperdocker    =                                 2.7 ms

Warm rebuild (single file change, median of 5):
  Docker         ==============================    503 ms
  Hyperdocker    =                                 1.0 ms
```

Why the difference? Docker re-executes `pip install` (all 7 packages) on every source file change because the `COPY . .` layer precedes the install step. Hyperdocker sees that only `app.py` changed, re-hashes that single file, and leaves everything else untouched.

Run `hd demo examples/flask-demo` to reproduce these numbers on your machine.

### Design Principles

Hyperdocker is designed around four principles that make it fast where Docker is slow.

### 1. Content-Defined Chunking

Files are split into variable-size chunks using FastCDC, not at fixed byte offsets. This means that inserting or removing bytes in the middle of a file only affects the chunks immediately surrounding the edit. All other chunks remain identical and are deduplicated automatically.

In practice, editing a single function in a 500 KB source file typically affects 1-2 chunks (16-32 KB). The other ~30 chunks are unchanged and already in the store.

### 2. Bottom-Up Invalidation

Docker invalidates top-down: if layer N changes, layers N+1 through the end are all re-executed. Hyperdocker invalidates bottom-up: only the ancestors of a changed node are rebuilt, and "rebuilt" means rehashing -- not re-executing commands.

Consider an environment with 500 source files, 200 npm packages, and 3 build steps. Editing one source file in Docker might trigger a full `npm install` (30-60 seconds). In hyperdocker, it triggers a rehash of the file node, its parent directory node, and the root env node -- a sub-second operation.

### 3. Lazy Materialization

The FUSE-projected filesystem does not extract the entire environment to disk. Files are served on-demand from the CAS when a process reads them. If your application only touches 50 of 500 source files during a test run, only those 50 files are ever read from the store. The rest exist as DAG nodes but are never materialized.

### 4. Cross-File and Cross-Version Deduplication

Because the CAS is content-addressed at the chunk level, deduplication happens automatically:

- Two copies of the same library in `node_modules` share chunks.
- Successive versions of a file that differ by a few lines share most chunks.
- Multiple environments using the same base image share all base-image chunks.

This means disk usage grows proportionally to unique content, not to the number of environments or versions.

---

## Comparison with Docker

| Dimension                     | Docker                                    | Hyperdocker                              |
|-------------------------------|-------------------------------------------|------------------------------------------|
| **Unit of caching**           | Ordered filesystem layer                  | Content-addressed chunk                  |
| **Invalidation direction**    | Top-down (layer N invalidates N+1..end)   | Bottom-up (only ancestors of changed node) |
| **Invalidation granularity**  | Entire layer                              | Individual file                          |
| **Deduplication scope**       | Identical layers across images            | Identical chunks across all files        |
| **Rebuild after 1-file edit** | Re-run all layers after the changed layer | Rehash one file + ancestors (~ms)        |
| **Filesystem model**          | Copy-on-write overlay (overlayfs)         | FUSE projection from DAG + overlay       |
| **File watching**             | Not built-in (requires polling or tools)  | Built-in with debouncing and filtering   |
| **Service management**        | `docker-compose` (separate tool)          | Built into `hd.toml`                     |
| **Configuration**             | Dockerfile + docker-compose.yml           | Single `hd.toml`                         |
| **Compression**               | Layer-level (gzip/zstd)                   | Chunk-level (zstd, transparent)          |
| **Garbage collection**        | `docker system prune`                     | Reference-counting GC (`hd cas gc`)      |
| **Hash algorithm**            | SHA-256                                   | BLAKE3 (3-4x faster)                     |
| **Implementation language**   | Go                                        | Rust                                     |

---

## Comparison with Alternatives

### Nix

Nix pioneered content-addressed package management and reproducible builds. Hyperdocker borrows the content-addressing concept but differs in scope and approach:

- **Nix** is a full package manager and build system with its own functional language. Hyperdocker is a container runtime -- it manages environments, not package builds.
- **Nix** hashes derivation inputs. Hyperdocker hashes file content directly (content-addressed, not input-addressed).
- **Nix** has a steep learning curve (the Nix language). Hyperdocker uses a simple TOML file.
- **Nix** does not include file watching, service management, or FUSE projection.

### Bazel

Bazel is a build system with content-addressed caching and remote execution.

- **Bazel** focuses on build artifact caching across a monorepo. Hyperdocker focuses on development environment lifecycle.
- **Bazel** requires BUILD files and a complex rule system. Hyperdocker requires a single `hd.toml`.
- **Bazel** does not manage running services or provide a container-like filesystem.
- **Bazel's** remote cache is analogous to a distributed CAS, which hyperdocker plans for v2.

### Devbox (by Jetify)

Devbox wraps Nix to provide a simpler developer experience.

- **Devbox** focuses on dependency isolation via Nix packages. Hyperdocker provides a full environment runtime with file watching and service management.
- **Devbox** does not chunk or deduplicate your source files.
- **Devbox** does not provide incremental invalidation of build steps.

### Dev Containers

Dev Containers (VS Code) use Docker under the hood and inherit all of Docker's layer-based limitations.

- **Dev Containers** are tied to VS Code and Docker. Hyperdocker is editor-agnostic and Docker-free.
- **Dev Containers** rebuild via Dockerfile. Hyperdocker rebuilds incrementally via DAG invalidation.
- **Dev Containers** do not provide built-in file watching or service management.

### OrbStack

OrbStack is a fast Docker Desktop replacement for macOS.

- **OrbStack** optimizes Docker's execution (faster VM, better I/O). Hyperdocker replaces Docker's model entirely.
- **OrbStack** still uses layers. A one-file change still invalidates downstream layers.
- **OrbStack** is macOS-only and closed source. Hyperdocker is cross-platform and MIT-licensed.

---

## Roadmap

### v1 (Current)

- [x] Content-addressed store with BLAKE3 + FastCDC + zstd
- [x] Merkle DAG with five node types
- [x] Bottom-up invalidation engine
- [x] DAG diffing
- [x] `hd.toml` spec parser with validation
- [x] Dependency provider registry (trait-based, extensible)
- [x] Deterministic lockfile (`hd.lock`)
- [x] Spec-to-DAG compiler with file tree ingestion
- [x] FUSE filesystem projection with overlay
- [x] File watcher with include/exclude filtering and debouncing
- [x] Service management with topological ordering and watch-based restart
- [x] OCI image reference parsing and layer unpacking
- [x] Dockerfile-to-`hd.toml` translation
- [x] Reference-counting garbage collection
- [x] File tree ingestion into CAS and DAG
- [x] Build state persistence and file-level change detection
- [x] Colored DAG tree renderer with diff highlighting
- [x] Benchmark harness: Docker vs Hyperdocker (`hd demo`)
- [x] CLI: `init`, `up`, `down`, `status`, `exec`, `ingest`, `lock`, `dag show`, `cas stats`, `cas gc`, `demo`

### v2 (Planned)

- **Distributed CAS** -- Share the content-addressed store across machines. Push/pull chunks to a remote store (S3, GCS, or a dedicated server). Team-wide deduplication.
- **Docker socket shim** -- Expose a Docker-compatible API so that tools expecting `docker build` and `docker run` can use hyperdocker transparently.
- **Language-aware reloaders** -- Instead of restarting a service on file change, inject the changed module at runtime (hot module replacement for Node.js, hot reload for Go/Rust).
- **Checkpoint/restore** -- Snapshot a running environment's state (processes, memory, network) and restore it instantly on another machine using CRIU.
- **Programmable API** -- Expose the DAG, CAS, and invalidation engine as a Rust library with stable API for building custom tooling on top.
- **Built-in dependency providers** -- Ship providers for apt, npm, pip, cargo, and brew out of the box, with automatic resolution and CAS ingestion.
- **Parallel build steps** -- Execute independent build steps concurrently when the DAG shows no data dependencies between them.
- **Remote execution** -- Run build steps on remote machines and pull only the output artifacts into the local CAS.
- **Layer-compatible export** -- Export a hyperdocker environment as an OCI image for deployment to Kubernetes or any container runtime.

---

## Contributing

### Prerequisites

- Rust 1.75+ (2021 edition)
- macFUSE (macOS) or FUSE3 (Linux)

### Building from Source

```bash
git clone https://github.com/omeedtehrani/hyperdocker.git
cd hyperdocker
cargo build
```

The `hd` binary is built to `target/debug/hd`.

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p hd-cas
cargo test -p hd-engine
cargo test -p hd-spec
cargo test -p hd-mount
cargo test -p hd-watch
cargo test -p hd-sandbox
cargo test -p hd-oci
cargo test -p hd-cli
```

Note: `hd-watch` tests involve filesystem polling and include short sleeps. They may be flaky on very slow CI runners. `hd-mount` FUSE tests require FUSE privileges and are skipped in environments without FUSE support.

### Project Structure

```
hyperdocker/
  Cargo.toml              # Workspace root
  Cargo.lock
  crates/
    hd-cas/               # Content-addressed store
      src/
        hash.rs           # BLAKE3 ContentHash type
        chunk.rs          # FastCDC content-defined chunking
        manifest.rs       # File manifest (chunk list + metadata)
        store.rs          # On-disk store with sharding and compression
        gc.rs             # Reference-counting garbage collector
    hd-engine/            # Merkle DAG engine
      src/
        node.rs           # Five DAG node types
        dag.rs            # In-memory DAG with CAS persistence
        invalidation.rs   # Bottom-up invalidation algorithm
        diff.rs           # DAG diffing (added/removed/changed)
    hd-spec/              # Configuration and compilation
      src/
        spec.rs           # hd.toml parser and validator
        provider.rs       # Dependency provider trait and registry
        compiler.rs       # Spec-to-DAG compiler
        lockfile.rs       # hd.lock serialization
    hd-mount/             # Filesystem projection
      src/
        projected.rs      # DAG-backed virtual filesystem
        overlay.rs        # In-memory write overlay
        fuse.rs           # FUSE filesystem adapter
        manager.rs        # Mount lifecycle management
    hd-watch/             # File watching
      src/
        watcher.rs        # notify-based recursive file watcher
        filter.rs         # Include/exclude path filtering
        debounce.rs       # Event coalescing
        pathmap.rs        # Bidirectional path <-> hash mapping
    hd-sandbox/           # Process management
      src/
        process.rs        # Managed child process wrapper
        service.rs        # Service with watch patterns and restart
        sandbox.rs        # Multi-service orchestrator
    hd-oci/               # OCI/Docker interop
      src/
        registry.rs       # Image reference parsing
        unpack.rs         # Tar/tar+gzip layer unpacking into CAS
        dockerfile.rs     # Dockerfile to hd.toml translation
    hd-cli/               # CLI binary
      src/
        main.rs           # Clap CLI definition
        render.rs         # Colored DAG tree renderer with diff highlighting
        commands/          # One module per subcommand
          init.rs
          up.rs           # File ingestion, state tracking, change detection
          down.rs
          status.rs
          exec.rs
          ingest.rs
          lock.rs
          dag.rs
          cas.rs
          demo.rs         # Benchmark harness (Docker vs Hyperdocker)
  examples/
    flask-demo/           # Bundled demo project for benchmarking
      app.py
      config.py
      requirements.txt
      templates/index.html
      Dockerfile          # Deliberately naive (COPY before pip install)
      hd.toml
```

---

## License

MIT -- see [LICENSE](LICENSE) for details.
