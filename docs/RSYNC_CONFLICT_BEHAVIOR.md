# rsync Behavior When Destination Exists

**Date:** 2025-10-20  
**Purpose:** Document rsync's behavior for type conflicts and re-sync scenarios  
**rsync version tested:** 3.4.1

## Summary

**Type conflicts:** rsync **ALWAYS FAILS** - no option to override  
**Same type:** rsync succeeds and **ALWAYS updates metadata** on existing directories

## Test Results

### 1. Directory → Non-Directory (CONFLICT)

```bash
# Source is directory, destination is a file
mkdir src
echo "file" > dst
rsync -av src dst

# Result: ERROR
ERROR: cannot overwrite non-directory with a directory
rsync error: errors selecting input/output files, dirs (code 3)
```

**Behavior:** rsync **FAILS** if destination exists as a file/symlink when source is a directory

### 2. File → Directory (NO CONFLICT)

```bash
# Source is file, destination is a directory  
echo "file" > src
mkdir dst
rsync -av src dst

# Result: SUCCESS (file goes INSIDE directory)
sending incremental file list
src
# Creates: dst/src (file inside directory)
```

**Behavior:** rsync **SUCCEEDS** - copies file INTO the existing directory

### 3. Directory → Directory (RE-SYNC)

```bash
# Both are directories
mkdir src dst
chmod 755 src
chmod 700 dst
rsync -av src/ dst/

# Result: SUCCESS, metadata UPDATED
stat -c '%a' dst  # Shows: 755 (updated to match source!)
```

**Behavior:** rsync **ALWAYS updates metadata** on existing directories during re-sync

### 4. File → File (OVERWRITE)

```bash
# Both are files
echo "old" > dst/file.txt
echo "new" > src/file.txt
rsync -av src/file.txt dst/

# Result: SUCCESS (overwrites)
```

**Behavior:** rsync **OVERWRITES** existing file with same name

### 5. Symlink → File or Directory (DEPENDS)

```bash
# Source is symlink
ln -s target src_link
echo "file" > dst_link
rsync -av src_link dst_link

# With --links (-l): Replaces dst_link with symlink
# Without --links: Follows symlink and copies target content
```

**Behavior:** Depends on `--links` flag

## Options That DON'T Help

### `--force`
```bash
man rsync | grep -A 5 "^       --force"
# Output: "delete a non-empty directory when it is to be replaced by a non-directory"
```

**Does NOT help with:** directory → non-directory (our case)  
**Only handles:** non-directory → directory (removing non-empty dir)

### `--delete`
```bash
rsync -av --delete src/ dst/  # dst is a file
# Result: ERROR - "cannot stat destination: Not a directory"
```

**Does NOT help:** Type conflicts still fail

### Conclusion

**rsync has NO options to handle type conflicts** - it always fails with an error.  
**This is by design** - type conflicts indicate user error or filesystem corruption.

## arsync Implementation Requirements

### Type Conflict Detection

```rust
match compio::fs::create_dir(&dst_path).await {
    Ok(()) => {
        // Created successfully
    }
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
        // Verify it's actually a directory
        let metadata = compio::fs::metadata(&dst_path).await?;
        if !metadata.is_dir() {
            return Err(SyncError::FileSystem(format!(
                "Cannot create directory {}: path exists but is not a directory",
                dst_path.display()
            )));
        }
        // It's a directory - continue
    }
    Err(e) => return Err(...),
}
```

### Metadata Preservation

**ALWAYS preserve metadata on directories:**
- New directories: Set metadata after creation
- Existing directories: Update metadata to match source (re-sync)
- This matches rsync's behavior of updating permissions/ownership on re-sync

```rust
// ALWAYS preserve, whether created or existed
preserve_directory_metadata_fd(&dst_dir_fd, metadata, config).await?;
```

### Error Messages

Match rsync's error format:
```
ERROR: cannot overwrite non-directory with a directory
```

## Test Cases

### Implemented Tests (tests/directory_resync_tests.rs)

1. ✅ **`test_resync_updates_directory_metadata`** - Re-sync existing directory → update metadata
2. ✅ **`test_type_conflict_file_to_directory_fails`** - Directory exists as file → FAIL
3. ✅ **`test_file_into_existing_directory_creates_nested`** - File → directory nesting
4. ✅ **`test_resync_preserves_directory_timestamps`** - Re-sync updates timestamps

### Additional Cases Needed

5. 🔲 **Directory exists as symlink** → FAIL with rsync-compatible error
6. 🔲 **Create new directory** → preserve metadata (basic case)
7. 🔲 **File overwrites file** → preserve metadata

## References

- **rsync source:** `main.c:766` - "cannot overwrite non-directory with a directory"
- **rsync source:** `main.c:772` - "cannot stat destination: Not a directory"
- **rsync manpage:** `--force` only handles non-directory → directory (opposite of our case)
- **rsync version tested:** 3.4.1
- **Test suite:** `tests/directory_resync_tests.rs`
- **Implementation:** `src/directory.rs:470-527` (type checking + metadata preservation)

