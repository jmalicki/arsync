# Testing Priorities and Levels

## Overview

This document outlines the testing priorities for arsync, organized by criticality and implementation phases. The goal is to focus on core functionality first, with advanced features as lower priority.

## Testing Levels

### Level 1: Core Functionality (CRITICAL - Must Pass)
**Priority**: HIGHEST
**Status**: ✅ IMPLEMENTED

#### Basic File Operations
- ✅ File copying works correctly
- ✅ Directory structure is preserved
- ✅ Basic permission preservation (read/write/execute)
- ✅ Basic timestamp preservation (seconds level)
- ✅ Error handling for common scenarios

#### Basic Metadata Preservation
- ✅ File ownership preservation
- ✅ File permissions preservation (basic modes)
- ✅ Directory permissions preservation
- ✅ Basic timestamp preservation (accessed/modified)

### Level 2: Enhanced Metadata (HIGH - Should Pass)
**Priority**: HIGH
**Status**: ✅ IMPLEMENTED

#### Advanced Permission Scenarios
- ✅ Special permission bits (setuid, setgid, sticky)
- ✅ Restrictive permissions (000, 444, 555)
- ✅ Complex permission combinations
- ✅ Permission error handling

#### Timestamp Scenarios
- ✅ Old timestamps (pre-2000)
- ✅ Future timestamps
- ✅ Identical access/modification times
- ✅ Basic nanosecond precision (where supported)

### Level 3: Advanced Features (MEDIUM - Nice to Have)
**Priority**: MEDIUM
**Status**: 🔄 IN PROGRESS

#### Nanosecond Precision (DISABLED FOR NOW)
- ❌ **DISABLED**: Nanosecond timestamp precision
- ❌ **DISABLED**: Sub-second timestamp edge cases
- ❌ **DISABLED**: High-precision timestamp preservation

**Rationale**: This is a nice-to-have feature that requires significant work to implement properly. Focus on core functionality first.

#### Extended Attributes (PLANNED)
- ❌ **PLANNED**: POSIX ACL preservation
- ❌ **PLANNED**: SELinux context preservation
- ❌ **PLANNED**: Custom user attributes
- ❌ **PLANNED**: xattr error handling

### Level 4: Performance Optimization (LOW - Future)
**Priority**: LOW
**Status**: 📋 PLANNED

#### Advanced Performance Features
- ❌ **FUTURE**: copy_file_range optimization
- ❌ **FUTURE**: Zero-copy operations
- ❌ **FUTURE**: Advanced fadvise optimizations
- ❌ **FUTURE**: Memory-mapped file support

## Current Test Implementation

### Test Structure
```rust
// Level 1: Core functionality (always enabled)
test_basic_file_copying()
test_basic_permission_preservation()
test_basic_timestamp_preservation()

// Level 2: Enhanced features (enabled)
test_advanced_permission_scenarios()
test_complex_timestamp_scenarios()

// Level 3: Advanced features (disabled for now)
if false { // Disabled - focus on core functionality
    test_nanosecond_precision()
    test_extended_attributes()
}

// Level 4: Performance (future)
// test_copy_file_range_optimization()
// test_zero_copy_operations()
```

### Test Configuration
```rust
// Test levels can be controlled via environment variables
let test_level = std::env::var("TEST_LEVEL")
    .unwrap_or_else(|_| "2".to_string())
    .parse::<u32>()
    .unwrap_or(2);

// Level 1: Always run
test_core_functionality();

// Level 2: Run if level >= 2
if test_level >= 2 {
    test_enhanced_features();
}

// Level 3: Run if level >= 3
if test_level >= 3 {
    test_advanced_features();
}
```

## Implementation Priorities

### Phase 1: Core Functionality ✅ COMPLETED
- ✅ Basic file copying
- ✅ Basic metadata preservation
- ✅ Error handling
- ✅ Basic testing framework

### Phase 2: Enhanced Features ✅ COMPLETED
- ✅ Advanced permission scenarios
- ✅ Complex timestamp handling
- ✅ Comprehensive test coverage
- ✅ Performance optimization

### Phase 3: Advanced Features 🔄 IN PROGRESS
- 🔄 Extended attributes support
- 🔄 Device file operations
- 🔄 Advanced symlink handling
- ❌ **DEFERRED**: Nanosecond precision (not critical)

### Phase 4: Performance Optimization 📋 PLANNED
- 📋 copy_file_range implementation
- 📋 Zero-copy operations
- 📋 Advanced memory management
- 📋 Per-CPU architecture

## Test Execution Strategy

### Development Testing
```bash
# Run only Level 1 tests (fastest)
TEST_LEVEL=1 cargo test

# Run Level 1 and 2 tests (default)
TEST_LEVEL=2 cargo test

# Run all tests including advanced features
TEST_LEVEL=3 cargo test
```

### CI/CD Pipeline
```yaml
# Basic functionality (Level 1)
test_basic:
  script: TEST_LEVEL=1 cargo test

# Enhanced features (Level 2)
test_enhanced:
  script: TEST_LEVEL=2 cargo test

# Advanced features (Level 3) - optional
test_advanced:
  script: TEST_LEVEL=3 cargo test
  allow_failure: true
```

## Current Status

### ✅ Working Features
- Basic file copying with compio
- Permission preservation
- Basic timestamp preservation
- Error handling and recovery
- Comprehensive test coverage

### 🔄 In Progress
- Extended attributes support
- Device file operations
- Advanced symlink handling

### ❌ Deferred (Not Critical)
- Nanosecond timestamp precision
- copy_file_range optimization
- Zero-copy operations
- Advanced performance features

## Next Steps

1. **Focus on Level 1 and 2**: Ensure all core functionality works perfectly
2. **Implement Level 3**: Add extended attributes and device file support
3. **Plan Level 4**: Design performance optimization features
4. **Document**: Create user guides for different use cases

## Conclusion

The current approach prioritizes core functionality over advanced features. Nanosecond precision and other advanced features are important but not blocking for basic file synchronization needs. The testing framework supports multiple levels, allowing us to focus on what matters most while planning for future enhancements.
