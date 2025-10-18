# Syscall Trace Filtering for arsync

This document describes techniques for filtering `strace` output to see only relevant operations, excluding program initialization noise.

## Problem

When running `strace` on `arsync`, you get thousands of syscalls related to:
- Dynamic linker loading libraries
- Runtime initialization
- `io_uring` setup
- Thread creation
- Signal handling setup

This makes it hard to see the actual file operations.

## Solutions

### 1. Filter by File Path (Best for single file)

Use `strace -P` to trace only operations on specific files:

```bash
strace -f -P /path/to/source/file -P /path/to/dest/file \
    ./target/release/arsync /path/to/source/file /path/to/dest/file -a
```

**Output**: Only syscalls involving those specific file paths (8-15 syscalls typically)

**Example**:
```
statx(AT_FDCWD, "/tmp/src/file.bin", ...) = 0
openat(AT_FDCWD, "/tmp/src/file.bin", O_RDONLY) = 7
openat(AT_FDCWD, "/tmp/dst/file.bin", O_WRONLY|O_CREAT|O_TRUNC) = 8
io_uring_enter(6, 2, 1, ...) = 2          # read
io_uring_enter(6, 1, 1, ...) = 1          # write
fchmod(8, 0644) = 0
fchown(8, 1000, 1000) = 0
utimensat(8, NULL, [...]) = 0              # FD-based timestamp
close(8) = 0
close(7) = 0
```

### 2. Filter from First Directory Read (Best for directories)

Use `benchmarks/trace_from_getdents.sh` to start tracing from the first directory traversal:

```bash
./benchmarks/trace_from_getdents.sh /path/to/src /path/to/dst -a
```

This script:
1. Runs full trace to `/tmp/full-trace-temp.txt`
2. Finds the first `getdents64` syscall (directory read)
3. Extracts everything from that point onward
4. Saves to `/tmp/work-only-trace.txt`

**Statistics**: Typically removes ~42% of initialization noise

**Example output**:
```
✅ Filtered trace written to: /tmp/work-only-trace.txt

📊 Statistics:
  - Initialization syscalls: 3056 (skipped)
  - Work syscalls: 4092 (captured)
  - Reduction: 42% of noise removed
```

### 3. Full Trace with Timestamps

For comprehensive analysis with timing information:

```bash
strace -f -tt -T -o /tmp/full-trace.txt \
    ./target/release/arsync /src /dst -a
```

Flags:
- `-f`: Follow forks/threads
- `-tt`: Timestamps with microseconds
- `-T`: Show syscall duration
- `-o`: Write to file (stderr stays clean)

Then grep for specific patterns:
```bash
# FD-based metadata operations
grep -E "(fchmod|fchown|futimens|utimensat.*NULL)" /tmp/full-trace.txt

# I/O operations
grep -E "(io_uring_enter|openat|read|write)" /tmp/full-trace.txt

# Thread creation
grep "clone3" /tmp/full-trace.txt
```

## Verification Examples

### Verify FD-based Metadata Operations

```bash
# Should see ONLY FD-based syscalls (no paths):
strace -e trace=chmod,chown,utimensat -f -o /tmp/metadata.txt \
    ./target/release/arsync /src /dst -a 2>&1

grep utimensat /tmp/metadata.txt
# Expected: utimensat(FD, NULL, ...) ✅ FD-based
# Bad:      utimensat(AT_FDCWD, "/path", ...) ❌ PATH-based
```

### Verify io_uring Usage

```bash
strace -e trace=io_uring_setup,io_uring_enter -f \
    ./target/release/arsync /src /dst -a 2>&1 | head -20
```

Should show:
- `io_uring_setup()` calls (one per worker thread + main thread)
- Many `io_uring_enter()` calls for actual I/O

### Verify Thread Count

```bash
strace -e trace=clone3 -f ./target/release/arsync /src /dst -a 2>&1 | wc -l
```

Should show number of worker threads created.

## Key Syscalls in arsync

### Per Directory:
- `openat(AT_FDCWD, "/path/to/dir", O_DIRECTORY)` - Open directory
- `getdents64(fd, ...)` - Read directory entries
- `statx(AT_FDCWD, "/path/to/entry", ...)` - Get entry metadata
- `fchmod(dir_fd, mode)` - Set directory permissions
- `fchown(dir_fd, uid, gid)` - Set directory ownership
- `utimensat(dir_fd, NULL, times)` - Set directory timestamps (FD-based!)

### Per File:
- `statx(AT_FDCWD, "file", ...)` - Get source metadata (x3-4 times)
- `openat(AT_FDCWD, "src_file", O_RDONLY)` - Open source
- `openat(AT_FDCWD, "dst_file", O_WRONLY|O_CREAT)` - Open/create dest
- `fallocate(dst_fd, 0, 0, size)` - Pre-allocate space
- `io_uring_enter(...)` - Async read/write operations
- `fsync(dst_fd)` - Flush to disk
- `fchmod(dst_fd, mode)` - Set permissions (FD-based)
- `fchown(dst_fd, uid, gid)` - Set ownership (FD-based)
- `fgetxattr(src_fd, ...)` / `fsetxattr(dst_fd, ...)` - Extended attrs (FD-based)
- `utimensat(dst_fd, NULL, times)` - Set timestamps (FD-based!)
- `close(dst_fd)` / `close(src_fd)`

### Security Note: 100% FD-based Metadata

All metadata operations use **file descriptors**, not paths:
- ✅ `fchmod(fd, mode)` - Not `chmod(path, mode)`
- ✅ `fchown(fd, uid, gid)` - Not `chown(path, uid, gid)`
- ✅ `utimensat(fd, NULL, times)` - Not `utimensat(AT_FDCWD, path, times)`
- ✅ `fgetxattr(fd, ...)` - Not `getxattr(path, ...)`

This eliminates TOCTOU (Time-of-Check-Time-of-Use) vulnerabilities where a malicious actor could replace a file with a symlink between stat and metadata operations.

## References

- `man 1 strace` - strace manual
- `man 2 futimens` - FD-based timestamp syscall
- `man 2 utimensat` - Can be FD-based when path is NULL
- `benchmarks/trace_from_getdents.sh` - Auto-filtering script

