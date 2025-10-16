# Positional Arguments Enhancement

**Goal**: Make arsync work like rsync with positional arguments instead of requiring flags.

## Current vs Proposed

### Current (Requires Flags)
```bash
arsync --source /data --destination /backup --archive
arsync --source /data --destination /backup -a
```

### Proposed (Positional Like rsync)
```bash
arsync /data /backup --archive
arsync /data /backup -a

# Still support flags for clarity/scripts
arsync --source /data --destination /backup -a
```

This matches rsync's behavior:
```bash
rsync -av /source /destination
```

---

## Implementation with Clap

### Option 1: Pure Positional (Recommended)

```rust
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,
    
    #[command(flatten)]
    pub io: IoConfig,
    
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    #[command(flatten)]
    pub output: OutputConfig,
}
```

**Usage:**
```bash
arsync /data /backup -a
arsync /data /backup --archive --queue-depth 8192
```

**Help text:**
```
Usage: arsync [OPTIONS] <SOURCE> <DESTINATION>

Arguments:
  <SOURCE>       Source directory or file
  <DESTINATION>  Destination directory or file

Options:
  -a, --archive              Archive mode
      --queue-depth <SIZE>   Queue depth [default: 4096]
  -v, --verbose             Verbose output
  -h, --help                Print help
```

---

### Option 2: Positional OR Flags (Maximum Compatibility)

```rust
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: Option<PathBuf>,
    
    /// Destination directory or file  
    #[arg(value_name = "DESTINATION")]
    pub destination: Option<PathBuf>,
    
    /// Source directory or file (alternative to positional)
    #[arg(short = 's', long, conflicts_with = "source")]
    pub source_flag: Option<PathBuf>,
    
    /// Destination directory or file (alternative to positional)
    #[arg(short = 'd', long, conflicts_with = "destination")]
    pub dest_flag: Option<PathBuf>,
    
    #[command(flatten)]
    pub io: IoConfig,
    
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    #[command(flatten)]
    pub output: OutputConfig,
}

impl Args {
    /// Get the actual source path (from positional or flag)
    pub fn get_source(&self) -> Result<PathBuf> {
        self.source
            .clone()
            .or_else(|| self.source_flag.clone())
            .ok_or_else(|| anyhow::anyhow!("Source is required"))
    }
    
    /// Get the actual destination path (from positional or flag)
    pub fn get_destination(&self) -> Result<PathBuf> {
        self.destination
            .clone()
            .or_else(|| self.dest_flag.clone())
            .ok_or_else(|| anyhow::anyhow!("Destination is required"))
    }
}
```

**Usage (both work):**
```bash
# Positional (modern, concise)
arsync /data /backup -a

# Flags (explicit, good for scripts)
arsync --source /data --destination /backup -a
arsync -s /data -d /backup -a
```

---

### Option 3: Positional with Optional Trailing (Like rsync)

Rsync supports multiple sources:
```bash
rsync file1 file2 dir1/ destination/
```

For arsync (if we want this later):
```rust
#[derive(Parser, Debug)]
pub struct Args {
    /// Source files/directories (last argument is destination)
    #[arg(value_name = "PATHS", num_args = 2..)]
    pub paths: Vec<PathBuf>,
    
    #[command(flatten)]
    pub io: IoConfig,
    
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    #[command(flatten)]
    pub output: OutputConfig,
}

impl Args {
    pub fn get_source_and_dest(&self) -> Result<(Vec<PathBuf>, PathBuf)> {
        if self.paths.len() < 2 {
            anyhow::bail!("At least source and destination required");
        }
        
        let dest = self.paths.last().unwrap().clone();
        let sources = self.paths[..self.paths.len()-1].to_vec();
        
        Ok((sources, dest))
    }
}
```

**Usage:**
```bash
arsync file1 file2 dir1/ /backup/
```

---

## Recommendation: Option 1 (Pure Positional)

**Why:**
1. ✅ **Matches rsync UX** - Familiar to users
2. ✅ **Simpler code** - No dual-path logic
3. ✅ **Cleaner help** - Less confusing
4. ✅ **Shorter commands** - Better UX
5. ✅ **Industry standard** - cp, mv, rsync all use positionals

**Breaking change?** 
- Yes, but early enough in development to do it
- Can announce in CHANGELOG
- Old scripts need update (but simpler)

---

## Migration Path

### Phase 1: Add Positional (Keep Flags Deprecated)

```rust
#[derive(Parser, Debug)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: Option<PathBuf>,
    
    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub dest_positional: Option<PathBuf>,
    
    /// Source (deprecated: use positional argument)
    #[arg(short, long, hide = true)]
    pub source_flag: Option<PathBuf>,
    
    /// Destination (deprecated: use positional argument)
    #[arg(short, long, hide = true)]
    pub destination: Option<PathBuf>,
    
    // ...
}

impl Args {
    pub fn validate(&self) -> Result<()> {
        // Warn on deprecated usage
        if self.source_flag.is_some() || self.destination.is_some() {
            eprintln!("Warning: --source and --destination flags are deprecated.");
            eprintln!("Use: arsync <SOURCE> <DESTINATION> [OPTIONS]");
        }
        
        // Require one or the other
        let source = self.source.clone()
            .or_else(|| self.source_flag.clone())
            .ok_or_else(|| anyhow::anyhow!("Source required"))?;
            
        let dest = self.dest_positional.clone()
            .or_else(|| self.destination.clone())
            .ok_or_else(|| anyhow::anyhow!("Destination required"))?;
        
        // Continue validation...
    }
}
```

### Phase 2: Remove Deprecated Flags (Next Major Version)

```rust
#[derive(Parser, Debug)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,
    
    // Flags removed - positional only
}
```

---

## Comparison with Similar Tools

### rsync (The Standard)
```bash
rsync [OPTIONS] SOURCE DEST
rsync -av /data /backup
```

### cp (POSIX Standard)
```bash
cp [OPTIONS] SOURCE DEST
cp -r /data /backup
```

### rclone (Modern Cloud Sync)
```bash
rclone [OPTIONS] SOURCE DEST
rclone sync /data remote:backup
```

### cargo (Rust Tool)
```bash
cargo build                    # No positionals for build
cargo install PACKAGE          # Positional for package name
cargo add DEPENDENCY           # Positional for dependency
```

### fd (Modern Find)
```bash
fd [OPTIONS] PATTERN [PATH]
fd test                        # Positional pattern
fd test /src                   # Positional pattern and path
```

**Pattern**: File operation tools (cp, mv, rsync, rclone) use positional args. Command-based tools (cargo, git) use subcommands + flags.

**arsync should follow**: cp/rsync pattern (file operations) → **Positional arguments**

---

## Updated Help Text

### Before (Current)
```
USAGE:
    arsync --source <SOURCE> --destination <DESTINATION> [OPTIONS]

OPTIONS:
    -s, --source <SOURCE>              Source directory or file
    -d, --destination <DESTINATION>    Destination directory or file
    -a, --archive                      Archive mode
        --queue-depth <SIZE>           Queue depth [default: 4096]
    -v, --verbose                      Verbose output
    -h, --help                         Print help
```

### After (Proposed)
```
USAGE:
    arsync [OPTIONS] <SOURCE> <DESTINATION>

ARGUMENTS:
    <SOURCE>       Source directory or file
    <DESTINATION>  Destination directory or file

OPTIONS:
    -a, --archive              Archive mode; same as -rlptgoD
    -r, --recursive            Recurse into directories
    -p, --perms                Preserve permissions
    -t, --times                Preserve modification times
    -X, --xattrs               Preserve extended attributes
        
        --queue-depth <SIZE>   Queue depth for io_uring [default: 4096]
        --cpu-count <COUNT>    Number of CPU cores (0=auto) [default: 0]
    
    -v, --verbose              Verbose output (-v, -vv, -vvv)
    -q, --quiet                Quiet mode
        --progress             Show progress
        --dry-run              Show what would be copied
    
    -h, --help                 Print help
    -V, --version              Print version
```

Much cleaner and more rsync-like!

---

## Code Changes Needed

### In `src/cli/mod.rs` (or current `src/cli.rs`)

#### Before
```rust
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Source directory or file
    #[arg(short, long)]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(short, long)]
    pub destination: PathBuf,
    
    // ... rest
}
```

#### After
```rust
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE", help_heading = "Arguments")]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(value_name = "DESTINATION", help_heading = "Arguments")]
    pub destination: PathBuf,
    
    #[command(flatten)]
    pub io: IoConfig,
    
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    #[command(flatten)]
    pub output: OutputConfig,
}
```

**That's it!** Remove `short` and `long` attributes, add `value_name`. Clap automatically makes them positional.

---

## Testing Examples

### Unit Test
```rust
#[test]
fn test_positional_args() {
    let args = Args::parse_from([
        "arsync",
        "/source/path",
        "/dest/path",
        "--archive",
    ]);
    
    assert_eq!(args.source, PathBuf::from("/source/path"));
    assert_eq!(args.destination, PathBuf::from("/dest/path"));
    assert!(args.metadata.archive);
}

#[test]
fn test_flags_mixed_with_positionals() {
    let args = Args::parse_from([
        "arsync",
        "/source",
        "/dest",
        "--queue-depth", "8192",
        "-v",
    ]);
    
    assert_eq!(args.source, PathBuf::from("/source"));
    assert_eq!(args.destination, PathBuf::from("/dest"));
    assert_eq!(args.io.queue_depth, 8192);
    assert_eq!(args.output.verbose, 1);
}

#[test]
fn test_order_independent() {
    // Options can come before or after positionals
    let args1 = Args::parse_from([
        "arsync", "-a", "/src", "/dst",
    ]);
    
    let args2 = Args::parse_from([
        "arsync", "/src", "/dst", "-a",
    ]);
    
    assert_eq!(args1.source, args2.source);
    assert_eq!(args1.destination, args2.destination);
    assert_eq!(args1.metadata.archive, args2.metadata.archive);
}
```

### Integration Test
```bash
# Test positional args work
./target/release/arsync /tmp/src /tmp/dst -a

# Test with options before
./target/release/arsync -a --queue-depth 8192 /tmp/src /tmp/dst

# Test with options after
./target/release/arsync /tmp/src /tmp/dst -a --queue-depth 8192

# Test verbose
./target/release/arsync /tmp/src /tmp/dst -av

# All should work identically to:
./target/release/arsync --source /tmp/src --destination /tmp/dst -a
```

---

## Real-World Examples

### rsync
```bash
# Basic
rsync -av /source /dest

# With options
rsync -av --delete --exclude="*.log" /source /dest

# Remote
rsync -av /local user@remote:/remote
```

### arsync (Proposed)
```bash
# Basic
arsync -a /source /dest

# With io_uring options
arsync -a --queue-depth 8192 /source /dest

# With progress
arsync -a --progress /source /dest

# All options
arsync -a --progress --queue-depth 8192 --cpu-count 16 /source /dest
```

---

## Edge Cases to Handle

### 1. Paths Starting with Dash
```bash
# This could be ambiguous
arsync -a /source/-weirdname

# Solution: Use -- to separate
arsync -a -- /source/-weirdname /dest/-weirdname
```

Clap handles this automatically with `--` separator.

### 2. Missing Arguments
```bash
# Error: missing destination
arsync -a /source

# Clap will show:
error: the following required arguments were not provided:
  <DESTINATION>
```

### 3. Too Many Arguments
```bash
# Error: too many positionals
arsync /src /dst /extra

# Clap will show:
error: unexpected argument '/extra' found
```

Unless we implement multi-source support (Option 3).

---

## Backwards Compatibility

### Option A: Breaking Change (Clean Slate)

**Announce in v1.0**:
```
BREAKING CHANGE: Source and destination are now positional arguments

Before: arsync --source /data --destination /backup -a
After:  arsync /data /backup -a

This matches rsync and other UNIX tools. Update your scripts accordingly.
```

### Option B: Deprecation Period

**v0.9**: Add positional, deprecate flags
```rust
#[arg(short, long, hide = true)]  // Hide but still works
pub source_flag: Option<PathBuf>,
```

**v1.0**: Remove flags entirely

### Option C: Support Both Forever

Keep both working:
```rust
pub fn get_source(&self) -> PathBuf {
    self.source.clone()
        .or_else(|| self.source_flag.clone())
        .expect("Source required")
}
```

**Recommendation**: Option B (Deprecation Period)
- v0.9: Add deprecation warnings
- v1.0: Remove old flags
- Gives users time to migrate

---

## Documentation Updates

### README.md
```markdown
## Usage

Basic usage (like rsync):
```bash
arsync [OPTIONS] SOURCE DESTINATION
```

Examples:
```bash
# Archive mode (recursive, preserve permissions, times, etc.)
arsync -a /data /backup

# With progress
arsync -a --progress /data /backup

# Tune io_uring performance
arsync -a --queue-depth 8192 --cpu-count 16 /data /backup
```

### Man Page / --help
Update to show positional arguments first, followed by options grouped by category:
- File Selection (--recursive, --links)
- Metadata (--perms, --times, --owner, --group, --xattrs)
- I/O Performance (--queue-depth, --cpu-count, --buffer-size)
- Output (--progress, --verbose, --quiet)
```

---

## Implementation Checklist

- [ ] Update `Args` struct to use positional arguments
  - [ ] Remove `short` and `long` from source
  - [ ] Remove `short` and `long` from destination  
  - [ ] Add `value_name` attributes
  
- [ ] Update validation
  - [ ] Ensure positional args are validated
  - [ ] Update error messages
  
- [ ] Update tests
  - [ ] Test positional parsing
  - [ ] Test option ordering flexibility
  - [ ] Test edge cases (missing args, too many args)
  - [ ] Update integration tests
  
- [ ] Update documentation
  - [ ] README.md usage examples
  - [ ] CHANGELOG.md breaking change notice
  - [ ] Help text verification
  - [ ] Man page (if exists)
  
- [ ] Benchmark scripts
  - [ ] Update benchmark scripts to use positional args
  - [ ] Update test scripts
  
- [ ] Consider multi-source support (future)
  - [ ] `arsync file1 file2 dir/ /backup/`
  - [ ] Like rsync's multiple sources

---

## Combined with CLI Refactoring

This enhancement fits perfectly with the modular CLI refactor:

```rust
// src/cli/mod.rs
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Source directory or file
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,
    
    /// Destination directory or file
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,
    
    /// I/O and performance configuration
    #[command(flatten)]
    pub io: IoConfig,
    
    /// Metadata preservation options
    #[command(flatten)]
    pub metadata: MetadataConfig,
    
    /// Output and verbosity control
    #[command(flatten)]
    pub output: OutputConfig,
}
```

**Do both refactors together:**
1. Modularize with flatten (maintainability)
2. Add positional args (UX improvement)

---

## Timeline

### As Part of CLI Refactor (Recommended)

**Combined effort: ~7 hours total**

1. Module refactor (5-6 hours)
2. Positional args (1 hour)
3. Testing both (30 min)

**Why together:**
- Only touch CLI once
- Single migration
- One breaking change announcement
- Tests updated once

### Separate (Alternative)

**If done separately:**
- Now: Module refactor (6 hours)
- v0.9: Add positional (1 hour)
- v1.0: Remove flags

---

## Recommendation

### ✅ Include in CLI Refactor

**Add to the refactoring plan:**

**Phase 3.5: Convert to Positional Arguments** (30 min)
- Remove `short` and `long` from source/destination
- Add `value_name` attributes
- Update tests
- Update help text

**Phase 5: Update Documentation** (add 30 min)
- Update README with new usage
- Add CHANGELOG entry
- Update benchmark scripts

**Total additional time**: ~1 hour

**Combined refactor**: ~7 hours (instead of 6)

---

## Conclusion

**Making source/destination positional is the right UX decision.**

✅ Matches rsync and UNIX tools  
✅ Shorter commands  
✅ More intuitive  
✅ Easy to implement with clap  
✅ Can be done alongside modular refactor  

**Recommended approach:**
1. Include in CLI modular refactor
2. Use pure positional (Option 1)
3. Announce as breaking change in v1.0
4. Update all docs and tests

This will make arsync feel like a modern, native replacement for rsync.

