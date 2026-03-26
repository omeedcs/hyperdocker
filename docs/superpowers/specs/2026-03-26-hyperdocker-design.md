# Hyperdocker Design Spec

A Rust-native, content-addressed, file-level incremental container runtime that replaces Docker's layer-based rebuild model with a persistent, mutable Merkle DAG execution graph.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Target audience | General-purpose, broad adoption | Maximize impact |
| Platform | Linux-first, macOS dev experience | Covers ~90% of target developers |
| Base images | OCI ingestion + pure declarative | OCI is the on-ramp, declarative is the destination |
| Distribution | Local-only for v1 | Nail single-dev experience first |
| Process model | Smart restart, sandbox reuse | Rebuild elimination is the real win, not process snapshotting |
| Language reload | None — language tools own reload | Clean separation of concerns |
| CLI identity | Own identity (`hd`), Dockerfile ingestion, no Docker socket shim | Clear differentiation from Docker |
| Architecture | Layered library — 8 crates, single binary + FUSE child process | Architectural discipline without IPC overhead |

## Architecture Overview

Eight Rust crates composed into a single binary with strict API boundaries:

```
hd-cli          → user-facing commands (thin client over Unix socket)
hd-engine       → Merkle DAG computation, dependency resolution, invalidation
hd-cas          → content-addressable store (BLAKE3, CDC chunking, dedup, GC)
hd-mount        → FUSE/macFUSE filesystem projection from DAG
hd-sandbox      → namespace/process management (Linux), process groups (macOS)
hd-watch        → inotify/FSEvents → DAG node mapping, debouncing, batching
hd-oci          → OCI image ingestion, Dockerfile translation
hd-spec         → TOML environment spec parsing, dependency providers, lockfile
```

Dependency order: `hd-cas` → `hd-engine` → `hd-spec` → `hd-mount` → `hd-watch` → `hd-sandbox` → `hd-oci` → `hd-cli`.

The binary runs as a long-lived daemon. The CLI is a thin client communicating over a Unix socket. The FUSE mount runs as a supervised child process for fault isolation.

---

## 1. Content-Addressable Store (`hd-cas`)

Every file, artifact, and dependency is stored as content-addressed chunks.

### Chunking

- Content-defined chunking (CDC) using FastCDC algorithm
- Target chunk size: 16KB, min 4KB, max 64KB
- Files smaller than 4KB stored as single chunks
- CDC ensures inserting bytes at the start of a file only re-hashes affected chunk boundaries

### Hashing

BLAKE3 for all content hashing. ~4x faster than SHA-256, tree-hashable (parallelizable), 256-bit output.

### Storage Layout

```
~/.hd/cas/
  objects/        # chunk blobs, keyed by BLAKE3 hash (first 2 chars as dir shard)
    ab/cd1234...  # raw chunk data, optionally zstd-compressed
  manifests/      # file manifests: ordered list of chunk hashes + metadata
    ef/5678ab...  # { chunks: [hash1, hash2, ...], size, mode, mtime }
  refs/           # named references to manifest hashes
```

### Deduplication

Content-addressed insertion: if a chunk hash exists, skip the write. Identical `node_modules/` trees across environments share 100% of storage.

### Garbage Collection

Reference-counting on manifests. Chunks with zero references are eligible for GC. GC runs as a background sweep, never during active operations.

### API

```rust
trait ContentStore {
    fn put_file(&self, path: &Path) -> Result<ManifestHash>;
    fn get_file(&self, hash: &ManifestHash, dest: &Path) -> Result<()>;
    fn has_chunk(&self, hash: &ChunkHash) -> bool;
    fn put_chunk(&self, data: &[u8]) -> Result<ChunkHash>;
    fn get_chunk(&self, hash: &ChunkHash) -> Result<Vec<u8>>;
    fn gc(&self) -> Result<GcStats>;
}
```

---

## 2. Merkle DAG Engine (`hd-engine`)

The DAG replaces Docker's linear layer model. The entire environment is a directed acyclic graph where nodes are content-addressed artifacts and edges are dependencies.

### Node Types

```
FileNode        — a single file (points to a CAS manifest)
DirNode         — a directory (hash derived from sorted child hashes)
PackageNode     — a resolved dependency (apt, npm, cargo, pip)
BuildStepNode   — a deterministic build action (like Dockerfile RUN)
EnvNode         — root node representing a complete environment
```

Every node stores its content hash, input node hashes, and metadata.

### Deterministic Hashing

A `BuildStepNode`'s hash = `BLAKE3(command + sorted input node hashes + env vars)`. Unchanged command + unchanged inputs = unchanged hash = skip entirely.

### Incremental Invalidation

Bottom-up invalidation (opposite of Docker):

1. Watcher detects file change, recomputes file's manifest hash
2. Engine walks UP the DAG to find stale ancestor nodes
3. Only stale nodes are recomputed; unaffected subtrees untouched

### DAG Storage

The DAG is stored in the CAS. Current environment state is a single root hash (`EnvNode`). Environment history is a chain of root hashes — cheap since most of the DAG is shared between versions.

### API

```rust
trait DagEngine {
    fn build(&self, spec: &EnvSpec) -> Result<EnvNode>;
    fn invalidate(&self, changed: &[FileChange]) -> Result<EnvNode>;
    fn diff(&self, a: &EnvNode, b: &EnvNode) -> Result<DagDiff>;
    fn query(&self, root: &EnvNode, path: &str) -> Result<Node>;
    fn hash(&self, node: &Node) -> ContentHash;
}
```

### Concurrency

Independent subtrees rebuild concurrently via work-stealing executor (Tokio or Rayon). Node-level locks prevent concurrent mutation of the same subtree.

---

## 3. Filesystem Projection (`hd-mount`)

The DAG is projected directly into a mountable filesystem — no tar unpacking.

### Mechanism

- **Linux:** FUSE. Runs as a separate supervised child process.
- **macOS:** macFUSE / FUSE-T. Same model, mount scoped to project directory.

### Projection

FUSE translates filesystem syscalls into DAG lookups:

```
open("/app/src/main.rs")
  → resolve path in DirNode tree
  → find FileNode
  → fetch chunks from CAS
  → return file descriptor backed by chunk data
```

### Lazy Materialization

Files are not written to disk until read. A 10GB environment with 500MB actively used touches only 500MB of I/O.

### Write-Back

Writes go to a writable overlay:

```
Read:   DAG (immutable) → CAS chunks → data
Write:  data → overlay buffer → flush to CAS → update DAG node
```

Writes are captured, content-addressed, and produce a full audit trail.

### Performance

- FUSE overhead (~2-5us/op) is negligible for dev workflows
- Frequently accessed chunks cached in memory-mapped LRU
- Directory listings pre-computed and cached at DirNode level

### Lifecycle

```
hd up       → compute DAG → spawn FUSE process → mount at ~/.hd/mounts/<env-id>/
hd down     → flush dirty overlay → unmount FUSE → checkpoint DAG state
hd restart  → re-mount from existing DAG (sub-second)
```

### API

```rust
trait MountManager {
    fn mount(&self, env: &EnvNode, mountpoint: &Path) -> Result<MountHandle>;
    fn unmount(&self, handle: &MountHandle) -> Result<()>;
    fn flush(&self, handle: &MountHandle) -> Result<EnvNode>;
    fn is_mounted(&self, mountpoint: &Path) -> bool;
}
```

---

## 4. File Watching & DAG Invalidation (`hd-watch`)

Detects host filesystem changes and triggers incremental DAG invalidation. Target: sub-100ms propagation.

### Bidirectional Mapping

```
Host path  →  DAG node hash    (host change triggers DAG update)
DAG hash   →  Host path(s)     (DAG recomputation updates sandbox FS)
```

### Watch Mechanism

- **Linux:** inotify (recursive). Fall back to fanotify for >8K directories.
- **macOS:** FSEvents API. Naturally recursive.

### Change Pipeline

```
FS event (file modified)
  → debounce (5ms window, coalesce rapid saves)
  → compute new file hash (BLAKE3, re-hash only changed chunks)
  → compare with current DAG node hash
  → if different:
      → update FileNode in DAG
      → propagate invalidation upward
      → notify sandbox of affected paths
      → sandbox restarts affected services
```

### Smart Filtering

Uses the environment spec to determine live paths. Excluded by default: `.git/`, `node_modules/.cache/`, build artifacts, editor temp files. Configurable via `[files]` section.

### Batching

Bulk operations (git checkout, npm install) are batched within the debounce window and submitted as a single DAG invalidation pass.

### API

```rust
trait FileWatcher {
    fn watch(&self, root: &Path, spec: &EnvSpec) -> Result<WatchHandle>;
    fn unwatch(&self, handle: &WatchHandle) -> Result<()>;
    fn on_change(&self, callback: impl Fn(Vec<FileChange>) + Send) -> Result<()>;
}

struct FileChange {
    host_path: PathBuf,
    dag_node: ContentHash,
    kind: ChangeKind, // Created, Modified, Deleted, Renamed
}
```

---

## 5. Sandbox Manager (`hd-sandbox`)

Long-lived execution environments that evolve in place as the DAG changes.

### Linux Isolation

Namespaces created once and reused across reloads:

```
Mount NS    — FUSE mount as root filesystem
PID NS      — isolated process tree, persists across restarts
Network NS  — virtual network, persists (no port re-binding)
User NS     — unprivileged operation, UID mapping
IPC NS      — isolated shared memory
```

No cgroups enforcement by default. Optional resource limits configurable in spec.

### macOS Isolation

Lighter construct (no kernel namespaces):

```
Process group  — supervised process group
FUSE mount     — scoped to project directory
Network        — host networking, localhost binding
Environment    — isolated env vars, PATH, working directory
```

The macOS value comes from DAG-driven incremental rebuilds and managed service lifecycle, not kernel isolation.

### Service Management

```rust
struct Service {
    name: String,
    command: Vec<String>,
    depends_on: Vec<String>,
    watch_paths: Vec<PathBuf>,
    restart_policy: RestartPolicy, // Always, OnFailure, Never
}
```

DAG invalidation reports changed paths → match against service `watch_paths` → only affected services restart. Unaffected services keep running.

### Restart Flow

```
DAG reports changed paths
  → match against service watch_paths
  → matched: SIGTERM → grace period (5s) → SIGKILL → re-exec in existing namespace
  → unmatched: no action
```

Re-exec'd services see updated files immediately via the FUSE mount.

### Lifecycle

```
hd up       → create namespaces → mount FUSE → start services in dependency order
hd down     → stop services (reverse order) → unmount → destroy namespaces
hd restart  → stop + start affected services (namespaces persist)
hd exec     → one-off process inside existing sandbox
hd status   → services, uptime, last restart reason
```

### API

```rust
trait SandboxManager {
    fn create(&self, spec: &EnvSpec, mount: &MountHandle) -> Result<Sandbox>;
    fn destroy(&self, sandbox: &Sandbox) -> Result<()>;
    fn restart_services(&self, sandbox: &Sandbox, changed: &[PathBuf]) -> Result<()>;
    fn exec(&self, sandbox: &Sandbox, cmd: &[String]) -> Result<ExitCode>;
    fn status(&self, sandbox: &Sandbox) -> Result<SandboxStatus>;
}
```

---

## 6. Environment Spec (`hd-spec`)

Declarative TOML configuration that compiles into the DAG.

### Example `hd.toml`

```toml
[environment]
name = "myapp"
base = "ubuntu:22.04"

[dependencies]
apt = ["curl", "git", "build-essential"]
node = "20.x"
npm = { file = "package.json" }

[build]
steps = [
    "npm install",
    "npm run build",
]
cache = ["node_modules", "dist"]

[services.web]
command = "npm run dev"
watch = ["src/**/*.ts", "src/**/*.tsx"]
port = 3000

[services.worker]
command = "node worker.js"
watch = ["worker.js", "lib/**"]
depends_on = ["web"]

[files]
include = ["src", "public", "package.json", "tsconfig.json"]
exclude = [".git", "node_modules/.cache", "*.log"]

[options]
restart_grace = "5s"
```

### Compilation to DAG

```
EnvNode (root)
  ├── BaseNode (ubuntu:22.04 → OCI layers unpacked into CAS)
  ├── PackageNode (apt: curl, git, build-essential)
  ├── PackageNode (node 20.x)
  ├── PackageNode (npm dependencies from lockfile)
  ├── BuildStepNode ("npm install")
  ├── BuildStepNode ("npm run build")
  └── FileNodes (src/, public/, package.json, tsconfig.json)
```

Node hashes are deterministic from inputs. Changing `package.json` invalidates the npm PackageNode and downstream BuildStepNodes. BaseNode and apt packages are untouched.

### Dependency Providers

Pluggable interface:

```rust
trait DependencyProvider {
    fn name(&self) -> &str;
    fn resolve(&self, spec: &Value) -> Result<Vec<PackageNode>>;
    fn install(&self, packages: &[PackageNode], into: &Path) -> Result<()>;
}
```

v1 ships with: **apt**, **npm**, **pip**, **cargo**.

### Lockfile

`hd lock` generates `hd.lock` pinning every dependency to exact version + CAS hash. Checking in `hd.lock` guarantees reproducible environments.

---

## 7. OCI Ingestion (`hd-oci`)

Compatibility bridge for existing Docker users.

### Path 1: OCI Image Pull

When `base = "ubuntu:22.04"` is specified:

1. Resolve image reference via registry
2. Pull manifest and layer blobs
3. Unpack layers sequentially into temp directory
4. Run final filesystem through CAS chunker
5. Build `BaseNode` subtree in DAG

Layers are not stored as layers. Tar archives discarded after unpacking. Two images containing identical `/usr/bin/curl` share chunks — cross-image dedup for free.

### Path 2: Dockerfile Translation

`hd ingest Dockerfile` parses and translates:

```
FROM node:20-alpine          → base = "node:20-alpine"
RUN apk add --no-cache git   → dependencies.apk = ["git"]
COPY package.json .           → files.include = ["package.json"]
RUN npm install               → build.steps = ["npm install"]
COPY . .                      → files.include = ["."]
CMD ["node", "server.js"]    → services.app.command = "node server.js"
```

Best-effort translation. Arbitrary shell commands become `build.steps` entries. User refines the generated `hd.toml`.

### Limitations

- Multi-stage builds flattened; `COPY --from` becomes inter-step dependencies
- `ARG`/`ENV` with dynamic interpolation resolved at ingestion time
- Health checks, ENTRYPOINT/CMD distinction simplified to `services.*.command`

Not aiming for perfect Dockerfile fidelity — it's a one-time migration tool.

### API

```rust
trait OciIngester {
    fn pull_image(&self, reference: &str) -> Result<BaseNode>;
    fn translate_dockerfile(&self, path: &Path) -> Result<EnvSpec>;
}
```

---

## 8. CLI (`hd-cli`)

Thin client communicating with the daemon over a Unix socket.

### Architecture

```
hd <command>  →  Unix socket  →  hd-daemon (long-lived)
                                    ├── hd-engine
                                    ├── hd-cas
                                    ├── hd-watch
                                    ├── hd-sandbox
                                    └── hd-mount (child process)
```

Daemon starts lazily on first `hd` command if not running.

### Core Commands

```
hd up [--detach]          Start environment. Build DAG, mount FS, start services.
hd down                   Stop services, unmount, preserve DAG state.
hd restart [service]      Restart all or specific service. Sandbox persists.
hd exec <cmd>             One-off command inside sandbox.
hd status                 Environment state: services, uptime, last changes.
hd logs [service]         Stream or tail service logs.
hd diff                   DAG diff since last hd up.
```

### Environment Management

```
hd init                   Create new hd.toml.
hd lock                   Generate/update hd.lock.
hd ingest <Dockerfile>    Translate Dockerfile to hd.toml.
```

### Inspection & Debugging

```
hd dag show               Print current DAG tree.
hd dag diff <a> <b>       Diff two DAG states.
hd cas stats              CAS storage usage, dedup ratio.
hd cas gc                 Run garbage collection.
```

### Daemon Management

```
hd daemon start           Start daemon explicitly.
hd daemon stop            Stop daemon and all environments.
hd daemon status          Daemon health and running environments.
```

### Output Philosophy

- `hd up` shows concise progress: which DAG nodes computed vs cached, with timing
- Color-coded service logs with name prefixes
- `hd status` dashboard: services, state, ports, last restart, DAG root hash
- All commands support `--json`

### Exit Codes

0 = success, 1 = general error, 2 = usage error.

---

## v1 Scope Boundaries

**In scope:**
- All 8 crates as described above
- Linux full isolation, macOS light isolation
- Local CAS only
- OCI image ingestion and Dockerfile translation
- apt, npm, pip, cargo dependency providers
- TOML environment spec with lockfile

**Out of scope (future versions):**
- Distributed CAS (S3/remote backing store)
- Language-aware reloaders (AST-level invalidation)
- Docker socket shim
- Process checkpoint/restore (CRIU-lite)
- Programmable API for AI agents
- YAML spec format (TOML only for v1)
- Windows support
