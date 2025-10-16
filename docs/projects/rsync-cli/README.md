# rsync-Compatible CLI Integration

**Status**: 🟡 Design Phase  
**Purpose**: Wire up complete rsync protocol to provide production-ready remote sync  
**Depends On**: [`rsync-wire`](../rsync-wire/) protocol implementation (✅ COMPLETE)

---

## Overview

This project integrates the complete rsync wire protocol implementation (106 tests passing!)
into the CLI to enable actual remote file synchronization over SSH.

**What This Enables**:

```bash
# Pull from remote
arsync user@host:/remote/path /local/path

# Push to remote
arsync /local/path user@host:/remote/path

# Works just like rsync!
```

---

## Current State

### ✅ Already Complete (rsync-wire protocol)

From [`docs/projects/rsync-wire/`](../rsync-wire/):
- ✅ Handshake protocol with 9-state FSM
- ✅ Capability negotiation (10 flags)
- ✅ File list exchange (rsync format, varint encoding)
- ✅ Seeded checksum algorithm
- ✅ Delta token stream encoding
- ✅ End-to-end protocol flow
- ✅ 106/106 tests passing
- ✅ 100% io_uring architecture

### 🟡 What This Project Adds

**Integration Glue** (~200 lines):
1. Wire rsync_compat functions to SSH transport
2. Implement --server mode (for remote invocation)
3. Enable remote-sync feature by default
4. Add integration tests
5. Polish error handling
6. Update documentation

**Estimated Effort**: 7-10 days

---

## Documents

- **[design.md](design.md)** - Complete design specification
  - Current state analysis
  - Gap analysis (what's missing)
  - Component-by-component design
  - CLI user experience
  - Testing strategy
  - **Start here**

- **[plan.md](plan.md)** - Implementation plan (coming soon)
  - Will be created via `/plan`

---

## Key Design Decisions

### 1. Build on rsync-wire Protocol ✅

**Decision**: Use the complete rsync_compat implementation (all phases complete)

**Rationale**: 
- Protocol is thoroughly tested (106 tests)
- Byte-for-byte verification passing
- Just needs SSH transport integration

### 2. SshTransport Wrapper Pattern

**Decision**: Create wrapper struct to adapt SshConnection to Transport trait

**Alternative**: Make SshConnection directly implement Transport
- Start with wrapper (less invasive)
- Can refactor later if needed

### 3. Server Mode Design

**Decision**: --server mode uses stdin/stdout via PipeTransport

**Rationale**:
- Matches rsync behavior
- SSH provides stdin/stdout connection
- rsync_compat already works with PipeTransport

### 4. Feature Flag Strategy

**Decision**: Enable `remote-sync` by default (opt-out pattern)

**Rationale**:
- Users expect `arsync user@host:/path` to work (we parse the syntax)
- rsync compatibility: rsync has remote sync by default
- Binary size impact minimal (~50-100KB)
- Better UX: "Just works" instead of compilation surprises
- Advanced users can opt-out with `--no-default-features` for minimal builds

---

## Dependencies

### Builds On

- **[rsync-wire protocol](../rsync-wire/)** - Complete implementation (✅ DONE)
  - See: `docs/projects/rsync-wire/plan.md`
  - All 7 phases complete
  - 106 tests passing

### Requires

- SSH client installed on system (for remote connections)
- Remote host must have `arsync` installed and in PATH
- SSH key authentication configured (or password auth supported)

---

## Success Criteria

**User Experience**:
- [ ] `arsync user@host:/remote /local` works
- [ ] `arsync /local user@host:/remote` works
- [ ] Metadata fully preserved
- [ ] Clear error messages
- [ ] Progress reporting works

**Technical**:
- [ ] All integration tests passing
- [ ] No regressions in existing tests
- [ ] Clippy clean
- [ ] Documentation complete

**Performance**:
- [ ] Within 20% of rsync performance
- [ ] Handles large files efficiently
- [ ] Handles many small files efficiently

---

## Timeline

| Phase | Description | Duration | Dependencies |
|-------|-------------|----------|--------------|
| 1 | Wire Integration Functions | 1-2 days | rsync-wire complete ✅ |
| 2 | Server Mode | 1 day | Phase 1 |
| 3 | Integration Testing | 2-3 days | Phases 1-2 |
| 4 | Feature Flag & CI | 1 day | Phase 3 |
| 5 | Error Handling & Polish | 1-2 days | Phase 4 |
| 6 | Documentation | 1 day | Phase 5 |

**Total**: 7-10 days

---

## Next Steps

1. **Review Design**: Read [design.md](design.md)
2. **Create Plan**: Run `/plan` to create implementation plan
3. **Create Branch**: `git checkout -b rsync-cli/implementation main`
4. **Start Implementing**: `/implement`

---

**Project**: rsync-compatible CLI integration  
**Owner**: You  
**Started**: TBD  
**Target Completion**: 7-10 days after start

