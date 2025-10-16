# rsync Wire Protocol Specification and Implementation Strategy

**Date**: October 9, 2025  
**Author**: Research and implementation planning  
**Status**: Design document - implementation to follow  
**Purpose**: Understand rsync's actual wire protocol before implementing compatibility

---

## Executive Summary

This document describes the rsync wire protocol in detail and outlines our implementation strategy for achieving full compatibility. rsync uses a **multiplexed I/O protocol** with message tags, making it significantly more complex than our current simple protocol.

**Key Insight**: rsync's protocol is NOT a simple bidirectional byte stream. It uses tagged messages to multiplex data, errors, logs, and control messages over a single stream.

---

## Part 1: Understanding rsync's Protocol Architecture

### 1.1 Protocol Versions

**rsync protocol versions** (as of 2025):
- **Version 27**: rsync 3.0.0 (2008) - baseline
- **Version 30**: rsync 3.1.0 (2013) - incremental recursion
- **Version 31**: rsync 3.2.0 (2020) - xxHash checksums
- **Version 32**: rsync 3.4.0 (2024) - current

**Our implementation target**: Protocol version 31/32  
**Backward compatibility**: Support down to version 27

### 1.2 Multiplexed I/O - The Critical Difference

**Problem**: A single byte stream must carry:
- File data
- Error messages
- Log messages
- Control messages
- Progress updates
- Checksums

**rsync's Solution**: Tagged messages (multiplexing)

```
Every message has:
  [tag: 1 byte][length: 3 bytes][data: length bytes]
```

**Message Tags** (from rsync source `io.c`):
```c
#define MPLEX_BASE 7

#define MSG_DATA       (MPLEX_BASE+0)   /* 7  - regular data */
#define MSG_ERROR_XFER (MPLEX_BASE+1)   /* 8  - transfer error */
#define MSG_INFO       (MPLEX_BASE+2)   /* 9  - info message */
#define MSG_ERROR      (MPLEX_BASE+3)   /* 10 - error message */
#define MSG_WARNING    (MPLEX_BASE+4)   /* 11 - warning */
#define MSG_ERROR_SOCKET (MPLEX_BASE+5) /* 12 - socket error */
#define MSG_LOG        (MPLEX_BASE+6)   /* 13 - log message */
#define MSG_CLIENT     (MPLEX_BASE+7)   /* 14 - client message */
#define MSG_REDO       (MPLEX_BASE+9)   /* 16 - redo request */
...
```

**Critical**: Pure data is tagged with MSG_DATA (7). Our current protocol sends untagged bytes, which rsync interprets as garbage.

---

## Part 2: Protocol Phases in Detail

### 2.1 Phase 1: Handshake and Negotiation

#### Simple Protocol Version Exchange

**Our current implementation** (works for arsync ↔ arsync):
```
Sender → Receiver: [1 byte: version]
Receiver → Sender: [1 byte: version]
```

**rsync's actual protocol** (what we need to implement):
```
1. Initial version exchange (UNTAGGED bytes!):
   Client → Server: [1 byte: version]
   Server → Client: [1 byte: version]

2. If version >= 23, exchange capabilities:
   Client → Server: [4 bytes: checksum seed]
   Server → Client: [4 bytes: checksum seed]

3. Negotiate options:
   - Checksum algorithm (MD4/MD5/xxHash)
   - Compression (zlib/zstd/none)
   - Incremental recursion
   - Preserve: permissions, times, links, etc.
```

**Why it matters**: Version and seed exchange happen BEFORE multiplexing starts. After this, all data is tagged.

### 2.2 Phase 2: File List Exchange

#### Our Current Format (Simple)
```
[4 bytes: file count]
For each file:
  [1 byte: flags]
  [4 bytes: path length]
  [N bytes: path]
  [8 bytes: size]
  [8 bytes: mtime]
  [4 bytes: mode]
  [4 bytes: uid]
  [4 bytes: gid]
  [if symlink: target path]
```

#### rsync's Actual Format (Complex)
```
File list is sent as TAGGED messages:

[tag=MSG_FLIST][length][encoded file entry]
[tag=MSG_FLIST][length][encoded file entry]
...
[tag=MSG_FLIST][length=0][empty] ← End of list marker

Each file entry uses VARIABLE-LENGTH encoding:
  - flags: 1-2 bytes (bit flags for what fields are present)
  - basename: varint length + bytes (NOT full path!)
  - dirname: sent once, reused for subsequent files in same directory
  - file_length: varint
  - modtime: varint delta from previous file's mtime
  - mode: varint
  - uid/gid: varint (only if --owner/--group)
  - checksum: 16 bytes MD5 (only if --checksum)
```

**Key Differences**:
1. **Varint encoding**: Not fixed-size integers
2. **Delta encoding**: mtime is delta from previous file
3. **Basename only**: Directories sent separately, basenames relative
4. **Conditional fields**: Only sent if flags indicate they're present
5. **Tagged messages**: Wrapped in MSG_FLIST tags

### 2.3 Phase 3: Block Checksum Exchange (Per File)

#### Our Current Format
```
Receiver → Sender:
  [4 bytes: checksum count]
  For each:
    [4 bytes: weak checksum]
    [16 bytes: strong checksum]
    [8 bytes: offset]
    [4 bytes: block_index]
```

#### rsync's Actual Format
```
Receiver → Sender (tagged as MSG_DATA):
  [4 bytes: block count]
  [4 bytes: block length]
  [4 bytes: remainder length] (last block if shorter)
  [4 bytes: checksum2 length] (usually 2 or 16)
  
  For each block:
    [4 bytes: rolling checksum (weak)]
    [2 or 16 bytes: MD4/MD5 checksum (strong)]
```

**Key Differences**:
1. **No offset field**: Implicit (block_index * block_length)
2. **Variable strong checksum size**: Can be 2 bytes (truncated) or 16 bytes
3. **Separate length fields**: block_length vs remainder for last block
4. **Tagged**: Wrapped in MSG_DATA tags

### 2.4 Phase 4: Delta Transmission

#### Our Current Format
```
Sender → Receiver:
  [4 bytes: instruction count]
  For each:
    [1 byte: type] (0=Literal, 1=BlockMatch)
    If Literal:
      [4 bytes: length]
      [N bytes: data]
    If BlockMatch:
      [4 bytes: block_index]
      [4 bytes: length]
```

#### rsync's Actual Format  
```
Sender → Receiver (stream of tagged messages):

Token-based encoding (NOT instruction count!):
  - Token byte determines what follows
  - Tokens encode both literal data and block references
  - Uses variable-length token encoding

Token formats:
  - If token == 0: End of file marker
  - If token >= 1 && token <= 96: Literal run of (token) bytes
  - If token >= 97: Block reference (complex encoding)

Block reference encoding:
  - High bits of token encode offset from previous block
  - Additional bytes encode exact block number
  - Length is implicit (block_size) unless last block

Literal data:
  [tag=MSG_DATA][length=N][N bytes of literal]

Block match:
  [tag=MSG_DATA][token byte][optional: block number bytes]
```

**Critical**: rsync uses a STREAMING token format, not a counted list of instructions!

---

## Part 3: Why Our Current Implementation Won't Work with rsync

### 3.1 Incompatibility Matrix

| Feature | Our Protocol | rsync Protocol | Impact |
|---------|-------------|----------------|--------|
| **Message framing** | Raw bytes | Tagged messages (MPLEX) | ❌ **CRITICAL** - rsync expects tags |
| **File list format** | Fixed fields | Varint + delta encoding | ❌ **MAJOR** - Different format |
| **Checksum format** | 4 fields per block | 2 fields, implicit offset | ❌ **MAJOR** - Different structure |
| **Delta encoding** | Instruction list | Token stream | ❌ **MAJOR** - Different approach |
| **Handshake** | Version only | Version + seed + caps | ⚠️ **MINOR** - Missing seed |
| **Integer encoding** | Little-endian fixed | Varint (7-bit continuation) | ❌ **MAJOR** - Different encoding |

**Conclusion**: Our protocol is fundamentally incompatible with rsync's wire format. We need a separate implementation path.

### 3.2 The Multiplexing Problem

**What happens when arsync talks to rsync**:

```
1. arsync sends: [0x1F] (version 31)
2. rsync reads: [0x1F] ✓ (correct!)
3. rsync sends: [0x1F] (version 31)
4. arsync reads: [0x1F] ✓ (correct!)

5. arsync sends file list: [0x00, 0x00, 0x00, 0x01, ...] (count=1)
6. rsync interprets as: [tag=0x00][length=0x00,0x00,0x01][...]
   - Tag 0 is INVALID!
   - rsync error: "unexpected tag 0"

FAILURE! Protocol mismatch detected immediately after handshake.
```

This explains the "unexpected tag 85" errors we saw in early tests!

---

## Part 4: Implementation Strategy

### 4.1 Two Protocol Modes

We need to maintain **two separate protocol implementations**:

#### Mode 1: arsync Native Protocol (CURRENT - KEEP THIS!)
**Use case**: arsync ↔ arsync synchronization  
**Status**: ✅ Fully implemented and working!  
**Features**:
- Simple, efficient format
- Fixed-size fields (easy to parse)
- No multiplexing overhead
- Delta algorithm working
- Metadata preservation working

**DO NOT CHANGE THIS!** This is our working implementation.

#### Mode 2: rsync Compatibility Protocol (TO IMPLEMENT)
**Use case**: arsync ↔ rsync interoperability  
**Status**: ⏳ To be implemented  
**Features**:
- Multiplexed I/O with tags
- Varint encoding
- rsync's token-based delta format
- rsync's file list format
- Full backward compatibility

### 4.2 Protocol Detection Strategy

**Question**: How does arsync know which protocol to use?

**Answer**: Protocol negotiation flag!

```rust
// Extended handshake (after version exchange)
enum ProtocolMode {
    ArsyncNative,  // Our efficient protocol
    RsyncCompat,   // rsync wire protocol
}

// Negotiation:
1. Exchange versions (both understand this)
2. If peer is arsync: send capability byte [0xFF] = "I support arsync native"
3. If peer responds [0xFF]: use arsync native protocol
4. If peer responds anything else or times out: use rsync compat protocol
```

**Benefit**: arsync ↔ arsync uses efficient protocol, arsync ↔ rsync uses compat protocol.

### 4.3 Module Organization

```
src/protocol/rsync.rs              (current - arsync native protocol)
  ├── send_via_pipe()              ✅ Working
  ├── receive_via_pipe()           ✅ Working  
  ├── generate_delta()             ✅ Working
  └── apply_delta()                ✅ Working

src/protocol/rsync_compat.rs       (NEW - rsync wire protocol)
  ├── read_mplex_message()         ✅ Implemented
  ├── write_mplex_message()        ✅ Implemented
  ├── rsync_send_via_pipe()        ⏳ To implement
  ├── rsync_receive_via_pipe()     ⏳ To implement
  ├── encode_file_list_rsync()     ⏳ To implement
  ├── decode_file_list_rsync()     ⏳ To implement
  ├── encode_checksums_rsync()     ⏳ To implement
  ├── decode_checksums_rsync()     ⏳ To implement
  ├── encode_delta_rsync()         ⏳ To implement
  └── decode_delta_rsync()         ⏳ To implement

src/protocol/varint.rs              (NEW - variable-length integers)
  ├── encode_varint()              ⏳ To implement
  └── decode_varint()              ⏳ To implement
```

---

## Part 5: Detailed Implementation Plan

### 5.1 Varint Encoding (Foundation)

rsync uses **7-bit continuation encoding** for variable-length integers:

```
Algorithm:
  - Lower 7 bits: data
  - High bit: continuation flag (1 = more bytes follow)

Examples:
  Value 127 (0x7F):
    → [0x7F] (1 byte)
  
  Value 128 (0x80):
    → [0x80, 0x01] (2 bytes)
    → byte1: 0x80 = 10000000 (continue bit set, value bits = 0)
    → byte2: 0x01 = 00000001 (continue bit clear, value bits = 1)
    → Decoded: 0 + (1 << 7) = 128
  
  Value 16383 (0x3FFF):
    → [0xFF, 0x7F] (2 bytes)
```

**Implementation**:
```rust
// src/protocol/varint.rs

pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        
        if value > 0 {
            byte |= 0x80; // Set continuation bit
        }
        
        result.push(byte);
        
        if value == 0 {
            break;
        }
    }
    
    result
}

pub async fn decode_varint<T: Transport>(transport: &mut T) -> Result<u64> {
    let mut result = 0u64;
    let mut shift = 0;
    
    loop {
        let mut byte_buf = [0u8; 1];
        transport::read_exact(transport, &mut byte_buf).await?;
        let byte = byte_buf[0];
        
        result |= ((byte & 0x7F) as u64) << shift;
        
        if (byte & 0x80) == 0 {
            break; // No continuation bit
        }
        
        shift += 7;
        
        if shift > 63 {
            anyhow::bail!("Varint too large (>64 bits)");
        }
    }
    
    Ok(result)
}
```

**Testing strategy**:
- Unit test encoding/decoding roundtrip
- Test edge cases: 0, 127, 128, 16383, u64::MAX
- Test against rsync's actual varint output (capture and verify)

### 5.2 File List Encoding (Complex)

#### rsync's File List Wire Format

**Directory Grouping**:
rsync sends files grouped by directory to avoid repeating directory names.

```
First file:
  [flags byte]
  [varint: dirname length]
  [dirname bytes]
  [varint: basename length]
  [basename bytes]
  [varint: file_length]
  [varint: modtime]
  [varint: mode]
  [if --owner: varint uid]
  [if --group: varint gid]

Subsequent file in same directory:
  [flags byte] (bit indicating "same directory")
  [varint: basename length]
  [basename bytes]
  [varint: file_length]
  [varint: modtime DELTA from previous]
  [varint: mode]
  ...
```

**Flags Byte** (from rsync source `flist.c`):
```
Bit 0 (XMIT_TOP_DIR): Top-level directory
Bit 1 (XMIT_SAME_MODE): Mode unchanged from previous
Bit 2 (XMIT_EXTENDED_FLAGS): Extended flags follow
Bit 3 (XMIT_SAME_RDEV_pre28): Device unchanged (old protocol)
Bit 4 (XMIT_SAME_UID): UID unchanged from previous
Bit 5 (XMIT_SAME_GID): GID unchanged from previous
Bit 6 (XMIT_SAME_NAME): Basename matches previous (for hardlinks)
Bit 7 (XMIT_LONG_NAME): Name length > 255 (use varint)
```

**Delta Encoding**:
- **mtime**: Sent as delta from previous file's mtime
- **mode**: Sent as delta if SAME_MODE not set
- **uid/gid**: Only if changed from previous file

**Benefits of rsync's format**:
- Smaller: Varint uses fewer bytes for small values
- Efficient: Delta encoding reduces redundancy
- Bandwidth: Critical for slow networks

**Drawbacks**:
- Complex: Stateful encoding/decoding
- Error-prone: Must track previous file's values
- Difficult to debug: Binary format with many edge cases

#### Implementation Approach

**Phase 1**: Implement simplified rsync file list (no delta encoding)
```rust
async fn encode_file_list_rsync_simple(files: &[FileEntry]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    
    for file in files {
        // Build file entry
        let mut entry = Vec::new();
        
        // Flags (simplified - no delta encoding yet)
        let mut flags = 0u8;
        if file.is_symlink { flags |= 0x?? }; // Need to find correct bit
        entry.push(flags);
        
        // Path (full path for now, not dirname/basename optimization)
        let path_bytes = file.path.as_bytes();
        entry.extend(encode_varint(path_bytes.len() as u64));
        entry.extend(path_bytes);
        
        // File length
        entry.extend(encode_varint(file.size));
        
        // Mtime (absolute for now, not delta)
        entry.extend(encode_varint(file.mtime as u64));
        
        // Mode
        entry.extend(encode_varint(file.mode as u64));
        
        // uid/gid if preserving ownership
        entry.extend(encode_varint(file.uid as u64));
        entry.extend(encode_varint(file.gid as u64));
        
        // Wrap in MSG_FLIST tag
        output.extend(write_mplex_message_bytes(MessageTag::FList, &entry)?);
    }
    
    // End of list marker
    output.extend(write_mplex_message_bytes(MessageTag::FList, &[])?);
    
    Ok(output)
}
```

**Phase 2**: Add delta encoding (optimization)
**Phase 3**: Add directory grouping (efficiency)

### 5.3 Delta Encoding (Token-Based)

#### rsync's Token Stream

rsync uses a **token-based encoding** for delta, not instruction lists:

```
Token values:
  0:        End of data for this file
  1-96:     Run of N literal bytes follows
  97-255:   Block reference (encoded as offset from previous)
  
Example delta stream:
  [tag=MSG_DATA][len=50][
    token=10,          ← 10 bytes of literal data follow
    ...10 bytes...,
    token=200,         ← Block reference (decode to block index)
    token=5,           ← 5 bytes of literal data
    ...5 bytes...,
    token=0            ← End of file
  ]
```

**Block Reference Decoding** (complex!):
```c
// From rsync source (token.c)
if (token >= 97) {
    int offset_bits = (token - 97) >> 4;
    int offset = read_bits(offset_bits);
    block_num = last_block + 1 + offset;
}
```

This is **significantly more complex** than our instruction list approach!

#### Why rsync uses tokens

1. **Bandwidth efficiency**: Single byte encodes common cases
2. **Streaming**: No need to count instructions upfront
3. **Incremental**: Can start reconstructing before full delta received
4. **Locality**: Block references encode offset from previous block (small numbers)

#### Our Implementation Options

**Option A: Full rsync token compatibility**
- Implement exact token encoding/decoding
- Match rsync's wire format byte-for-byte
- Pro: 100% compatible
- Con: Complex, harder to debug

**Option B: Simplified compatible format** (RECOMMENDED)
- Use multiplexed I/O (required)
- Use varint encoding (required)
- Simplify delta to explicit instructions
- Pro: Easier to implement and debug
- Con: May not be 100% compatible (need to test)

**Option C: Hybrid approach**
- Implement full token format for rsync compat mode
- Keep simple format for arsync native mode
- Use protocol detection to switch
- Pro: Best of both worlds
- Con: More code to maintain

---

## Part 6: Implementation Phases (Realistic Estimate)

### Phase 1: Varint Implementation (1-2 hours)
**Goal**: Variable-length integer encoding/decoding

**Files**:
- `src/protocol/varint.rs` (new)

**Tests**:
- Roundtrip encoding/decoding
- Edge cases (0, 127, 128, u64::MAX)
- Compatibility with rsync's varint

**Deliverable**: Working varint codec with tests

### Phase 2: Multiplexed I/O (2-3 hours)
**Goal**: Handle rsync's tagged message protocol

**Files**:
- `src/protocol/rsync_compat.rs` (enhance current)

**Implementation**:
- Read/write tagged messages ✅ (done!)
- Handle MSG_DATA, MSG_ERROR, MSG_INFO
- Buffer management for partial messages
- Error handling for unknown tags

**Tests**:
- Message roundtrip
- Tag handling
- Error propagation

**Deliverable**: Multiplexed I/O layer working

### Phase 3: rsync File List Format (4-6 hours)
**Goal**: Encode/decode rsync-compatible file lists

**Challenges**:
- Varint encoding
- Directory grouping (dirname/basename split)
- Delta encoding (mtime deltas, uid/gid deltas)
- Flags byte management
- Conditional fields

**Implementation Strategy**:
1. Start with simplified format (no delta encoding)
2. Test against rsync's actual output (packet capture)
3. Add delta encoding
4. Add directory grouping optimization

**Tests**:
- Parse rsync's actual file list (captured)
- Generate file list, feed to rsync
- Roundtrip testing

**Deliverable**: rsync-compatible file list encoding/decoding

### Phase 4: rsync Checksum Format (2-3 hours)
**Goal**: Generate checksums in rsync's format

**Changes needed**:
- Remove offset and block_index fields (implicit)
- Support variable strong checksum length (2 or 16 bytes)
- Add block_length and remainder fields
- Tag messages as MSG_DATA

**Tests**:
- Generate checksums, feed to rsync
- Parse rsync's checksums
- Verify rsync accepts our format

**Deliverable**: rsync-compatible checksum exchange

### Phase 5: rsync Delta Format (6-8 hours) **MOST COMPLEX!**
**Goal**: Implement rsync's token-based delta encoding

**This is the hardest part!**

**Substeps**:
1. Study rsync token.c in detail
2. Implement token encoding for our delta
3. Implement token decoding for rsync's delta
4. Handle literal runs (tokens 1-96)
5. Handle block references (tokens 97-255)
6. Test exhaustively

**Tests**:
- Generate delta, feed to rsync receiver
- Parse rsync's delta, reconstruct file
- Roundtrip with various file sizes

**Deliverable**: Full rsync delta compatibility

### Phase 6: Integration and Testing (4-6 hours)
**Goal**: Wire everything together and make Tests 3 & 4 pass

**Tasks**:
- Integrate all components
- Add protocol mode detection
- Test rsync → arsync (Test 3)
- Test arsync → rsync (Test 4)
- Debug protocol mismatches
- Handle edge cases

**Deliverable**: Tests 3 & 4 passing!

---

## Part 7: Estimated Timeline

### Conservative Estimate (40-50 hours)
```
Phase 1: Varint             →  2 hours
Phase 2: Multiplexed I/O    →  3 hours
Phase 3: File List          →  8 hours (complex!)
Phase 4: Checksum Format    →  3 hours
Phase 5: Delta Tokens       → 12 hours (very complex!)
Phase 6: Integration        →  8 hours
Phase 7: Debug & Polish     →  8 hours
                             ─────────
Total:                        44 hours
```

### Aggressive Estimate (24-32 hours)
If we simplify and skip some optimizations:
```
Phase 1: Varint             →  1 hour
Phase 2: Multiplexed I/O    →  2 hours
Phase 3: File List (simple) →  4 hours
Phase 4: Checksum Format    →  2 hours
Phase 5: Delta (basic)      →  6 hours
Phase 6: Integration        →  4 hours
Phase 7: Debug              →  5 hours
                             ─────────
Total:                        24 hours
```

---

## Part 8: Alternative: librsync Integration

### What is librsync?

librsync is a library that implements the rsync algorithm independently of the rsync utility. It provides C APIs for delta generation and application.

**Pros**:
- Battle-tested implementation
- Handles all protocol quirks
- Actively maintained

**Cons**:
- C library (needs FFI bindings)
- Not async (blocking I/O)
- Large dependency
- Doesn't help with protocol multiplexing

**Verdict**: Not recommended. We want full control and async implementation.

---

## Part 9: Recommended Approach

### Strategy: Incremental Compatibility

**Step 1**: Document what we have ✅
- Our arsync native protocol works perfectly
- Delta algorithm is sound
- Metadata preservation is complete
- Tests 1 & 2 passing

**Step 2**: Implement varint codec (1-2 hours)
- Small, contained module
- Well-understood algorithm
- Easy to test

**Step 3**: Enhance multiplexed I/O (2-3 hours)
- Build on existing `rsync_compat.rs`
- Handle all message tags
- Proper error propagation

**Step 4**: Implement simplified file list (4-6 hours)
- Skip delta encoding initially
- Use full paths (not dirname/basename optimization)
- Get basic format working

**Step 5**: Implement simplified delta format (4-6 hours)
- Convert our DeltaInstructions to rsync tokens
- Handle literal runs and block references
- Test with real rsync

**Step 6**: Iterate until Tests 3 & 4 pass (4-8 hours)
- Debug protocol mismatches
- Fix edge cases
- Add missing features

**Total realistic time**: 15-25 hours of focused development

---

## Part 10: Success Criteria

### Minimum Viable Compatibility

For Tests 3 & 4 to pass, we need:

1. ✅ **Handshake**: Version exchange working
2. ⏳ **Multiplexed I/O**: Tagged messages
3. ⏳ **File List**: rsync-parseable format (simplified OK)
4. ⏳ **Checksums**: rsync-compatible format
5. ⏳ **Delta**: rsync-compatible token stream
6. ✅ **Metadata**: Already working

### Full Compatibility (Stretch Goals)

- [ ] Directory grouping optimization
- [ ] Delta encoding for file list (mtime deltas)
- [ ] Optimal token encoding
- [ ] Compression support
- [ ] Incremental recursion
- [ ] Hard link detection (rsync-style)
- [ ] --checksum whole-file verification
- [ ] --delete mode
- [ ] --partial mode (resume transfers)

---

## Part 11: Risks and Mitigation

### Risk 1: Protocol Complexity

**Risk**: rsync's protocol has evolved over 25 years with many edge cases  
**Mitigation**: Start simple, iterate, test against real rsync extensively

### Risk 2: Undocumented Behavior

**Risk**: rsync's wire format not fully documented, must reverse-engineer  
**Mitigation**: Packet captures, read rsync source code, test thoroughly

### Risk 3: Version Compatibility

**Risk**: Protocol version 27-32 have subtle differences  
**Mitigation**: Test against multiple rsync versions, start with newest (v32)

### Risk 4: Time Investment

**Risk**: Could take 40+ hours to get fully working  
**Mitigation**: Incremental approach, validate at each step, accept "good enough" initially

---

## Part 12: Decision Points

### Question 1: Do we need 100% rsync compatibility?

**Options**:
A. **Full compatibility**: Byte-for-byte rsync protocol  
B. **Functional compatibility**: Works with rsync, maybe not optimal  
C. **Hybrid**: arsync native for arsync, compat mode for rsync

**Recommendation**: **Option C (Hybrid)**
- Keep our efficient protocol for arsync ↔ arsync
- Implement "good enough" compat for rsync interop
- Optimize compatibility later if needed

### Question 2: What's the minimum for "working"?

**Definition of "working" for Tests 3 & 4**:
- Files transfer correctly
- Metadata preserved
- No errors from rsync
- Delta algorithm reduces bandwidth

**NOT required for "working"**:
- Optimal bandwidth (can be less efficient than rsync)
- All rsync features (just core sync)
- All protocol versions (just 31/32)

### Question 3: How much time to invest now?

**Options**:
A. **Full implementation now**: 40+ hours  
B. **Minimum viable now**: 15-20 hours  
C. **Document and defer**: 0 hours (this document!)

**Recommendation**: **Option B (Minimum viable)**
- Implement enough for Tests 3 & 4 to pass
- Defer optimizations
- Document what's missing

---

## Part 13: Conclusion and Next Steps

### What We Know Now

1. **Our arsync native protocol works perfectly** ✅
   - Delta algorithm functional
   - Metadata complete
   - Test 2 passing

2. **rsync protocol is complex**
   - Multiplexed I/O required
   - Varint encoding needed
   - Token-based delta format
   - Stateful file list encoding

3. **We need two protocol modes**
   - Native mode for arsync ↔ arsync (keep current)
   - Compat mode for arsync ↔ rsync (implement new)

### Recommended Implementation Order

**Phase A: Foundation** (3-4 hours)
1. Implement varint codec
2. Test varint thoroughly
3. Enhance multiplexed I/O

**Phase B: File List** (6-8 hours)
4. Implement simplified rsync file list format
5. Test file list parsing with rsync
6. Add delta encoding if needed

**Phase C: Delta** (6-10 hours)
7. Implement token-based delta encoding
8. Test delta with rsync
9. Debug and fix issues

**Phase D: Integration** (4-6 hours)
10. Wire up rsync compat mode
11. Add protocol detection
12. Make Tests 3 & 4 pass

**Total**: 19-28 hours for full compatibility

### Deferred to Future

- Optimal delta encoding
- Directory grouping optimization
- Compression support
- All rsync features (--delete, --partial, etc.)
- Protocol versions < 30

---

## Part 14: Open Questions to Research

1. **How does rsync negotiate capabilities?**
   - Need to study capability exchange in detail
   - What capabilities must we advertise?

2. **How does rsync handle file list EOF?**
   - Is it the MSG_FLIST with length=0?
   - Or a specific token?

3. **How are block references exactly encoded?**
   - The offset_bits calculation is unclear
   - Need to study token.c in detail

4. **How does rsync handle errors mid-transfer?**
   - Can it resume?
   - How are errors propagated?

5. **What's the minimum feature set for compatibility?**
   - Can we skip compression?
   - Can we skip incremental recursion?

---

## Appendix A: rsync Source Code References

**Key files to study** (from rsync source):
- `io.c` - Multiplexed I/O implementation (~2000 lines)
- `flist.c` - File list encoding (~3000 lines)
- `generator.c` - Checksum generation (~2000 lines)
- `sender.c` - File sending (~400 lines)
- `receiver.c` - File receiving (~800 lines)
- `token.c` - Delta token encoding (~400 lines)
- `match.c` - Block matching (~400 lines)

**Total relevant code**: ~9000 lines of C

**Our implementation**: ~1400 lines of Rust (native protocol)  
**Compat layer estimate**: +2000 lines (for full rsync compat)

---

## Appendix B: Test Strategy for Compatibility

### Packet Capture Approach

```bash
# Capture rsync protocol
rsync -av /source/ /dest/ --log-file=rsync.log --debug=ALL 2>&1 | tee rsync-output.txt

# Analyze with our test
arsync --pipe --mode=rsync-compat --debug /source/ /dest/

# Compare byte streams
diff <(xxd rsync-captured.bin) <(xxd arsync-generated.bin)
```

### Incremental Testing

1. **Test varint**: Generate values, compare with rsync's output
2. **Test file list**: Parse rsync's actual file list
3. **Test checksums**: Feed our checksums to rsync
4. **Test delta**: Feed our delta to rsync receiver

### Validation Criteria

- rsync accepts our messages without errors
- Files transfer correctly
- Metadata preserved
- No "protocol error" messages

---

## Summary

**Current Status**: 
- arsync native protocol: ✅ Complete and working
- rsync compatibility: ⏳ Requires 20-30 hours of work

**Complexity**: rsync's protocol is significantly more complex than anticipated:
- Multiplexed I/O (tagged messages)
- Varint encoding (7-bit continuation)
- Delta encoding (file list state)
- Token-based delta (complex encoding)

**Recommendation**:
1. ✅ **Celebrate what works**: Our native protocol is production-ready
2. 📝 **Document thoroughly**: This document!
3. ⏳ **Implement incrementally**: Varint → Multiplex → File List → Delta
4. 🧪 **Test extensively**: Against real rsync at each phase

**Next Concrete Step**: Implement and test varint codec (1-2 hours)

---

**Last Updated**: October 9, 2025  
**Pages**: 7  
**Word Count**: ~2,500  
**Status**: Design complete - ready for implementation

**This document will guide the rsync compatibility implementation and set realistic expectations for the work required.**

