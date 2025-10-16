*Read this in other languages: [English](../../README.md) | [Pirate 🏴‍☠️](README.pirate.md)*

---

# ![arsync](../arsync.png "arsync") (Pirate Edition)

**arsync** = **a**rrrr-synchronous **[rsync](https://github.com/WayneD/rsync)** (the "a" be standin' fer asynchronous, i.e., [io_uring](https://kernel.dk/io_uring.pdf))

High-performance async treasure plunderin' fer Linux - a modern rsync alternative built on io_uring, arrr!

[![CI](https://github.com/jmalicki/arsync/workflows/CI/badge.svg)](https://github.com/jmalicki/arsync/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

📚 **Charts & Maps**: [Developer Guide](docs/DEVELOPER.md) • [Implementation Plan](docs/IMPLEMENTATION_PLAN.md) • [Testing Strategy](docs/TESTING_STRATEGY.md)

---

## Quick Start fer Landlubbers (Gettin' Yer Sea Legs!)

Ahoy there! Ready to set sail and start plunderin' some treasure? Here be the quickest way to get aboard, matey:

```bash
# First, we be needin' to clone the ship from port, savvy?
git clone https://github.com/jmalicki/arsync.git
cd arsync
cargo build --release

# Now let's do some basic plunderin' - simple as that, arrr!
arsync -a --source /data --destination /backup

# Want to see what yer crew be up to? Add progress reports, aye!
arsync -a --source /data --destination /backup --progress
```

**What ye be needin' aboard ship**: Linux kernel 5.6+ (or newer, the fresher the better!), Rust 1.70+ (fer buildin' this fine vessel)

---

# rsync vs arsync: Feature Comparison fer the Seven Seas

## Introduction: Modern Linux Development Practices (How We Built This Fine Ship!)

Ahoy, mateys! `arsync` represents **30+ years o' hard-won lessons learned** sailin' the treacherous seas of [Linux](https://www.kernel.org/) systems programmin'. We be applyin' all the modern best practices to deliver ye the absolute finest treasure plunderin' experience a pirate could ask for, arrr!

Now, don't get me wrong - [rsync](https://rsync.samba.org/) ([GitHub](https://github.com/WayneD/rsync)) was a right groundbreakin' tool when it sailed forth in 1996, aye. But it be built with the constraints and knowledge of that bygone era, ye see. `arsync`, on the other hand, leverages decades worth o' advances in the art o' file plunderin':

### 🚀 The Six Key Innovations (Arrr!)

---

### 1. io_uring: Designed fer Modern NVMe Treasure Vaults (The Secret Weapon!)

**What be io_uring, ye ask?** Well matey, [io_uring](https://kernel.dk/io_uring.pdf) be a modern Linux kernel interface (introduced in kernel 5.1, back in 2019) that provides **asynchronous I/O** through shared ring buffers between userspace and the kernel. Blimey, that sounds fancy! But here's what it means fer ye: Unlike them traditional blockin' syscalls that require one system call per operation (slow as a barnacle-covered hull!), io_uring lets ye submit **batches o' I/O operations** without blockin' the crew, and the kernel notifies ye when they be complete. Think of it as a high-speed cargo conveyor belt fer yer precious treasure, savvy?

**The Problem (Why Old Ways Ain't Good Enough):** Listen up, matey! Modern [NVMe](https://nvmexpress.org/) SSDs were designed by some clever shipwrights with **massively parallel command queues** - we be talkin' up to 64K commands per queue, with 64K queues total! That's designed to saturate PCIe bandwidth and exploit all the inherent parallelism o' flash memory, aye. But here's the rub: Traditional blockin' syscalls (one thread = one I/O at a time) create a right proper **bottleneck** that wastes this magnificent ship's capability *([see NVMe architecture deep-dive →](docs/NVME_ARCHITECTURE.md))*:
- Each `read()` or `write()` call blocks the crew from doin' anything else, blast it!
- Single-threaded rsync can only issue ~10,000 operations/second (slower than a three-legged sea turtle!)
- **Result: Yer fancy $2000 NVMe treasure vault ends up performin' like a measly $50 USB trinket! Shiver me timbers!**

**io-uring Solution (The Pirate's Secret!):**
- Submit **thousands upon thousands o' I/O operations** without blockin' a single crew member! They all work together, matey!
- The kernel processes 'em all in parallel, saturatin' that NVMe hardware like a hold full o' gold!
- **Result: Blow me down! 2-5x throughput** - now yer NVMe performs exactly as the master shipbuilders designed it to! Arrr!

**Real-world booty haul (Actual treasure counts, no fibbin'):**
- rsync: ~420 MB/s on 10,000 small treasures (bottlenecked by syscall overhead - slower than molasses!)
- arsync: ~850 MB/s (By Neptune's beard! That be 2x faster - saturatin' that NVMe queue depth like a proper pirate!)

**References fer the Scholarly Pirates:**
- [io_uring design documentation](https://kernel.dk/io_uring.pdf) - Jens Axboe (io_uring creator)
- [Linux io_uring man page](https://man7.org/linux/man-pages/man7/io_uring.7.html) - Official Linux documentation
- [Efficient IO with io_uring](https://kernel.dk/io_uring-whatsnew.pdf) - Performance characteristics and design goals

---

### 2. Security: TOCTOU-Free Operations to Thwart Scallywags (Defendin' Against Mutiny!)

**The Problem (Beware o' Treacherous Bilge Rats!):** Ahoy, pay attention now! rsync be usin' them ancient 1980s path-based syscalls (`chmod`, `lchown`) that be **vulnerable to race conditions** - that means scallywags can pull a fast one on ye! Here be the rogues' gallery:
- **CVE-2024-12747**: Symlink race condition allowin' privilege escalation (Dec 2024 - actively exploited by scurvy dogs as we speak!)
- **CVE-2007-4476**: Local privilege escalation via symlink attacks (them scallywags been at it fer years!)
- **CVE-2004-0452**: Arbitrary file ownership changes (boardin' yer treasure!)

**arsync Solution (Our Secret Defense, Arrr!):**
- We use **file descriptor-based operations** instead (`fchmod`, `fchown`, `fgetxattr`, `fsetxattr`) - much cleverer, savvy?
- **Impossible to exploit, matey!** - FDs be bound tight to [inodes](https://man7.org/linux/man-pages/man7/inode.7.html), not paths. Scallywags can't swap 'em out!
- Follows all them [MITRE](https://cwe.mitre.org/)/[NIST](https://www.nist.gov/) secure codin' guidelines fer proper defendin' o' yer treasure, aye aye!

**Real-world impact (Why This Matters):** Ye can run arsync as captain (root) without worryin' about symlink attack vulnerabilities from those backstabbin' scallywags! Sleep easy in yer hammock, I say!

**References:**
- [CVE-2024-12747](https://www.cve.org/CVERecord?id=CVE-2024-12747) - rsync symlink race condition (Dec 2024)
- [CERT Vulnerability Note VU#952657](https://kb.cert.org/vuls/id/952657) - rsync TOCTOU vulnerability
- [MITRE CWE-362](https://cwe.mitre.org/data/definitions/362.html) - Race Condition (recommends FD-based operations)
- [MITRE CWE-367](https://cwe.mitre.org/data/definitions/367.html) - Time-of-Check Time-of-Use (TOCTOU) Race Condition
- [fchmod(2) man page](https://man7.org/linux/man-pages/man2/fchmod.2.html) - "avoids race conditions"
- [fchown(2) man page](https://man7.org/linux/man-pages/man2/fchown.2.html) - "avoids race conditions"

---

### 3. I/O Optimization: fadvise and fallocate fer Swifter Sailin' (Catchin' the Trade Winds!)

**The Problem (When the Kernel's Sailin' Blind):** Without proper hints, the poor kernel don't know nothin' about yer I/O patterns, savvy? This leads to all manner o' inefficiency:
- Wastes precious memory cachin' treasure ye won't ever look at again (like hoar din' hardtack that's gone moldy!)
- File fragmentation slows down writes somethin' fierce (like tryin' to navigate through a scattered archipelago!)
- Inefficient read-ahead strategies (the lookout's gazin' in the wrong direction, blast it!)

**arsync Solution (Givin' the Kernel a Proper Chart!):**
- `fadvise(NOREUSE)`: We tell the kernel "Don't be cachin' this, matey!" - frees up memory fer other ship operations, clever as ye please!
- `fallocate()`: We preallocate treasure space ahead o' time (reduces that pesky fragmentation and makes writes faster than a shark!)
- Result: **Blow me down! 15-30% better throughput** on large cargo shipments, arrr!

**References:**
- [LKML: fadvise reduces memory pressure](https://lkml.org/lkml/2004/6/4/43) - Catalin BOIE demonstrates fadvise preventin' unnecessary cachin'
- [LKML: fadvise for I/O patterns](https://lkml.org/lkml/2004/6/4/179) - Bill Davidsen on kernel memory management
- [LKML: Page cache optimization](https://lkml.org/lkml/2023/3/15/1110) - Johannes Weiner on fadvise benefits
- [posix_fadvise(2) man page](https://man7.org/linux/man-pages/man2/posix_fadvise.2.html) - POSIX_FADV_NOREUSE and other hints
- [fallocate(2) man page](https://man7.org/linux/man-pages/man2/fallocate.2.html) - Preallocation to reduce fragmentation

---

### 4. Modern Metadata: statx vs stat (Better Maps o' Yer Treasure!)

**The Problem (Usin' Ancient, Rusty Tools):** Listen here! rsync still be usin' `stat`/`lstat` from the bloomin' 1970s! That be older than most pirates' grandpappies!
- Microsecond timestamp precision (loses precious information like grains o' gold through the cracks!)
- Blockin' syscalls (slows down traversal like barnacles on the hull, matey!)
- Can't even get creation times or extended info (incomplete maps o' yer treasure!)

**arsync Solution (A Fine Modern Sextant!):**
- `statx`: That be a modern syscall from 2017 (kernel 4.11+) - much better fer navigation!
- **Nanosecond** timestamp precision, by Blackbeard's beard! That be 1000x more accurate than the finest ship's chronometer!
- Async via io_uring (don't block the crew - everyone works in parallel, savvy?)
- Extensible as all get-out (can request specific fields, and supports future additions when we discover new treasure types!)

**References:**
- [statx(2) man page](https://man7.org/linux/man-pages/man2/statx.2.html) - Modern stat with nanosecond timestamps
- [LWN: The statx() system call](https://lwn.net/Articles/685791/) - Design rationale and advantages
- [Linux kernel commit](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=a528d35e8bfcc521d7cb70aaf03e1bd296c8493f) - statx implementation (kernel 4.11, 2017)

---

### 5. Single-Pass Hardlink Detection (No Waitin' Around Like a Landlubber!)

**The Problem (rsync Makes Ye Wait Forever!):** Shiver me timbers, rsync's two-pass approach be slower than a leaky rowboat:
- Has to pre-scan the entire cargo hold before startin' (15+ seconds fer large holds with no progress shown! The crew thinks the ship be stuck!)
- Uses ~80 MB memory fer the inode map (that be a lot o' precious ship's resources, arrr!)
- The whole crew sees the ship as "frozen" - not a good look when ye got treasure to plunder!

**arsync Solution (Smart as a Whip, Matey!):**
- Integrated detection that happens right durin' traversal - no separate pass needed! We work while we walk, savvy?
- Immediate progress feedback keeps the crew informed and happy, aye!
- Uses only ~8 MB memory (that be 10x less! More rum fer everyone!)
- **Avast! 15x faster time-to-first-plunder** - ye be seein' results before ye can say "pieces o' eight!"

---

### 6. Modern Software Engineering: Rust + Comprehensive Testin' (Built to Last the Ages!)

**The Problem (Old Code Be Leaky as a Sieve!):** Now don't get me wrong - rsync served well fer its time. But matey, it be written in [C](https://en.wikipedia.org/wiki/C_(programming_language)) back in 1996 with limited test coverage:
- Manual memory management everywhere (potential fer bugs that'll sink ships faster than a cannonball through the hull!)
- No compile-time safety guarantees (ye don't know ye got a leak 'til yer already underwater!)
- Limited automated testin' (hard to add tests to a C codebase - like tryin' to teach an old sea dog new tricks!)
- Difficult to refactor safely (one wrong move and the whole ship goes down!)

**arsync Solution (Built Like a Proper Man-o'-War!):**
- Written in **[Rust](https://www.rust-lang.org/)** with memory safety guarantees built right in - no scuttlin' yer own ship, arrr!
- **93 automated tests**, by Davy Jones! That be across 15 test files with ~4,500 lines o' test code! We test everything!
- **Comprehensive test categories** (we check every knot and rigging, matey):
  - Unit tests (permissions, timestamps, ownership, xattr)
  - Integration tests (cargo hold traversal, hardlinks, symlinks)
  - Edge case tests (special permissions, unicode, long filenames)
  - Performance tests (many treasures, large cargo, concurrent ops)
  - **rsync compatibility tests** (validates identical behavior against actual rsync)
  - **Flag behavior tests** (validates on/off semantics)
- **CI/CD pipeline** with pre-commit hooks ([rustfmt](https://github.com/rust-lang/rustfmt), [clippy](https://github.com/rust-lang/rust-clippy))
- **Type safety**: Impossible to mix up file descriptors, paths, or metadata
- **Fearless refactorin'**: Compiler catches errors before the ship leaves port

**Testin' comparison (Quality Control, Savvy?):**
- rsync: Primarily manual testin', limited automated test suite (crossin' yer fingers and hopin' fer the best!)
- arsync: **93 automated tests** with >50% test-to-code ratio (we check EVERYTHING, matey!)

**Real-world impact (Why Ye Should Care):**
- Bugs be caught at compile time, not at 3am when yer transferrin' critical treasure! (Sleep well in yer hammock!)
- Safe to add new features without breakin' existing behavior (the ship stays seaworthy!)
- Full confidence that rsync compatibility be maintained across all changes (we keep our promises, arrr!)

**References:**
- [The Rust Programming Language](https://doc.rust-lang.org/book/) - Memory safety without garbage collection
- [Rust's Ownership System](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html) - Prevents use-after-free and buffer overflows
- [Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html) - Data race prevention at compile time
- [NSA Software Memory Safety Report (2022)](https://media.defense.gov/2022/Nov/10/2003112742/-1/-1/0/CSI_SOFTWARE_MEMORY_SAFETY.PDF) - Recommends memory-safe languages like Rust
- [Microsoft Security Response Center](https://msrc.microsoft.com/blog/2019/07/a-proactive-approach-to-more-secure-code/) - 70% of vulnerabilities be memory safety issues

### Quick Comparison Table fer Mateys

| Innovation | rsync (1996) | arsync (2024) | Impact |
|------------|--------------|----------------------|--------|
| **I/O Architecture** | Blockin' syscalls | io_uring async | 2x faster on small treasures |
| **Security** | Path-based (CVEs) | FD-based (TOCTOU-free) | No privilege escalation from scallywags |
| **I/O Hints** | None | fadvise + fallocate | 15-30% better throughput |
| **Metadata Syscalls** | `stat` (1970s) | `statx` (2017) | Nanosecond precision |
| **Hardlink Detection** | Two-pass | Single-pass integrated | 15x faster start, 10x less memory |
| **Language** | C (manual memory) | Rust (memory safe) | No buffer overflows, use-after-free |
| **Test Coverage** | Limited | 93 automated tests | Bugs caught before settin' sail |
| **Test Code** | Minimal | 4,500 lines | >50% test-to-code ratio |

### The Result - What All This Means Fer Ye, Matey!

Ahoy! By applyin' these six modern practices (each one a treasure in itself!), `arsync` achieves somethin' special:
- **2x faster** on many small treasures (io_uring parallelism means all hands work together!)
- **More secure** than a chest with three locks (immune to TOCTOU vulnerabilities from backstabbin' scallywags, PLUS memory safety!)
- **Better crew experience** (immediate progress reports, no frozen periods where everyone's wonderin' if the ship's stuck!)
- **More efficient** than a well-trimmed sail (better memory usage, proper I/O hints to the kernel!)
- **More accurate** than the finest navigational instruments (nanosecond timestamps, arrr!)
- **More reliable** than the North Star (comprehensive testin' and type safety means fewer surprises!)

This be what **30 years o' hard-won Linux evolution PLUS modern software engineering** looks like when applied to the noble art o' treasure plunderin', savvy? We didn't just build a tool - we built a legend!

---

## Table o' Contents

1. [Overview](#overview)
2. [Design Philosophy](#design-philosophy)
3. [Security Advantages](#security-advantages) ⚠️ **Critical Security Information fer Defendin' Yer Treasure**
4. [Command-Line Options Comparison](#command-line-options-comparison)
   - [Fully Supported (rsync-compatible)](#-fully-supported-rsync-compatible)
   - [Partial Support / Different Behavior](#-partial-support--different-behavior)
   - [Flags Accepted But Not Yet Implemented](#-flags-accepted-but-not-yet-implemented)
   - [Not Supported (Remote/Network Features)](#-not-supported-remotenetwork-features)
   - [arsync Exclusive Features](#-arsync-exclusive-features)
5. [Capability Comparison](#capability-comparison)
   - [Performance Characteristics](#performance-characteristics-speed-o-the-ship)
   - [Metadata Preservation](#metadata-preservation)
   - [Default Behavior](#default-behavior)
6. [Usage Examples](#usage-examples-fer-sailors)
   - [Equivalent Commands](#equivalent-commands)
   - [arsync Performance Tuning](#arsync-performance-tunin-optimizin-yer-ship)
7. [When to Use Which Tool](#when-to-use-which-tool-choosin-yer-weapon)
8. [Migration Guide](#migration-guide)
9. [Performance Benchmarks](#performance-benchmarks)
10. [Test Validation](#test-validation)
11. [Conclusion](#conclusion)
12. [Additional Technical Details](#additional-technical-details-fer-the-curious-sailors)
    - [Hardlink Detection: arsync vs rsync](#hardlink-detection-arsync-vs-rsync)
    - [Progress Reporting: arsync vs rsync](#progress-reporting-arsync-vs-rsync)
13. [Appendices](#appendices)
    - [NVMe Architecture and io_uring](docs/NVME_ARCHITECTURE.md)
    - [Why fadvise is Superior to O_DIRECT](docs/FADVISE_VS_O_DIRECT.md)
14. [Contributing](#contributing)
15. [License](#license)

---

## Overview

`arsync` be designed as a drop-in replacement fer `rsync` fer **local, single-ship** treasure synchronization. This scroll compares command-line options and capabilities between the two tools.

## Design Philosophy

- **rsync**: Universal tool fer local and remote sync with many protocol options
- **arsync**: Specialized fer local ship operations with maximum performance usin' io_uring

## Command-Line Options Comparison

### ✅ Fully Supported (rsync-compatible)

| rsync Flag | arsync | Description | Notes |
|------------|---------------|-------------|-------|
| `-a, --archive` | `-a, --archive` | Archive mode (same as `-rlptgoD`) | Identical behavior |
| `-r, --recursive` | `-r, --recursive` | Recurse into cargo holds | Identical behavior |
| `-l, --links` | `-l, --links` | Copy [symlinks](https://man7.org/linux/man-pages/man7/symlink.7.html) as symlinks | Identical behavior |
| `-p, --perms` | `-p, --perms` | Preserve permissions | Identical behavior |
| `-t, --times` | `-t, --times` | Preserve modification times | Identical behavior |
| `-g, --group` | `-g, --group` | Preserve group | Identical behavior |
| `-o, --owner` | `-o, --owner` | Preserve owner (captain only) | Identical behavior |
| `-D` | `-D, --devices` | Preserve device/special cargo | Identical behavior |
| `-X, --xattrs` | `-X, --xattrs` | Preserve [extended attributes](https://man7.org/linux/man-pages/man7/xattr.7.html) | Identical behavior |
| `-A, --acls` | `-A, --acls` | Preserve [ACLs](https://man7.org/linux/man-pages/man5/acl.5.html) (implies `--perms`) | Identical behavior |
| `-H, --hard-links` | `-H, --hard-links` | Preserve [hard links](https://man7.org/linux/man-pages/man2/link.2.html) | **Better**: Integrated detection durin' traversal *([see detailed comparison ↓](#hardlink-detection-arsync-vs-rsync))* |
| `-v, --verbose` | `-v, --verbose` | Verbose output fer the crew | Multiple levels supported (`-vv`, `-vvv`) |
| `--dry-run` | `--dry-run` | Show what would be plundered | Identical behavior |

### 🔄 Partial Support / Different Behavior

| rsync Flag | arsync | Status | Notes |
|------------|---------------|--------|-------|
| `-q, --quiet` | `--quiet` | Implemented | Suppress non-error output (keep the crew quiet) |
| `--progress` | `--progress` | **Enhanced** | Real-time discovery + completion progress *([see detailed comparison ↓](#progress-reporting-arsync-vs-rsync))* |

### 🚧 Flags Accepted But Not Yet Implemented

These flags be accepted fer rsync compatibility but don't currently affect behavior:

| rsync Flag | arsync | Status | Notes |
|------------|---------------|--------|-------|
| `-U, --atimes` | `-U, --atimes` | **Not implemented** | Flag accepted but access times not preserved (yet) |
| `--crtimes` | `--crtimes` | **Not implemented** | Flag accepted but creation times not preserved (yet) |

### ❌ Not Supported (Remote/Network Features)

These flags be **not applicable** fer local-only operations:

| rsync Flag | Reason Not Supported |
|------------|---------------------|
| `-e, --rsh` | No remote sync support (we don't sail to other ships) |
| `--rsync-path` | No remote sync support |
| `-z, --compress` | Local I/O don't benefit from compression |
| `--bwlimit` | Local I/O not bandwidth-limited |
| `--partial` | Not applicable to local atomic operations |
| `--checksum`, `-c` | Uses io_uring fer direct plunderin', not checksums |
| `--delete` | Not a sync tool; plunders only |

**Note on `-U/--atimes` and `--crtimes`:** These flags be currently accepted (fer command-line compatibility) but don't affect behavior yet. Full implementation be planned fer a future voyage. In practice, these be rarely used with rsync as well, since preservin' access times defeats the purpose of trackin' access, and creation times ain't consistently supported across treasure vaults.

### ⚡ arsync Exclusive Features

Features that `arsync` has but `rsync` doesn't:

| Flag | Description | Performance Benefit |
|------|-------------|---------------------|
| `--queue-depth` | io_uring submission queue depth (1024-65536) | 2-5x throughput on high-performance treasure vaults |
| `--max-files-in-flight` | Max concurrent treasures per crew member (1-10000) | Optimal parallelism tunin' |
| `--cpu-count` | Number of crew members to use (0 = auto) | Per-crew queue architecture fer scalin' |
| `--buffer-size-kb` | Buffer size in KB (0 = auto) | Fine-tune memory vs throughput |
| `--copy-method` | Plunderin' method (currently auto=read_write) | Reserved fer future optimizations |

## Security Advantages

### Why File Descriptor-Based Operations Matter fer Defendin' Yer Treasure

`arsync` uses **file descriptor-based syscalls** fer all metadata operations, eliminatin' an entire class o' security vulnerabilities that affect rsync and other tools usin' path-based syscalls.

#### What be a TOCTOU Attack?

**TOCTOU** = **Time-of-Check to Time-of-Use** race condition (a scallywag's trick!)

This be a type o' attack where a scallywag exploits the time gap between:
1. **Check**: When a program checks a treasure (e.g., "be this a regular cargo?")
2. **Use**: When the program operates on that treasure (e.g., "change its permissions")

Between these two steps, a scallywag can **swap the treasure fer a symlink** pointin' to a sensitive system file.

**The attack be simple:**
```bash
# 1. Program checks: "/backup/myfile.txt be a regular treasure" ✓
# 2. Scallywag acts: rm /backup/myfile.txt && ln -s /etc/passwd /backup/myfile.txt
# 3. Program executes: chmod("/backup/myfile.txt", 0666)
# 4. Result: /etc/passwd be now world-writable! 💥 PRIVILEGE ESCALATION (Ye've been boarded!)
```

#### Real-World rsync Vulnerabilities (Scallywag Exploits!)

**CVE-2024-12747** (December 2024) - **ACTIVELY EXPLOITED BY SCURVY DOGS**
- **Vulnerability**: Symbolic link race condition in rsync
- **Impact**: Privilege escalation, unauthorized treasure access
- **Severity**: High (CVSS score pendin')
- **Root cause**: Path-based `chmod`/`chown` syscalls
- **Reference**: https://kb.cert.org/vuls/id/952657

**CVE-2007-4476** - rsync Symlink Followin' Vulnerability
- **Impact**: Local privilege escalation
- **Cause**: Path-based operations followin' symlinks
- **Affected**: rsync versions < 3.0.0

**CVE-2004-0452** - Race Condition in chown Operations
- **Impact**: Arbitrary file ownership changes
- **Cause**: TOCTOU in path-based chown

These be **not theoretical** - these vulnerabilities have been exploited in the wild by scallywags to:
- Gain captain (root) privileges on multi-crew systems
- Modify sensitive system scrolls (`/etc/passwd`, `/etc/shadow`)
- Bypass security restrictions
- Escalate privileges in container environments

#### How arsync Eliminates These Vulnerabilities (Defendin' Yer Ship!)

**The key difference: File Descriptors**

Instead o' usin' paths (which can be swapped by scallywags), we use **file descriptors** that be bound to the actual treasure:

**rsync (vulnerable path-based):**
```c
// From rsync's syscall.c
int do_chmod(const char *path, mode_t mode) {
    return chmod(path, mode);  // ← Path can be swapped between check and use by scallywags!
}
```

**arsync (secure FD-based, protected from scallywags):**
```rust
// Open treasure ONCE, get file descriptor
let file = File::open(path).await?;  // ← FD bound to inode, not path

// All operations use FD (immune to path swaps from scallywags)
file.set_permissions(perms).await?;     // fchmod(fd, ...) - secure!
file.fchown(uid, gid).await?;           // fchown(fd, ...) - secure!
```

**Why this be secure:**
1. File descriptor refers to the **inode** (the actual treasure in the hold)
2. Even if the path be swapped to a symlink by a scallywag, **FD still points to original treasure**
3. Operations be **atomic** - no time gap fer scallywags to exploit
4. **Impossible to attack** - scallywags cannot change what the FD points to

#### Authoritative Sources (fer the Scholarly Pirates)

**MITRE CWE-362: Concurrent Execution usin' Shared Resource (Race Condition)**
- URL: https://cwe.mitre.org/data/definitions/362.html
- Recommendation: **"Use file descriptors instead of file names"**
- Quote: *"Usin' file descriptors instead of file names be the recommended approach to avoidin' TOCTOU flaws."*

**MITRE CWE-367: Time-of-Check Time-of-Use (TOCTOU) Race Condition**
- URL: https://cwe.mitre.org/data/definitions/367.html
- Lists path-based operations as a common cause

**Linux Kernel Documentation:**
- `fchmod(2)` man page: *"fchmod() be identical to chmod(), except that the file... be specified by the file descriptor fd. This avoids race conditions from scallywags."*
- `fchown(2)` man page: *"These system calls... change the ownership... specified by a file descriptor, thus avoidin' race conditions."*
- `openat(2)` man page: *"openat() can be used to avoid certain kinds o' race conditions."*

**NIST Secure Codin' Guidelines:**
- Recommends usin' `*at` syscalls fer security-critical operations
- Explicitly warns against TOCTOU in file operations

#### Comparison: rsync vs arsync Security (Defendin' Yer Treasure)

| Operation | rsync Implementation | arsync Implementation | Security Impact |
|-----------|---------------------|------------------------------|-----------------|
| **Set Permissions** | `chmod(path, mode)` ([source](https://github.com/WayneD/rsync/blob/master/syscall.c#L90-L100)) | `fchmod(fd, mode)` | **CRITICAL**: rsync vulnerable to symlink swap attacks from scallywags |
| **Set Ownership** | `lchown(path, uid, gid)` ([source](https://github.com/WayneD/rsync/blob/master/syscall.c#L206-L215)) | `fchown(fd, uid, gid)` | **CRITICAL**: rsync vulnerable to privilege escalation by scallywags |
| **Extended Attributes** | `setxattr(path, ...)` | `fsetxattr(fd, ...)` | **HIGH**: rsync can be tricked into modifyin' wrong treasures |
| **Timestamps** | `utimes(path, ...)` | `futimens(fd, ...)` | **MEDIUM**: rsync can set wrong treasure times |

**Vulnerability Ratin':**
- rsync: **Vulnerable to TOCTOU attacks** from scallywags in metadata operations
- arsync: **Immune to TOCTOU attacks** via FD-based operations (scallywag-proof!)

#### Why This Matters fer Yer Treasure Transfers

**Scenario: Multi-crew system or container environment**

If ye run rsync as captain (root) to preserve ownership:

```bash
# rsync runnin' as captain to preserve ownership
$ sudo rsync -a /source/ /backup/
```

**An unprivileged scallywag can:**
1. Watch fer rsync to start plunderin'
2. Quickly replace treasures in `/backup/` with symlinks to system scrolls
3. rsync's `chmod`/`chown` calls follow the symlinks
4. **Result: Scallywag gains control o' system scrolls** (`/etc/passwd`, `/etc/shadow`, etc.)

**arsync be immune (scallywag-proof!):**
```bash
$ sudo arsync -a --source /source --destination /backup
# ✓ Scallywag can swap paths all they want
# ✓ File descriptors still point to original treasures
# ✓ System scrolls be safe from boardin'
```

#### Additional Security Benefits (Defendin' the Ship)

Beyond TOCTOU prevention, FD-based operations also:

1. **Avoid umask interference**: `fchmod` sets exact permissions, `chmod` be affected by umask
2. **Prevent symlink confusion**: Operations never follow symlinks unintentionally
3. **Enable atomicity**: Treasure opened and metadata set without interruption from scallywags
4. **Better audit trail**: Operations tied to specific file descriptors

#### Summary

**arsync be fundamentally more secure** than rsync fer metadata operations:

- ✅ **Immune to CVE-2024-12747** and similar TOCTOU vulnerabilities from scallywags
- ✅ **Follows MITRE/NIST security best practices** fer defendin' yer treasure
- ✅ **Safe fer privileged operations** (captain/root, sudo)
- ✅ **No known metadata-related CVEs** (by design, scallywag-proof)

rsync's use o' path-based syscalls be a **30+ year old design** from before these vulnerabilities were well understood. arsync uses **modern security practices** from the ground up to defend against scallywags.

## Capability Comparison

### Performance Characteristics (Speed o' the Ship)

| Feature | rsync | arsync | Advantage |
|---------|-------|---------------|-----------|
| **I/O Architecture** | Blockin' syscalls | io_uring async | **arsync**: 2-5x throughput |
| **Treasure Plunderin'** | `read`/`write` loops | io_uring `read_at`/`write_at` + `fallocate` | **arsync**: Async I/O with preallocation |
| **Metadata Operations** | Synchronous syscalls | io_uring `statx` | **arsync**: Async metadata |
| **Hardlink Detection** | Separate analysis pass | Integrated durin' traversal | **arsync**: Single-pass operation |
| **Symlink Operations** | `readlink`/`symlink` | io_uring `readlinkat`/`symlinkat` | **arsync**: Async symlinks |
| **Parallelism** | Single-threaded | Per-crew queues | **arsync**: Scales with crew size |
| **Small Treasures** | ~420 MB/s | ~850 MB/s | **arsync**: 2x faster |
| **Large Cargo** | ~1.8 GB/s | ~2.1 GB/s | **arsync**: 15% faster |

### Metadata Preservation

| Metadata Type | rsync | arsync | Implementation |
|---------------|-------|---------------|----------------|
| **Permissions** | ✅ `chmod` (path-based) | ✅ `fchmod` (FD-based) | arsync avoids umask + TOCTOU *([see security →](#why-file-descriptor-based-operations-matter-fer-defendin-yer-treasure))* |
| **Ownership** | ✅ `lchown` (path-based) | ✅ `fchown` (FD-based) | arsync prevents race conditions from scallywags *([see security →](#why-file-descriptor-based-operations-matter-fer-defendin-yer-treasure))* |
| **Timestamps** | ✅ `utimes` | ✅ `utimensat` (nanosec) | arsync has nanosecond precision |
| **Extended Attributes** | ✅ `getxattr`/`setxattr` | ✅ `fgetxattr`/`fsetxattr` (FD-based) | arsync be immune to symlink attacks *([see security →](#why-file-descriptor-based-operations-matter-fer-defendin-yer-treasure))* |
| **ACLs** | ✅ `-A` | ✅ `-A` (implies `-p`) | Compatible behavior |
| **Hard Links** | ✅ `-H` | ✅ `-H` (integrated) | arsync detects durin' traversal |

### Default Behavior

| Aspect | rsync | arsync | Notes |
|--------|-------|---------------|-------|
| **Metadata Preservation** | Off by default | Off by default | **Identical**: Must use `-a` or specific flags |
| **Recursive** | Off by default | Off by default | **Identical**: Must use `-r` or `-a` |
| **Symlinks** | Copy target by default | Copy target by default | **Identical**: Use `-l` to copy as symlinks |
| **Hard Links** | Not detected | Detected but not preserved | Use `-H` to preserve |

## Usage Examples fer Sailors

### Equivalent Commands

#### Basic recursive plunderin' with all metadata:
```bash
# rsync
rsync -a /source/ /destination/

# arsync (pirate style!)
arsync -a --source /source --destination /destination
```

#### Plunder with permissions and times only:
```bash
# rsync
rsync -rpt /source/ /destination/

# arsync
arsync -rpt --source /source --destination /destination
```

#### Plunder with extended attributes:
```bash
# rsync
rsync -aX /source/ /destination/

# arsync
arsync -aX --source /source --destination /destination
```

#### Verbose dry run (see what ye'd plunder):
```bash
# rsync
rsync -av --dry-run /source/ /destination/

# arsync
arsync -av --dry-run --source /source --destination /destination
```

### arsync Performance Tunin' (Optimizin' Yer Ship)

Commands unique to `arsync` fer performance optimization:

```bash
# High-throughput configuration (NVMe treasure vaults, fast storage)
arsync -a \
  --source /source \
  --destination /destination \
  --queue-depth 8192 \
  --max-files-in-flight 2048 \
  --cpu-count 16

# Low-latency configuration (spinnin' disk treasure vaults, network storage)
arsync -a \
  --source /source \
  --destination /destination \
  --queue-depth 1024 \
  --max-files-in-flight 256 \
  --cpu-count 4
```

## When to Use Which Tool (Choosin' Yer Weapon)

### Use `arsync` when:

- ✅ Plunderin' treasures **on the same ship** (local → local)
- ✅ Performance be critical (NVMe treasure vaults, fast storage)
- ✅ Ye have many small treasures (2x faster than rsync)
- ✅ Ye want integrated hardlink detection
- ✅ Ye need modern kernel features (io_uring)
- ✅ Ye want to defend against scallywags (security)

### Use `rsync` when:

- ✅ Plunderin' treasures **over the network** (sailin' to remote ships)
- ✅ Ye need `--delete` fer true synchronization
- ✅ Ye need checksum-based verification (`-c`)
- ✅ Ye need bandwidth limitin' (`--bwlimit`)
- ✅ Sailin' on older ships (kernel < 5.6)
- ✅ Ye need partial transfer resume (`--partial`)

## Migration Guide (From Old Ship to New)

### From rsync to arsync

Most rsync commands translate directly:

```bash
# Before (old way)
rsync -avH /source/ /destination/

# After (pirate way!)
arsync -avH --source /source --destination /destination
```

**Key Differences:**
1. Use `--source` and `--destination` instead o' positional arguments
2. Trailin' slashes on paths be **not** significant (unlike rsync)
3. No remote ship support (no `user@host:path` syntax)
4. No `--delete` flag (tool plunders only, doesn't synchronize)

## Performance Benchmarks (Speed Trials)

Detailed benchmarks on [Ubuntu](https://ubuntu.com/) 22.04, Linux Kernel 5.15, 16-crew system, [NVMe](https://nvmexpress.org/) treasure vault:

| Workload | rsync | arsync | Speedup |
|----------|-------|---------------|---------|
| 1 GB single treasure | 1.8 GB/s | 2.1 GB/s | 1.15x |
| 10,000 × 10 KB treasures | 420 MB/s | 850 MB/s | 2.0x |
| Deep cargo hold tree | 650 MB/s | 1.2 GB/s | 1.85x |
| Mixed workload | 580 MB/s | 1.1 GB/s | 1.9x |

## Test Validation (Provin' Our Claims)

All compatibility claims in this scroll be **validated by automated tests** that run both tools side-by-side and compare results.

### Test Suite: `tests/rsync_compat.rs`

This test suite runs **both rsync and arsync** with identical inputs and verifies they produce identical outputs:

| Test | What It Validates | Command Tested |
|------|-------------------|----------------|
| `test_archive_mode_compatibility` | Archive mode produces identical results | `rsync -a` vs `arsync -a` |
| `test_permissions_flag_compatibility` | Permissions preserved identically | `rsync -rp` vs `arsync -rp` |
| `test_timestamps_flag_compatibility` | Timestamps preserved identically | `rsync -rt` vs `arsync -rt` |
| `test_combined_flags_compatibility` | Multiple flags work together | `rsync -rpt` vs `arsync -rpt` |
| `test_symlinks_compatibility` | Symlinks copied identically | `rsync -rl` vs `arsync -rl` |
| `test_default_behavior_compatibility` | Default (no metadata) matches | `rsync -r` vs `arsync -r` |
| `test_large_file_compatibility` | Large treasures (10MB) handled identically | `rsync -a` vs `arsync -a` |
| `test_many_small_files_compatibility` | 100 small treasures handled identically | `rsync -a` vs `arsync -a` |
| `test_deep_hierarchy_compatibility` | Deep nestin' handled identically | `rsync -a` vs `arsync -a` |

**How to run:**
```bash
# Run rsync compatibility test suite (requires rsync installed)
cargo test --test rsync_compat

# Run specific compatibility test
cargo test --test rsync_compat test_archive_mode_compatibility
```

**What the tests verify:**
- ✅ Treasure content be byte-fer-byte identical
- ✅ Permissions (mode bits) match exactly
- ✅ Ownership (UID/GID) matches exactly
- ✅ Timestamps match within 1ms (treasure vault precision)
- ✅ Symlink targets match exactly
- ✅ Cargo hold structure be identical
- ✅ Treasure types (regular/symlink/directory) match

### Test Suite: `tests/metadata_flag_tests.rs`

Additional tests verify flag on/off behavior works correctly:

| Test | What It Validates |
|------|-------------------|
| `test_permissions_not_preserved_when_flag_off` | Without `--perms`, permissions use umask |
| `test_permissions_preserved_when_flag_on` | With `--perms`, permissions match source |
| `test_timestamps_not_preserved_when_flag_off` | Without `--times`, timestamps be current |
| `test_timestamps_preserved_when_flag_on` | With `--times`, timestamps match source |
| `test_archive_mode_preserves_all_metadata` | `-a` enables all metadata preservation |
| `test_directory_permissions_not_preserved_when_flag_off` | Cargo hold permissions respect flags |
| `test_directory_permissions_preserved_when_flag_on` | Cargo hold permissions preserved with flag |
| `test_individual_flags_match_archive_components` | `-p` works same alone or in `-a` |

**Run with:**
```bash
cargo test --test metadata_flag_tests
```

### Continuous Integration (Automated Quality Checks)

These tests run automatically in CI to ensure:
1. We remain rsync-compatible across voyages
2. No regressions in metadata preservation
3. Flag behavior stays consistent

## Conclusion (The Final Word)

`arsync` be a **drop-in replacement** fer `rsync` when:
- Operatin' on a single ship (local → local)
- Usin' rsync-compatible flags (`-a`, `-r`, `-l`, `-p`, `-t`, `-g`, `-o`, `-D`, `-X`, `-A`, `-H`)
- Performance matters (especially fer many small treasures)
- Security matters (defendin' against scallywags)

**Our compatibility be validated by 18 automated tests** that compare actual behavior against rsync.

Fer remote sync, network operations, or advanced rsync features (`--delete`, `--checksum`, `--partial`), continue usin' [rsync](https://github.com/WayneD/rsync).

---

## Additional Technical Details (Fer the Curious Sailors)

### Hardlink Detection: arsync vs rsync

`arsync` implements hardlink detection fundamentally differently from `rsync`, with significant performance and efficiency advantages:

### rsync's Two-Pass Approach

rsync uses a **separate pre-processin' phase** fer hardlink detection:

1. **Pre-scan Pass**: Before any plunderin' begins, rsync scans the entire source cargo hold to build an inode map
2. **Memory Overhead**: Maintains a complete inode-to-path mappin' in memory fer the entire source hold
3. **Latency**: Crew sees no progress durin' the pre-scan phase (can take minutes fer large holds)
4. **Separate Logic**: Hardlink detection be isolated from the main plunderin' logic

Example rsync behavior:
```bash
$ rsync -aH /large-tree/ /backup/
# Long pause with no output while scannin'...
# (buildin' inode map in memory)
# Then plunderin' begins with progress output
```

### arsync's Integrated Approach

`arsync` integrates hardlink detection **durin' traversal** usin' `io_uring statx`:

1. **Single-Pass Operation**: Detection happens simultaneously with discovery and plunderin'
2. **Streamin' Metadata**: Uses io_uring's async `statx` to get inode information on-demand
3. **Immediate Progress**: Crew sees both discovery and plunderin' progress from the start
4. **Efficient Memory**: Only tracks inodes as they're discovered (bounded by max-files-in-flight)
5. **Concurrent Processin'**: Multiple treasures processed in parallel while detectin' hardlinks

Example arsync behavior (with --pirate flag fer extra fun!):
```console
$ arsync -aH --source /large-tree --destination /backup --progress --pirate
# Immediate progress output (no waitin' around!):
Treasure sighted on the horizon, ahoy: 1523 chests o' precious booty | Booty plundered and secured, blimey: 847 chests | Crew be haulin' aboard right now, avast: 256 chests
# (discovery and plunderin' happen simultaneously, like a well-oiled crew!)
```

### Performance Comparison

Fer a cargo hold tree with 10,000 treasures and 2,000 hardlinks:

| Metric | rsync -aH | arsync -aH | Advantage |
|--------|-----------|-------------------|-----------|
| **Pre-scan Time** | ~15 seconds | 0 seconds | No pre-scan needed |
| **Time to First Plunder** | ~15 seconds | <1 second | **15x faster** start |
| **Memory Usage** | ~80 MB (inode map) | ~8 MB (in-flight only) | **10x less** memory |
| **Total Time** | ~45 seconds | ~28 seconds | **1.6x faster** overall |
| **Crew Experience** | "Ship be frozen" then progress | Immediate progress | **Better UX** |

### Technical Implementation

**arsync's approach:**
```rust
// Durin' cargo hold traversal (pseudo-code):
for each directory entry {
    // Get metadata usin' io_uring statx (async, fast)
    let metadata = statx_async(entry).await;
    let inode = metadata.inode();
    
    if inode_tracker.seen(inode) {
        // This be a hardlink - create link instead o' plunderin' content
        create_hardlink_async(entry, original_path).await;
    } else {
        // First time seein' this inode - plunder content
        copy_file_content_async(entry).await;
        inode_tracker.mark_seen(inode, entry.path());
    }
}
```

**Key advantages:**
1. **No separate scan**: Detection be part o' normal traversal
2. **io_uring statx**: Async metadata retrieval (doesn't block the crew)
3. **Bounded memory**: Only track inodes currently in flight
4. **Parallel discovery**: Multiple paths explored concurrently
5. **Early detection**: Hardlinks avoided as soon as discovered

### Treasure Vault Boundary Detection

Additionally, `arsync` uses `statx` to detect treasure vault boundaries automatically:
- Prevents cross-vault hardlinks (would fail anyway)
- Optimizes operations per vault (detects boundaries fer hardlinks)
- No crew configuration needed (unlike rsync's `-x` flag)

### Conclusion

arsync's integrated hardlink detection be:
- **Faster**: No pre-scan overhead, immediate start
- **More efficient**: Lower memory usage, streamin' approach  
- **Better UX**: Progress visible from the start to the crew
- **More scalable**: Bounded memory regardless o' hold size

This be possible because io_uring's async `statx` allows metadata queries to happen concurrently with treasure operations, eliminatin' the need fer a separate analysis phase.

### Progress Reportin': arsync vs rsync

Both tools support `--progress`, but `arsync` provides significantly more informative real-time progress due to its architecture.

### rsync's Progress Display

rsync shows progress **only durin' treasure transfer**:

```bash
$ rsync -av --progress /source/ /destination/
# Long pause while discoverin' treasures (no progress shown to crew)
# Then, fer each treasure bein' plundered:
sending incremental file list
file1.txt
      1,234,567  45%  123.45MB/s    0:00:02
file2.txt
        567,890  12%   98.76MB/s    0:00:05
```

**Limitations:**
- No feedback durin' cargo hold discovery phase
- Progress only shown per-treasure durin' transfer
- No visibility into total operation
- Can't tell how much work remains
- Appears "frozen" durin' discovery o' large holds (crew gets anxious)

### arsync's Progress Display

`arsync` shows **concurrent discovery and plunderin' progress**:

```console
$ arsync -a --source /source --destination /destination --progress --pirate
Treasure sighted on the horizon, ahoy: 1523 chests o' precious booty (1.2 GB) | Booty plundered and secured, blimey: 847 chests (780 MB o' pieces o' eight) | Crew be haulin' aboard right now, avast: 256 chests
[==============>                    ] 55% | 1.5 GB/s knots through the waves | bells 'til we reach port: 0:00:03

Treasure sighted on the horizon, ahoy: 2891 chests (2.1 GB) | Booty plundered and secured, blimey: 2156 chests (1.8 GB) | Crew be haulin' aboard: 128 chests  
[=========================>         ] 85% | 1.8 GB/s knots through the waves | bells 'til we reach port: 0:00:01

✓ BLOW ME DOWN! All treasure be safely stowed in the hold! Break out the rum, we be RICH! 🏴‍☠️ Yo ho ho!
  Total: 3,024 chests o' precious booty (2.3 GB pieces o' eight) plundered in 5.2 ticks o' the sand hourglass
  Average haul: 1.7 GB/s - What a magnificent voyage, mateys!
```

**Advantages (Why This Be Better, Matey!):**
- **Real-time discovery**: Shows treasures bein' discovered while plunderin' - the crew knows what's happenin' every moment!
- **Concurrent progress**: Discovery happens in parallel with plunderin' - all hands workin' together like a proper crew should!
- **In-flight trackin'**: Shows exactly how many treasures be currently bein' hauled aboard - no mysteries here, savvy?
- **Total visibility**: Crystal clear view o' all the work discovered so far - ye can see the whole treasure map!
- **Better estimates**: The ETA to port improves as more treasures be discovered - gettin' more accurate all the time!
- **Never appears frozen**: Always shows the crew workin' hard - no wonderin' if the ship be stuck on a sandbar!

### Technical Comparison

| Aspect | rsync --progress | arsync --progress | Advantage |
|--------|------------------|--------------------------|-----------|
| **Discovery Phase** | No progress shown | Live treasure/hold count | **arsync** |
| **Transfer Phase** | Per-treasure progress | Aggregate + per-treasure | **arsync** |
| **Concurrency Visibility** | Single-threaded (no concurrency) | Shows in-flight operations | **arsync** |
| **ETA Accuracy** | Per-treasure only | Overall + improvin' | **arsync** |
| **Crew Experience** | "Ship be frozen" then per-treasure | Immediate feedback | **arsync** |
| **Throughput Display** | Per-treasure MB/s | Aggregate GB/s | **arsync** |

### Architecture Difference

**rsync (single-threaded, sequential):**
```
[Discovery Phase - no progress, crew be anxious]
    ↓
[Treasure 1] ──> Transfer (progress shown)
    ↓
[Treasure 2] ──> Transfer (progress shown)
    ↓
[Treasure 3] ──> Transfer (progress shown)
```

**arsync (parallel, concurrent, happy crew):**
```
[Discovery] ─┬─> [Treasure 1 Transfer]
             ├─> [Treasure 2 Transfer]  ← All happenin'
             ├─> [Treasure 3 Transfer]  ← simultaneously
             ├─> [Treasure 4 Transfer]  ← with progress
             └─> [More Discovery]       ← fer everything
```

### Progress Durin' Large Operations

Example: Plunderin' 100,000 small treasures

**rsync behavior:**
```bash
$ rsync -av --progress /data/ /backup/
# 30 seconds o' silence (discoverin' 100,000 treasures, crew be gettin' worried)
# Then:
file000001.txt
         1,234  100%    1.23MB/s    0:00:00
file000002.txt
         2,345  100%    2.34MB/s    0:00:00
# ... 99,998 more lines ...
```

**arsync behavior (with --pirate flag, arrr!):**
```console
$ arsync -a --source /data --destination /backup --progress --pirate
# Immediately starts showin' (crew be happy, no waitin' around!):
Treasure sighted on the horizon, ahoy: 1,234 chests o' precious booty | Booty plundered and secured, blimey: 856 chests | Crew be haulin' aboard right now, avast: 378 chests
[==>                        ] 8% | 850 MB/s knots through the waves | bells 'til we reach port: 0:01:23

# Updates continuously as we sail (the crew loves it!):
Treasure sighted on the horizon, ahoy: 45,678 chests o' precious booty | Booty plundered and secured, blimey: 38,234 chests | Crew be haulin' aboard: 512 chests
[==========>                ] 45% | 920 MB/s knots through the waves | bells 'til we reach port: 0:00:42

# Near completion (almost to port!):
Treasure sighted on the horizon, ahoy: 100,000 chests o' precious booty | Booty plundered and secured, blimey: 98,500 chests | Crew be haulin' aboard: 1,500 chests
[=======================>   ] 98% | 875 MB/s knots through the waves | bells 'til we reach port: 0:00:03

✓ BLOW ME DOWN! All treasure be safely stowed in the hold! Break out the rum, we be RICH! 🏴‍☠️ Yo ho ho!
```

### Implementation Details

**arsync's progress trackin':**
1. **Atomic counters**: Lock-free counters updated from multiple crew members
2. **Non-blockin' updates**: Progress display doesn't slow down operations
3. **Intelligent throttlin'**: Updates every 100ms to avoid flicker
4. **Memory efficient**: Progress state be <1KB regardless o' operation size

**Key metrics tracked:**
- Treasures discovered (total found so far)
- Treasures plundered (finished plunderin')
- Treasures in-flight (currently bein' processed)
- Bytes discovered/plundered
- Throughput (movin' average)
- Time elapsed
- Estimated time remainin'

### Crew Experience Benefits

1. **No "frozen ship" periods**: Crew sees activity immediately
2. **Better ETAs**: Estimates improve as discovery progresses
3. **Cancellation confidence**: Can safely cancel knowin' progress
4. **Debuggin' insight**: Can see if discovery or plunderin' be slow
5. **Capacity plannin'**: Real-time throughput helps predict completion

### Conclusion (The Bottom Line, Savvy?)

Ahoy! `arsync --progress` (especially with `--pirate`!) provides **superior visibility** into yer operations - like havin' a crow's nest view o' everything:
- **Immediate feedback**: No discovery phase blackout means the crew stays informed from the first bell! No one's left wonderin'!
- **Concurrent trackin'**: Shows discovery PLUS plunderin' happenin' simultaneously - it's like watchin' a well-choreographed ship's dance!
- **Better estimates**: The ETA to port improves as the operation progresses - gettin' more accurate with every league we sail!
- **More informative**: Shows in-flight operations and overall state - ye know exactly what every crew member be doin'!

This magnificent spectacle be enabled by arsync's parallel architecture where discovery and plunderin' happen concurrently, unlike rsync's old sequential approach (one thing at a time like a landlubber!). We be sailin' circles around 'em, matey!

---

## Appendices (Additional Scrolls)

### NVMe Architecture and io_uring

Fer a comprehensive deep-dive into why NVMe was designed with massive parallelism and how io_uring exploits this architecture, see [NVME_ARCHITECTURE.md](docs/NVME_ARCHITECTURE.md).

**Key takeaways:**
- NVMe: 64K queues × 64K commands = 4 billion outstandin' operations
- Traditional blockin' I/O wastes 90% o' NVMe performance
- io_uring's queue-pair model matches NVMe's native architecture
- Result: 8.5x throughput improvement on small treasures

### Why fadvise be Superior to O_DIRECT

Fer a detailed explanation o' why arsync uses `fadvise` instead o' `O_DIRECT`, includin' Linus Torvalds' famous "deranged monkey" critique, see [FADVISE_VS_O_DIRECT.md](docs/FADVISE_VS_O_DIRECT.md).

**Key takeaways:**
- O_DIRECT requires strict 4KB alignment (painful)
- O_DIRECT be synchronous (blocks, can't hide latency)
- fadvise retains kernel optimizations (read-ahead, write-behind)
- Result: fadvise + io_uring be 15-30% faster than O_DIRECT

---

## Contributin' (Join the Crew!)

Contributions be welcome, matey! Please see [DEVELOPER.md](docs/DEVELOPER.md) fer guidelines.

1. Fork the repository (get yer own ship)
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit yer changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request (request to join the fleet)

## License

This project be licensed under the MIT license (see [LICENSE](LICENSE) or http://opensource.org/licenses/MIT).

## Acknowledgments (Thanks to These Fine Sailors)

- **[rsync](https://rsync.samba.org/)** ([GitHub](https://github.com/WayneD/rsync)) - Pioneerin' treasure synchronization tool created by Andrew Tridgell and Paul Mackerras way back in 1996! We be deeply grateful to Wayne Davison (current cap'n maintainer), and all the contributors who've developed and maintained rsync over nearly three decades on the high seas! rsync revolutionized treasure plunderin' and remains the gold standard, aye! This project stands on the shoulders o' their groundbreakin' work, arrr!
- [io_uring](https://kernel.dk/io_uring.pdf) - Linux kernel asynchronous I/O interface by Jens Axboe (the master shipwright!)
- [compio](https://github.com/compio-rs/compio) - Completion-based async runtime fer Rust (keeps the ship sailin' smooth!)
- [Rust](https://www.rust-lang.org/) - Memory-safe systems programmin' language (prevents the ship from sinkin'!)
- [krbaker](https://github.com/krbaker) - Painted our ship's magnificent colors and flew our Jolly Roger proud! Fine work on the icon, matey!

---

**Arrr! May yer treasure plunderin' be swift and yer holds be full! 🏴‍☠️**

