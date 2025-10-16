# Metadata Preservation

arsync provides comprehensive metadata preservation for both files and directories, ensuring that all file system attributes are correctly copied from source to destination.

## Overview

Metadata preservation in arsync includes:

- **File Permissions**: Including special bits (setuid, setgid, sticky)
- **File Ownership**: User and group ownership preservation
- **File Timestamps**: Access and modification times with nanosecond precision
- **File Extended Attributes**: All extended attributes (xattr)
- **Directory Permissions**: Including special bits for directories
- **Directory Ownership**: Directory user and group ownership
- **Directory Timestamps**: Directory access and modification times
- **Directory Extended Attributes**: All directory extended attributes

## Technical Implementation

### File Descriptor Operations

All metadata preservation uses file descriptor-based operations for maximum efficiency and security:

- **`fchmod`**: File permissions using file descriptors
- **`fchown`**: File ownership using file descriptors
- **`futimesat`**: File timestamps using file descriptors
- **`fgetxattr`/`fsetxattr`/`flistxattr`**: Extended attributes using file descriptors

### Error Handling

Metadata preservation includes comprehensive error handling:

- **Graceful Degradation**: Failed metadata operations log warnings but don't stop the copy process
- **Detailed Logging**: All metadata operations are logged with appropriate detail levels
- **Error Recovery**: Copy continues even if some metadata operations fail

### Performance

Metadata preservation is designed for minimal performance impact:

- **Efficient Syscalls**: Uses the most efficient syscalls available
- **Batch Operations**: Groups related metadata operations when possible
- **Async Operations**: All metadata operations are fully asynchronous

## Usage

### Automatic Metadata Preservation

By default, arsync preserves all metadata:

```bash
# All metadata is preserved by default
arsync --source /data --destination /backup
```

### Selective Metadata Preservation

You can control which metadata is preserved:

```bash
# Preserve only extended attributes
arsync --source /data --destination /backup --preserve-xattr

# Preserve only ownership
arsync --source /data --destination /backup --preserve-ownership

# Disable all metadata preservation
arsync --source /data --destination /backup --no-preserve-metadata
```

### Verbose Metadata Logging

Enable detailed logging for metadata operations:

```bash
# Show metadata preservation details
arsync --source /data --destination /backup --verbose

# Show only metadata warnings
arsync --source /data --destination /backup --log-level metadata
```

## File Metadata Preservation

### Permissions

File permissions are preserved including special bits:

- **Regular Permissions**: Read, write, execute for owner, group, and others
- **Special Bits**: Setuid, setgid, and sticky bits
- **Implementation**: Uses `fchmod` syscall with file descriptors

### Ownership

File ownership is preserved using efficient syscalls:

- **User ID**: Preserves the file's user ownership
- **Group ID**: Preserves the file's group ownership
- **Implementation**: Uses `fchown` syscall with file descriptors

### Timestamps

File timestamps are preserved with nanosecond precision:

- **Access Time**: When the file was last accessed
- **Modification Time**: When the file was last modified
- **Implementation**: Uses `futimesat` syscall with file descriptors

### Extended Attributes

All extended attributes are preserved:

- **User Attributes**: User-defined extended attributes
- **System Attributes**: System-defined extended attributes
- **Security Attributes**: Security-related extended attributes
- **Implementation**: Uses `fgetxattr`, `fsetxattr`, and `flistxattr` syscalls

## Directory Metadata Preservation

### Directory Permissions

Directory permissions are preserved including special bits:

- **Directory Permissions**: Read, write, execute for owner, group, and others
- **Special Bits**: Setuid, setgid, and sticky bits for directories
- **Implementation**: Uses `fchmod` syscall with file descriptors

### Directory Ownership

Directory ownership is preserved:

- **User ID**: Preserves the directory's user ownership
- **Group ID**: Preserves the directory's group ownership
- **Implementation**: Uses `fchown` syscall with file descriptors

### Directory Timestamps

Directory timestamps are preserved:

- **Access Time**: When the directory was last accessed
- **Modification Time**: When the directory was last modified
- **Implementation**: Uses `futimesat` syscall with file descriptors

### Directory Extended Attributes

All directory extended attributes are preserved:

- **User Attributes**: User-defined extended attributes on directories
- **System Attributes**: System-defined extended attributes on directories
- **Implementation**: Uses `fgetxattr`, `fsetxattr`, and `flistxattr` syscalls

## Integration Points

### File Copy Process

Metadata preservation is integrated into the file copy process:

1. **Pre-Copy**: Capture source file metadata
2. **Copy**: Copy file content using io_uring operations
3. **Post-Copy**: Apply metadata to destination file

### Directory Creation Process

Metadata preservation is integrated into directory creation:

1. **Directory Creation**: Create destination directory
2. **Metadata Application**: Apply source directory metadata to destination

## Testing

### Test Coverage

Comprehensive test coverage includes:

- **Basic Metadata Tests**: Permissions, ownership, timestamps, xattr
- **Special Bits Tests**: Setuid, setgid, sticky bit preservation
- **Extended Attributes Tests**: Binary data, multiple attributes, error handling
- **Directory Tests**: Directory metadata preservation
- **Integration Tests**: End-to-end metadata preservation

### Test Execution

Run metadata preservation tests:

```bash
# Run all metadata tests
cargo test metadata

# Run specific metadata tests
cargo test file_xattr_tests
cargo test directory_metadata_tests
cargo test comprehensive_metadata_tests
```

## Troubleshooting

### Common Issues

#### Permission Denied Errors

If you encounter permission denied errors:

```bash
# Run with appropriate permissions
sudo arsync --source /data --destination /backup

# Or preserve only what you have permission for
arsync --source /data --destination /backup --preserve-xattr
```

#### Extended Attributes Not Supported

If extended attributes are not supported on your filesystem:

```bash
# Disable xattr preservation
arsync --source /data --destination /backup --no-preserve-xattr
```

#### Ownership Preservation Fails

If ownership preservation fails:

```bash
# Check if you have appropriate permissions
id
ls -la /data

# Or disable ownership preservation
arsync --source /data --destination /backup --no-preserve-ownership
```

### Debugging

Enable verbose logging to debug metadata issues:

```bash
# Enable detailed logging
arsync --source /data --destination /backup --verbose --log-level debug

# Check specific metadata operations
arsync --source /data --destination /backup --log-level metadata
```

## Performance Considerations

### Metadata Overhead

Metadata preservation adds minimal overhead:

- **File Operations**: ~1-2% overhead for file metadata
- **Directory Operations**: ~0.5-1% overhead for directory metadata
- **Extended Attributes**: Variable overhead based on number of attributes

### Optimization

Metadata preservation is optimized for performance:

- **Efficient Syscalls**: Uses the most efficient syscalls available
- **Batch Operations**: Groups related operations when possible
- **Async Operations**: All operations are fully asynchronous
- **Error Handling**: Minimal overhead for error handling

## Future Enhancements

Planned enhancements for metadata preservation:

- **ACL Support**: POSIX ACL preservation
- **SELinux Context**: SELinux security context preservation
- **Capabilities**: Linux capabilities preservation
- **Performance Optimization**: Further performance improvements
- **Cross-Platform Support**: Windows and macOS metadata preservation
