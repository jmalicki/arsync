# PHASE 6: Delta Token Handling ✅ COMPLETE

**Goal**: Implement rsync token stream format for delta transfer

**Commit**: 6e933e9

## Phase 6.1: Implement Token Encoding/Decoding ✅ COMPLETE

### Add to `src/protocol/rsync_compat.rs`

- [x] Reused existing `DeltaInstruction` enum from rsync.rs:
  ```rust
  pub enum DeltaInstruction {
      Literal(Vec<u8>),  // Raw data to insert
      BlockMatch { block_index: u32, length: u32 },  // Copy from basis
  }
  ```

- [x] Implemented `delta_to_tokens(delta) -> Vec<u8>` (85 lines):
  - [x] Initialize last_block_index = -1 (i32 for offset calculation)
  - [x] For each DeltaInstruction:
    - [x] **Literal instruction**:
      - [x] Split into 96-byte chunks (rsync max literal size)
      - [x] For each chunk:
        - [x] Token byte = chunk.len() (1-96)
        - [x] Append chunk bytes
      - [x] Handle partial chunks correctly
      - [x] Preserve all literal data
    - [x] **BlockMatch instruction**:
      - [x] Calculate offset = block_index - last_block_index - 1
      - [x] **Simple offset** (0-15):
        - [x] Token = 97 + offset (tokens 97-112)
        - [x] No extra bytes
      - [x] **Complex offset** (>=16):
        - [x] Calculate bit_count (bits needed for offset)
        - [x] Token = 97 + (bit_count << 4) (tokens 113-255)
        - [x] Append offset bytes (little-endian)
      - [x] Update last_block_index = block_index
  - [x] Append end marker (token 0)
  - [x] Return complete token stream as Vec<u8>

- [x] Implemented `tokens_to_delta(tokens, checksums) -> Vec<DeltaInstruction>` (120 lines):
  - [x] Initialize last_block_index = -1
  - [x] Parse token stream byte by byte
  - [x] **Token 0**: End of data, break loop
  - [x] **Tokens 1-96**: Literal run
    - [x] Read next `token` bytes from stream
    - [x] Create Literal(data) instruction
  - [x] **Tokens 97-112**: Simple block match
    - [x] offset = token - 97 (0-15)
    - [x] block_index = last_block_index + offset + 1
    - [x] Create BlockMatch instruction
    - [x] Update last_block_index
  - [x] **Tokens 113-255**: Complex block match
    - [x] Extract bit_count: (token - 97) >> 4
    - [x] Calculate byte_count from bit_count
    - [x] Read offset bytes from stream
    - [x] Decode little-endian offset
    - [x] block_index = last_block_index + offset + 1
    - [x] Create BlockMatch instruction
    - [x] Update last_block_index
  - [x] Return Vec<DeltaInstruction>
  - [x] Handle malformed streams gracefully

## Phase 6.2: Implement Delta Exchange Functions ✅ COMPLETE

- [x] Implemented `send_delta_rsync(writer, delta)`:
  - [x] Convert delta to tokens using delta_to_tokens()
  - [x] Send tokens as MSG_DATA message
  - [x] Log token count for debugging
  - [x] Return Result<()>

- [x] Implemented `receive_delta_rsync(reader, checksums)`:
  - [x] Read MSG_DATA message containing tokens
  - [x] Parse tokens using tokens_to_delta()
  - [x] Return Vec<DeltaInstruction>
  - [x] Log instruction count

## Phase 6.3: Create Comprehensive Integration Tests ✅ COMPLETE

### Create `tests/rsync_delta_token_tests.rs`

- [x] Created new test file (280+ lines)
- [x] Added comprehensive module documentation
- [x] Explained rsync token stream format

#### Test: test_literal_encoding

- [x] Create simple literal: "Hello"
- [x] Create DeltaInstruction::Literal(b"Hello".to_vec())
- [x] Encode to tokens using delta_to_tokens()
- [x] Verify token sequence:
  - [x] Token 5 (length)
  - [x] 'H', 'e', 'l', 'l', 'o'
  - [x] Token 0 (end marker)
- [x] Total: 7 bytes

#### Test: test_large_literal_chunking

- [x] Create 200-byte literal data (all zeros)
- [x] Encode to tokens
- [x] Verify chunks: 96 + 96 + 8 bytes
- [x] Verify token sequence:
  - [x] [96][...96 bytes of zeros...]
  - [x] [96][...96 bytes of zeros...]
  - [x] [8][...8 bytes of zeros...]
  - [x] [0]
- [x] Validate chunk boundaries are correct
- [x] Verify all 200 bytes preserved

#### Test: test_block_match_simple_offset

- [x] Create 4 block matches:
  - [x] Block 0 (offset -1 → 0 = token 97)
  - [x] Block 1 (offset 0 → 1 = token 97)
  - [x] Block 3 (offset 1 → 3 = token 100)
  - [x] Block 4 (offset 0 → 4 = token 97)
- [x] Encode to tokens
- [x] Verify tokens: [97, 97, 100, 97, 0]
- [x] Verify offset calculation logic
- [x] Decode and verify block indices match

#### Test: test_block_match_complex_offset

- [x] Create large offset (1000 blocks apart)
- [x] Block matches: 0, 1000
- [x] Encode to tokens
- [x] Verify complex encoding:
  - [x] Token for block 0: 97 (simple)
  - [x] Token for block 1000: 113+ with bit_count
  - [x] Extra offset bytes appended
  - [x] Little-endian encoding verified
- [x] Decode and verify correct block indices

#### Test: test_delta_roundtrip

- [x] Create mixed delta (literals + block matches):
  - [x] Literal (50 bytes)
  - [x] BlockMatch(block_index=5)
  - [x] Literal (100 bytes → should chunk to 96+4)
  - [x] BlockMatch(block_index=10)
  - [x] BlockMatch(block_index=11, consecutive)
- [x] Encode to tokens using delta_to_tokens()
- [x] Decode tokens using tokens_to_delta()
- [x] Verify delta_original == delta_decoded
- [x] Verify all instruction types preserved:
  - [x] Literal sizes correct
  - [x] Literal data matches
  - [x] Block indices correct
- [x] Verify literal chunking: 50 bytes as one chunk, 100 bytes as 96+4

#### Test: test_empty_delta

- [x] Empty delta (no instructions)
- [x] Encode to tokens
- [x] Verify tokens = [0] (just end marker, 1 byte)
- [x] Decode and verify empty Vec returned

#### Test: test_only_literals

- [x] Delta with only Literal instructions:
  - [x] Literal (10 bytes)
  - [x] Literal (50 bytes)
  - [x] Literal (200 bytes → chunks to 96+96+8)
- [x] Encode and verify token stream
- [x] Decode and verify all literals preserved
- [x] Verify chunking behavior correct

#### Test: test_only_block_matches

- [x] Delta with only BlockMatch instructions:
  - [x] Blocks: 0, 1, 2, 3, 4 (all consecutive)
- [x] Encode to tokens
- [x] Verify all tokens are 97 (offset 0)
- [x] Decode and verify block indices: 0, 1, 2, 3, 4
- [x] Verify consecutive block optimization works

#### Test: test_summary

- [x] Document delta token test suite
- [x] List all 8 tests
- [x] Explain token stream format:
  - [x] Token 0: End marker
  - [x] Tokens 1-96: Literal length + data
  - [x] Tokens 97-255: Block match with offset encoding
- [x] Note importance for rsync compatibility

### Acceptance Criteria for Phase 6 ✅ COMPLETE

- [x] 8/8 delta token tests passing
- [x] Token encoding correct:
  - [x] Token 0: End marker ✅
  - [x] Tokens 1-96: Literal length ✅
  - [x] Tokens 97-112: Simple block offset (0-15) ✅
  - [x] Tokens 113-255: Complex block offset with extra bytes ✅
- [x] Literal chunking works (max 96 bytes per chunk) ✅
- [x] Offset encoding works:
  - [x] Simple (0-15): single token
  - [x] Complex (>=16): token + extra bytes (little-endian)
- [x] Roundtrip verified for all patterns:
  - [x] Only literals
  - [x] Only block matches
  - [x] Mixed literals + matches
  - [x] Empty delta
  - [x] Large literals (chunking)
  - [x] Large offsets (complex encoding)
- [x] All edge cases tested
- [x] All using compio runtime (#[compio::test])
- [x] All using futures::join! for concurrency
- [x] Code formatted
- [x] Commit message: "feat(delta): implement rsync delta token encoding/decoding"
- [x] **Commit**: 6e933e9

---

