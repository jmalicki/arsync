# io_uring Safety Quick Reference

**🎯 TL;DR**: We use **compio's owned buffer model** to make io_uring provably safe at compile-time.

---

## The Problem in 30 Seconds

```rust
// ❌ UNSAFE: Borrowed buffer with io_uring
let mut buffer = vec![0u8; 1024];
io_uring.read(&mut buffer).await;  // Kernel holds pointer
buffer[0] = 42;  // ⚠️ DATA RACE - kernel still writing!
```

**Why unsafe?** Kernel owns buffer pointer, Rust borrow checker can't protect you.

---

## Our Solution in 30 Seconds

```rust
// ✅ SAFE: Owned buffer with compio
let buffer = vec![0u8; 1024];
let (result, buffer) = file.read_at(buffer, 0).await;
//                                  ^^^^^^ Ownership transferred
//                                         ^^^^^^ Ownership returned
```

**Why safe?** Compiler prevents accessing buffer during operation.

---

## Key Patterns

### ✅ Pattern 1: Basic Read

```rust
let buffer = vec![0u8; 1024];
let (result, buffer) = file.read_at(buffer, offset).await;
let bytes_read = result?;
// Use buffer[..bytes_read]
```

### ✅ Pattern 2: Buffer Reuse (Zero Allocations)

```rust
let mut buffer = vec![0u8; 64 * 1024];

loop {
    // Read (transfer ownership)
    let (result, buf) = src.read_at(buffer, offset).await;
    let bytes_read = result?;
    buffer = buf;  // Get ownership back
    
    if bytes_read == 0 { break; }
    
    // Write (transfer ownership again)
    let (result, buf) = dst.write_at(buffer, offset).await;
    result?;
    buffer = buf;  // Get ownership back again
}
// Total allocations: 1 (the vec! at the start)
```

### ✅ Pattern 3: Error Handling

```rust
let buffer = vec![0u8; 1024];
let (result, buffer) = file.read_at(buffer, 0).await;

match result {
    Ok(bytes_read) => {
        // Buffer is valid and contains data
        process(&buffer[..bytes_read]);
    }
    Err(e) => {
        // Buffer is returned even on error
        retry_with_buffer(buffer)?;
    }
}
```

---

## Common Mistakes

### ❌ Don't: Try to access buffer during operation

```rust
let buffer = vec![0u8; 1024];
let fut = file.read_at(buffer, 0);
buffer[0] = 42;  // ❌ COMPILE ERROR: value moved
```

### ❌ Don't: Drop buffer before completion

```rust
let buffer = vec![0u8; 1024];
let fut = file.read_at(buffer, 0);
drop(buffer);  // ❌ COMPILE ERROR: value moved
```

### ❌ Don't: Use borrowed buffer APIs with io_uring

```rust
// No such API exists in compio (by design!)
let mut buffer = vec![0u8; 1024];
file.read_at_borrowed(&mut buffer, 0).await;  // ❌ UNSAFE
```

---

## Safety Guarantees

| Guarantee | Mechanism | Example |
|-----------|-----------|---------|
| **No use-after-free** | Ownership transfer | Can't access buffer after `.read_at(buffer, ...)` |
| **No data races** | Exclusive ownership | Can't modify buffer during operation |
| **Safe cancellation** | `Drop` cancels ops | Dropping future cancels io_uring operation |
| **Lifetime safety** | `'static` bound on `IoBuf` | Buffer lives long enough |

---

## Performance

| Metric | Value |
|--------|-------|
| **Safety overhead** | Zero |
| **Memory overhead** | Zero |
| **Speed vs unsafe io_uring** | Same |
| **Allocations** | 1 per buffer (reusable) |

**Conclusion**: Same speed as unsafe code, but compile-time safe.

---

## Comparison Table

| Approach | Safety | Performance | Complexity |
|----------|--------|-------------|------------|
| **Borrowed buffers + io_uring** | ❌ Unsafe | ⚡ Fast | Simple |
| **Raw io_uring (unsafe)** | ❌ Manual | ⚡ Fast | Complex |
| **Compio (owned buffers)** | ✅ Safe | ⚡ Fast | Simple |
| **Tokio-uring (owned buffers)** | ✅ Safe | ⚡ Fast | Simple |

---

## When to Use What

### Use **compio** when:
- ✅ You want io_uring performance with safety
- ✅ You need cross-platform support (Linux/macOS/Windows)
- ✅ You want standard async/await ergonomics
- ✅ You're building a new project

### Use **tokio-uring** when:
- ✅ You're already using Tokio ecosystem
- ✅ You only need Linux support
- ✅ You want official Tokio maintenance

### **Never** use:
- ❌ Raw io_uring with borrowed buffers
- ❌ Manual unsafe buffer management

---

## Quick Debug Checklist

If you get a compile error:

1. ✅ **"value moved"** → Good! This is the safety working
2. ✅ Check you're getting buffer back: `let (result, buffer) = ...`
3. ✅ Reassign buffer: `buffer = buf;`
4. ✅ Don't try to use buffer while operation is pending

---

## Code Review Checklist

- [ ] All io_uring operations use owned buffers (not `&mut`)
- [ ] Buffers are received back from operations: `let (result, buffer) = ...`
- [ ] No `unsafe` blocks related to io_uring buffer management
- [ ] Buffers are reused where possible (performance optimization)
- [ ] Error paths also receive buffers back

---

## Further Reading

- **Full Analysis**: [README.md](README.md)
- **Implementation Examples**: `src/copy.rs` (lines 228-309)
- **Compio Documentation**: <https://docs.rs/compio/>
- **Original Criticism**: <https://tonbo.io/blog/async-rust-is-not-safe-with-io-uring>

---

**Last Updated**: October 21, 2025  
**Maintainer**: arsync development team

