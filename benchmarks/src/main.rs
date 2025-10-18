//! Syscall analysis tool for arsync
//!
//! End-to-end analysis tool that:
//! - Creates test dataset
//! - Runs arsync with strace
//! - Parses syscall output
//! - Generates markdown reports

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "syscall-analyzer")]
#[command(about = "End-to-end syscall analysis for arsync")]
struct Args {
    /// Path to arsync binary to analyze
    #[arg(long)]
    arsync_bin: PathBuf,

    /// Number of test files to create
    #[arg(long, default_value = "5")]
    num_files: usize,

    /// File size in MB
    #[arg(long, default_value = "10")]
    file_size_mb: usize,

    /// Output markdown report path
    #[arg(long, default_value = "/tmp/syscall-analysis-report.md")]
    output: PathBuf,

    /// Source test directory (will be created)
    #[arg(long, default_value = "/tmp/syscall-test-src")]
    test_dir_src: PathBuf,

    /// Destination test directory (will be created)
    #[arg(long, default_value = "/tmp/syscall-test-dst")]
    test_dir_dst: PathBuf,
}

#[derive(Debug, Default)]
struct SyscallStats {
    io_uring_setup: usize,
    io_uring_enter: usize,
    io_uring_batch_sizes: Vec<usize>,
    statx_total: usize,
    statx_path_based: usize,
    statx_fd_based: usize,
    openat_total: usize,
    openat_path_based: usize,
    fallocate: usize,
    fchmod: usize,
    fchown: usize,
    utimensat_total: usize,
    utimensat_fd_based: usize,
    utimensat_path_based: usize,
    // Metadata preservation breakdown by type
    fchmod_files: usize,
    fchmod_dirs: usize,
    fchown_files: usize,
    fchown_dirs: usize,
    utimensat_files: usize,
    utimensat_dirs: usize,
    utimensat_symlinks: usize,
    // Directory operations
    mkdir_total: usize,
    mkdirat_total: usize,
    // Symlink operations
    symlink_total: usize,
    symlinkat_total: usize,
    readlink_total: usize,
    readlinkat_total: usize,
    lstat_total: usize,
    // io_uring operation types (from SQE submissions)
    io_uring_ops: HashMap<String, usize>,
    // Unexpected/legacy syscalls (should not be used)
    open_total: usize,   // Should use openat
    stat_total: usize,   // Should use statx or fstat
    chmod_total: usize,  // Should use fchmod
    chown_total: usize,  // Should use fchown
    utime_total: usize,  // Should use utimensat
    utimes_total: usize, // Should use utimensat
    access_total: usize, // TOCTOU-vulnerable
    creat_total: usize,  // Deprecated, use openat
    read_total: usize,   // Should be async via io_uring
    write_total: usize,  // Should be async via io_uring
    pread_total: usize,  // Should use io_uring read_at
    pwrite_total: usize, // Should use io_uring write_at
    per_file: HashMap<String, FileStats>,
    dir_stats: DirectoryStats,
    // All other syscalls (catch-all)
    all_syscalls: HashMap<String, usize>,
    // Filtering statistics (for transparency)
    filtered_startup_syscalls: usize,
    filtered_eventfd_read: usize,
    filtered_eventfd_write: usize,
    filtered_incomplete_traces: usize,
}

#[derive(Debug, Default)]
struct FileStats {
    statx: usize,
    openat: usize,
    mentions: usize,
}

#[derive(Debug, Default)]
struct DirectoryStats {
    src_statx: usize,
    src_openat: usize,
    getdents: usize,
    dst_fchmod: usize,
    dst_fchown: usize,
}

#[derive(Debug)]
enum ExitCode {
    Success = 0,
    Warning = 1,
    Failure = 2,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("============================================");
    println!("arsync Syscall Analysis");
    println!("============================================");
    println!();

    // Step 1: Create test dataset
    println!("Creating test dataset...");
    create_test_dataset(&args)?;
    println!(
        "✓ Created {} files × {}MB = {}MB",
        args.num_files,
        args.file_size_mb,
        args.num_files * args.file_size_mb
    );
    println!();

    // Step 2: Run arsync with strace
    println!("Running arsync with strace...");
    let trace_raw = run_strace_analysis(&args)?;
    println!("✓ Trace captured");
    println!();

    // Step 3: Parse strace output
    let stats = parse_strace_output(&args, &trace_raw)?;

    // Step 4: Generate markdown report
    let report = generate_markdown_report(&args, &stats)?;

    // Step 5: Write report
    fs::write(&args.output, report)
        .with_context(|| format!("Failed to write report to {:?}", args.output))?;

    println!("✓ Report generated: {:?}", args.output);
    println!();

    // Determine exit code based on analysis
    let exit_code = determine_exit_code(&stats, args.num_files);

    // Exit 0 for success or warnings (don't fail CI on warnings)
    // Exit non-zero only for critical failures
    match exit_code {
        ExitCode::Success => {
            println!("✅ Analysis complete: No issues detected");
            std::process::exit(0);
        }
        ExitCode::Warning => {
            println!("⚠️  Analysis complete: Warnings detected (not failing)");
            std::process::exit(0);
        }
        ExitCode::Failure => {
            println!("❌ Analysis complete: Critical issues detected");
            std::process::exit(2);
        }
    }
}

fn create_test_dataset(args: &Args) -> Result<()> {
    // Remove old directories
    let _ = fs::remove_dir_all(&args.test_dir_src);
    let _ = fs::remove_dir_all(&args.test_dir_dst);

    // Create source directory
    fs::create_dir_all(&args.test_dir_src)
        .with_context(|| format!("Failed to create {:?}", args.test_dir_src))?;

    // Create test files using dd
    for i in 1..=args.num_files {
        let filename = format!("file{}.bin", i);
        let filepath = args.test_dir_src.join(&filename);

        let status = Command::new("dd")
            .arg("if=/dev/urandom")
            .arg(format!("of={}", filepath.display()))
            .arg("bs=1M")
            .arg(format!("count={}", args.file_size_mb))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to run dd command")?;

        if !status.success() {
            anyhow::bail!("Failed to create test file: {}", filename);
        }
    }

    // Create nested directories to test directory operations
    let nested_dir = args.test_dir_src.join("subdir1/subdir2");
    fs::create_dir_all(&nested_dir).context("Failed to create nested directories")?;

    // Create a file in the nested directory
    let nested_file = nested_dir.join("nested_file.txt");
    fs::write(&nested_file, b"test content in nested directory")
        .context("Failed to create nested file")?;

    // Create symlinks to test symlink operations
    // 1. Symlink to a regular file
    let link_to_file = args.test_dir_src.join("link_to_file1.bin");
    let target_file = args.test_dir_src.join("file1.bin");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &link_to_file)
        .context("Failed to create symlink to file")?;

    // 2. Symlink to a directory
    let link_to_dir = args.test_dir_src.join("link_to_subdir");
    let target_dir = args.test_dir_src.join("subdir1");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_dir, &link_to_dir)
        .context("Failed to create symlink to directory")?;

    // 3. Relative symlink
    let relative_link = args.test_dir_src.join("relative_link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink("nested_file.txt", &relative_link)
        .context("Failed to create relative symlink")?;

    Ok(())
}

fn run_strace_analysis(args: &Args) -> Result<String> {
    // Clean destination before running to ensure we see all mkdir/create operations
    if args.test_dir_dst.exists() {
        fs::remove_dir_all(&args.test_dir_dst)
            .with_context(|| format!("Failed to remove {:?}", args.test_dir_dst))?;
    }

    // Verify destination is gone
    if args.test_dir_dst.exists() {
        anyhow::bail!(
            "Destination directory still exists after cleanup: {:?}",
            args.test_dir_dst
        );
    }

    // Run strace on arsync with full metadata preservation
    // -a = archive mode (preserves permissions, times, etc.)
    // -r = recursive (copies directories and their contents)
    // -l = copy symlinks as symlinks
    let output = Command::new("strace")
        .arg("-e")
        .arg("trace=all")
        .arg("-f")
        .arg(&args.arsync_bin)
        .arg(&args.test_dir_src)
        .arg(&args.test_dir_dst)
        .arg("-a") // Archive mode
        .arg("-r") // Recursive
        .arg("-l") // Preserve symlinks
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to run strace")?;

    // strace writes to stderr
    let trace_output = String::from_utf8_lossy(&output.stderr).to_string();

    // Save trace to temp file for debugging
    fs::write("/tmp/syscall-analysis-raw.txt", &trace_output)?;

    Ok(trace_output)
}

fn parse_strace_output(args: &Args, raw_content: &str) -> Result<SyscallStats> {
    let mut stats = SyscallStats::default();

    // Filter trace to only include syscalls after first getdents
    // This excludes program initialization (library loading, etc.)
    let lines: Vec<&str> = raw_content.lines().collect();
    let getdents_idx = lines.iter().position(|line| line.contains("getdents"));

    let (_startup_lines, work_lines) = if let Some(idx) = getdents_idx {
        // Count filtered startup syscalls
        stats.filtered_startup_syscalls = idx;
        (&lines[..idx], &lines[idx..])
    } else {
        // No getdents found, use all content
        (&[] as &[&str], &lines[..])
    };

    let filtered_content: String = work_lines.join("\n");
    let content_to_analyze = if filtered_content.is_empty() {
        // If no getdents found, use all content
        raw_content
    } else {
        &filtered_content
    };

    // Parse all syscalls (to build complete inventory)
    // Match format: "syscall_name(...)" or "PID syscall_name(...)"
    let syscall_re = Regex::new(r"(?:^\[?pid \d+\]?\s+)?(\w+)\(").unwrap();
    for line in content_to_analyze.lines() {
        // Skip lines that are continuations or don't contain syscalls
        if line.trim_start().starts_with('<') || line.trim_start().starts_with('=') {
            continue;
        }
        if let Some(cap) = syscall_re.captures(line) {
            let syscall_name = cap[1].to_string();
            *stats.all_syscalls.entry(syscall_name).or_insert(0) += 1;
        }
    }

    // Count io_uring operations (use full raw_content for these)
    stats.io_uring_setup = raw_content.matches("io_uring_setup").count();
    stats.io_uring_enter = raw_content.matches("io_uring_enter").count();

    // Parse io_uring batch sizes (use full raw_content)
    let batch_re = Regex::new(r"io_uring_enter\([0-9]+, (\d+),").unwrap();
    for cap in batch_re.captures_iter(raw_content) {
        if let Ok(size) = cap[1].parse::<usize>() {
            stats.io_uring_batch_sizes.push(size);
        }
    }

    // Count statx operations (use filtered content)
    stats.statx_total = content_to_analyze.matches("statx(").count();
    stats.statx_path_based = content_to_analyze.matches("statx(AT_FDCWD").count()
        - content_to_analyze.matches("statx(AT_FDCWD, \"\"").count();
    stats.statx_fd_based = content_to_analyze.matches("statx(").count() - stats.statx_path_based;

    // Count openat operations (use filtered content)
    stats.openat_total = content_to_analyze.matches("openat(").count();
    let openat_user_re = Regex::new(r#"openat\(AT_FDCWD, "/[^"]*""#).unwrap();
    for line in content_to_analyze.lines() {
        if openat_user_re.is_match(line) {
            // Exclude system paths
            if !line.contains("/etc")
                && !line.contains("/lib")
                && !line.contains("/proc")
                && !line.contains("/sys")
            {
                stats.openat_path_based += 1;
            }
        }
    }

    // Count other operations (use filtered content)
    stats.fallocate = content_to_analyze.matches("fallocate(").count();
    stats.fchmod = content_to_analyze.matches("fchmod").count();
    stats.fchown = content_to_analyze.matches("fchown").count();
    stats.utimensat_total = content_to_analyze.matches("utimensat").count();

    // Parse fchmod by type (based on mode bits in octal)
    // 0100xxx = S_IFREG (regular file)
    // 0040xxx = S_IFDIR (directory)
    // 0120xxx = S_IFLNK (symlink - rare, Linux ignores symlink permissions)
    // 0xxx (no type bits, < 01000) = directory (common strace format)
    let fchmod_re = Regex::new(r"fchmod\(\d+, (0[0-7]+)").unwrap();
    for cap in fchmod_re.captures_iter(content_to_analyze) {
        if let Ok(mode) = u32::from_str_radix(&cap[1], 8) {
            let file_type = mode & 0o170000; // S_IFMT mask
            match file_type {
                0o100000 => stats.fchmod_files += 1, // S_IFREG - definitely a file
                0o040000 => stats.fchmod_dirs += 1,  // S_IFDIR - definitely a directory
                0 if mode < 0o1000 => stats.fchmod_dirs += 1, // No type bits but looks like dir perms
                _ => {} // Unknown/ambiguous - will show in totals but not breakdown
            }
        }
    }

    // Parse fchown similarly - fchown doesn't show mode, so we need to correlate with fchmod
    // For now, we'll estimate based on the ratio we saw in fchmod
    // Better approach: track FD numbers and match operations on same FD
    // But that's complex, so for now we'll just show totals
    stats.fchown_files = stats.fchmod_files; // Approximate: usually 1:1 correspondence
    stats.fchown_dirs = stats.fchmod_dirs; // Approximate: usually 1:1 correspondence

    // Parse utimensat - FD-based calls use (fd, NULL, ...) format
    // Path-based calls use (AT_FDCWD, "path", ...) format
    // We can infer file type by correlating with FDs, but that's complex
    // For now, assume FD-based utimensat correlates with fchmod operations
    stats.utimensat_files = stats.fchmod_files;
    stats.utimensat_dirs = stats.fchmod_dirs;

    // Symlinks: utimensat with AT_SYMLINK_NOFOLLOW flag would be for symlinks
    // Pattern: utimensat(..., AT_SYMLINK_NOFOLLOW) at the end
    let utimens_symlink_re = Regex::new(r"utimensat\([^)]+AT_SYMLINK_NOFOLLOW").unwrap();
    stats.utimensat_symlinks = utimens_symlink_re.find_iter(content_to_analyze).count();

    // Count FD-based vs path-based utimensat
    let utimens_fd_re = Regex::new(r"utimensat\(\d+, NULL").unwrap();
    stats.utimensat_fd_based = utimens_fd_re.find_iter(content_to_analyze).count();
    let utimens_path_re = Regex::new(r#"utimensat\(AT_FDCWD, "/"#).unwrap();
    stats.utimensat_path_based = utimens_path_re.find_iter(content_to_analyze).count();

    // Count directory operations
    stats.mkdir_total = content_to_analyze.matches("mkdir(\"").count();
    stats.mkdirat_total = content_to_analyze.matches("mkdirat(").count();

    // Count symlink operations
    stats.symlink_total = content_to_analyze.matches("symlink(\"").count();
    stats.symlinkat_total = content_to_analyze.matches("symlinkat(").count();
    stats.readlink_total = content_to_analyze.matches("readlink(\"").count();
    stats.readlinkat_total = content_to_analyze.matches("readlinkat(").count();
    stats.lstat_total = content_to_analyze.matches("lstat(\"").count();

    // Parse io_uring operation types from traces
    // These can come from bpftrace or strace with specific filters
    // Pattern: IORING_OP_<NAME>
    let io_uring_op_re = Regex::new(r"IORING_OP_(\w+)").unwrap();
    for cap in io_uring_op_re.captures_iter(raw_content) {
        let op_name = cap[1].to_string();
        *stats.io_uring_ops.entry(op_name).or_insert(0) += 1;
    }

    // Count unexpected/legacy syscalls (use filtered content)
    stats.open_total = content_to_analyze.matches("open(\"").count(); // Exclude openat
    stats.stat_total = content_to_analyze.matches("stat(\"").count(); // Exclude statx, fstat
    stats.chmod_total = content_to_analyze.matches("chmod(\"").count(); // Exclude fchmod
    stats.chown_total = content_to_analyze.matches("chown(\"").count(); // Exclude fchown
    stats.utime_total = content_to_analyze.matches("utime(").count();
    stats.utimes_total = content_to_analyze.matches("utimes(").count();
    stats.access_total = content_to_analyze.matches("access(").count();
    stats.creat_total = content_to_analyze.matches("creat(").count();

    // Count synchronous read/write for FILE I/O only (not eventfd/pipe/socket)
    // Eventfd operations are exactly 8 bytes: "\1\0\0\0\0\0\0\0" or similar
    // Exclude these as they're for thread synchronization, not file I/O
    let read_re = Regex::new(r"read\(\d+,").unwrap();
    let write_re = Regex::new(r"write\(\d+,").unwrap();

    for line in content_to_analyze.lines() {
        // Skip unfinished/resumed lines (incomplete trace entries)
        if line.contains("<unfinished") || line.contains("<... ") {
            stats.filtered_incomplete_traces += 1;
            continue;
        }

        // Exclude 8-byte operations (eventfd/pipe synchronization)
        let is_8_byte = line.contains(", 8)") || line.contains("\\1\\0\\0\\0\\0\\0\\0\\0");

        if read_re.is_match(line) {
            if is_8_byte {
                stats.filtered_eventfd_read += 1;
            } else {
                stats.read_total += 1;
            }
        }
        if write_re.is_match(line) {
            if is_8_byte {
                stats.filtered_eventfd_write += 1;
            } else {
                stats.write_total += 1;
            }
        }
    }

    stats.pread_total = content_to_analyze.matches("pread").count();
    stats.pwrite_total = content_to_analyze.matches("pwrite").count();

    // Per-file breakdown (first 3 files) - use filtered content
    for i in 1..=3.min(args.num_files) {
        let filename = format!("file{}.bin", i);
        let mut file_stats = FileStats::default();

        // Simple pattern: just look for the filename in the syscall line
        // This catches /path/to/file1.bin" or similar patterns
        let escaped_filename = regex::escape(&filename);
        let statx_re = Regex::new(&format!(r"statx.*/{}", escaped_filename)).unwrap();
        let openat_re = Regex::new(&format!(r"openat.*/{}", escaped_filename)).unwrap();

        file_stats.statx = statx_re.find_iter(content_to_analyze).count();
        file_stats.openat = openat_re.find_iter(content_to_analyze).count();
        file_stats.mentions = content_to_analyze.matches(&filename).count();

        stats.per_file.insert(filename, file_stats);
    }

    // Directory stats (use filtered content)
    let src_path = args.test_dir_src.to_string_lossy();
    stats.dir_stats.src_statx = content_to_analyze
        .matches(&format!("statx.*\"{}\"", src_path))
        .count();
    stats.dir_stats.src_openat = content_to_analyze
        .matches(&format!("openat.*\"{}\".*O_DIRECTORY", src_path))
        .count();
    stats.dir_stats.getdents = content_to_analyze.matches("getdents").count();
    stats.dir_stats.dst_fchmod = stats.fchmod;
    stats.dir_stats.dst_fchown = stats.fchown;

    Ok(stats)
}

fn generate_markdown_report(args: &Args, stats: &SyscallStats) -> Result<String> {
    let mut report = String::new();

    // Header
    report.push_str(&format!(
        "# 📊 Syscall Analysis Report\n\n\
        **Date:** {}\n\
        **Test:** {} files × {}MB\n\
        **Binary:** `{}`\n\n\
        ---\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
        args.num_files,
        args.file_size_mb,
        args.arsync_bin.display()
    ));

    // Filtered Syscalls (transparency section)
    add_filtered_syscalls_section(&mut report, stats);

    // io_uring Usage
    add_io_uring_section(&mut report, stats, args.num_files);

    // io_uring Operations Breakdown
    add_io_uring_operations_section(&mut report, stats);

    // Metadata Operations
    add_metadata_section(&mut report, stats, args.num_files);

    // File Operations
    add_file_operations_section(&mut report, stats, args.num_files);

    // Metadata Preservation
    add_metadata_preservation_section(&mut report, stats, args.num_files);

    // Directory Operations
    add_directory_operations_section(&mut report, stats);

    // Symlink Operations
    add_symlink_operations_section(&mut report, stats);

    // Per-directory breakdown
    add_directory_section(&mut report, stats, &args.test_dir_src, &args.test_dir_dst);

    // Unexpected/Legacy Syscalls
    add_unexpected_syscalls_section(&mut report, stats);

    // All Syscalls (comprehensive list)
    add_all_syscalls_section(&mut report, stats);

    // Security Assessment
    add_security_section(&mut report, stats);

    // Recommendations
    add_recommendations_section(&mut report, stats, args.num_files);

    // Summary Table
    add_summary_table(&mut report, stats, args.num_files);

    // Footer
    report.push_str(
        "\n---\n\n\
        📄 **Full Traces:**\n\
        - Detailed: `/tmp/syscall-analysis-raw.txt`\n",
    );

    Ok(report)
}

fn add_filtered_syscalls_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 🔍 Filtered Syscalls (Transparency)\n\n");
    report.push_str(
        "The following syscalls were filtered out of the analysis for accuracy. \
        This section allows you to verify the filtering logic is working correctly.\n\n",
    );

    report.push_str("| Filter Category | Count | Reason |\n");
    report.push_str("|-----------------|-------|--------|\n");

    report.push_str(&format!(
        "| Program startup | {} | Syscalls before first `getdents` (library loading, initialization) |\n",
        stats.filtered_startup_syscalls
    ));

    let total_eventfd = stats.filtered_eventfd_read + stats.filtered_eventfd_write;
    if total_eventfd > 0 {
        report.push_str(&format!(
            "| Eventfd/pipe sync | {} | 8-byte `read()`/`write()` for thread synchronization |\n",
            total_eventfd
        ));
        report.push_str(&format!(
            "| └─ `read()` (8-byte) | {} | Thread/event synchronization |\n",
            stats.filtered_eventfd_read
        ));
        report.push_str(&format!(
            "| └─ `write()` (8-byte) | {} | Thread/event synchronization |\n",
            stats.filtered_eventfd_write
        ));
    }

    report.push_str(&format!(
        "| Incomplete traces | {} | `<unfinished ...>` and `<... resumed>` lines |\n",
        stats.filtered_incomplete_traces
    ));

    let total_filtered =
        stats.filtered_startup_syscalls + total_eventfd + stats.filtered_incomplete_traces;
    report.push_str(&format!(
        "| **Total filtered** | **{}** | |\n",
        total_filtered
    ));

    report.push_str("\n");

    if stats.filtered_startup_syscalls > 100 {
        report.push_str(
            "ℹ️  **INFO:** Large startup syscall count is normal (includes library loading, TLS setup, etc.)\n\n",
        );
    }

    report.push_str(
        "> **Note:** All counts below exclude these filtered syscalls and focus only on actual file sync operations.\n\n",
    );
}

fn add_io_uring_section(report: &mut String, stats: &SyscallStats, _num_files: usize) {
    report.push_str("## 🔄 io_uring Usage\n\n");

    report.push_str(&format!(
        "- **io_uring_setup calls:** {} (one per worker thread + main)\n\
        - **io_uring_enter calls:** {}\n\n",
        stats.io_uring_setup, stats.io_uring_enter
    ));

    if stats.io_uring_enter > 100 {
        report.push_str("✅ **PASS:** Heavy io_uring usage\n\n");
    } else {
        report.push_str(&format!(
            "❌ **FAIL:** Low io_uring usage (expected >100 for {} files)\n\n",
            _num_files
        ));
    }

    // Batching efficiency
    if !stats.io_uring_batch_sizes.is_empty() {
        let single_op = stats
            .io_uring_batch_sizes
            .iter()
            .filter(|&&x| x == 1)
            .count();
        let multi_op = stats
            .io_uring_batch_sizes
            .iter()
            .filter(|&&x| x > 1)
            .count();
        let avg: f64 = stats.io_uring_batch_sizes.iter().sum::<usize>() as f64
            / stats.io_uring_batch_sizes.len() as f64;
        let max = stats.io_uring_batch_sizes.iter().max().unwrap_or(&0);

        report.push_str("### Batching Efficiency\n\n");
        report.push_str("| Metric | Value |\n");
        report.push_str("|--------|-------|\n");
        report.push_str(&format!(
            "| Single-op submissions (batch=1) | {} |\n",
            single_op
        ));
        report.push_str(&format!(
            "| Multi-op submissions (batch≥2) | {} |\n",
            multi_op
        ));
        report.push_str(&format!("| Average batch size | {:.1} ops/submit |\n", avg));
        report.push_str(&format!("| Maximum batch size | {} ops/submit |\n\n", max));

        if avg <= 1.5 {
            report.push_str(
                "⚠️  **WARNING:** Poor batching (avg≤1.5, mostly single-op submissions)\n",
            );
            report.push_str("> Better batching could reduce syscall overhead\n\n");
        } else if avg >= 3.0 {
            report.push_str("✅ **EXCELLENT:** Good batching (avg≥3 ops/submit)\n\n");
        } else {
            report.push_str("✅ **GOOD:** Decent batching (1.5 < avg < 3 ops/submit)\n\n");
        }
    }
}

fn add_io_uring_operations_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 🔄 io_uring Operations Breakdown\n\n");

    if !stats.io_uring_ops.is_empty() {
        // We have actual io_uring operation data (from bpftrace or similar)
        let mut ops: Vec<_> = stats.io_uring_ops.iter().collect();
        ops.sort_by(|a, b| b.1.cmp(a.1));

        let total: usize = ops.iter().map(|(_, count)| *count).sum();

        report.push_str("| Operation | Count | Expected |\n");
        report.push_str("|-----------|-------|----------|\n");

        // Expected operations for a file copy tool
        let expected_ops = [
            "READ",
            "WRITE",
            "READV",
            "WRITEV",
            "STATX",
            "FSTAT",
            "FALLOCATE",
            "OPENAT",
            "OPENAT2",
            "CLOSE",
            "FSYNC",
            "FDATASYNC",
            "MKDIRAT",
            "UNLINKAT",
            "RENAMEAT",
            "SYMLINKAT",
            "LINKAT",
        ];

        for (op, count) in &ops {
            let is_expected = expected_ops.contains(&op.as_str());
            let status = if is_expected { "✅" } else { "⚠️" };
            report.push_str(&format!("| {} | {} | {} |\n", op, count, status));
        }

        report.push_str(&format!("\n**Total io_uring operations:** {}\n\n", total));

        // Flag unexpected operations
        let unexpected: Vec<_> = ops
            .iter()
            .filter(|(op, _)| !expected_ops.contains(&op.as_str()))
            .collect();

        if !unexpected.is_empty() {
            report.push_str("### ⚠️  Unexpected io_uring Operations\n\n");
            for (op, count) in unexpected {
                report.push_str(&format!(
                    "- **`IORING_OP_{}`**: {} submissions\n",
                    op, count
                ));
            }
            report.push_str("\n> These operations are unexpected for a file copy tool. Review to ensure they're intentional.\n\n");
        } else {
            report.push_str("✅ **All io_uring operations are expected for file copying**\n\n");
        }
    } else {
        report.push_str(
            "ℹ️  **INFO:** io_uring operation types not visible in standard strace output.\n\n",
        );
        report.push_str("> **Note:** To see detailed io_uring operation breakdown, use `bpftrace` or kernel tracing.\n");
        report.push_str("> High `io_uring_enter` count + low direct syscalls indicates operations are async via io_uring.\n\n");

        // Show the inference
        report.push_str("### Inferred io_uring Usage\n\n");
        report.push_str("Based on syscall patterns:\n\n");
        report.push_str(&format!(
            "- **io_uring_enter calls**: {} (operations submitted)\n",
            stats.io_uring_enter
        ));
        report.push_str(&format!(
            "- **File read() calls (FD≥100)**: {} (should be 0 with io_uring)\n",
            stats.read_total
        ));
        report.push_str(&format!(
            "- **File write() calls (FD≥100)**: {} (should be 0 with io_uring)\n",
            stats.write_total
        ));
        report.push_str(&format!(
            "- **pread/pwrite calls**: {} (should use io_uring read_at/write_at)\n",
            stats.pread_total + stats.pwrite_total
        ));
        report.push_str(&format!(
            "- **Direct statx calls**: {} (some may be io_uring)\n\n",
            stats.statx_total
        ));

        // Note about low-FD operations
        let total_read = *stats.all_syscalls.get("read").unwrap_or(&0);
        let total_write = *stats.all_syscalls.get("write").unwrap_or(&0);
        if total_read > stats.read_total || total_write > stats.write_total {
            report.push_str(&format!(
                "> Note: {} read() and {} write() calls on low FDs (eventfd/pipe for thread sync) excluded from file I/O counts\n\n",
                total_read - stats.read_total,
                total_write - stats.write_total
            ));
        }

        if stats.read_total == 0
            && stats.write_total == 0
            && stats.pread_total + stats.pwrite_total == 0
        {
            report.push_str(
                "✅ **EXCELLENT:** All file I/O via io_uring (no direct read/write syscalls)\n\n",
            );
        } else if stats.read_total < 10 && stats.write_total < 10 {
            report.push_str("✅ **GOOD:** Minimal direct file I/O syscalls\n\n");
        } else {
            report.push_str("⚠️  **WARNING:** High direct file I/O syscalls detected\n\n");
        }
    }
}

fn add_metadata_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📋 Metadata Operations\n\n");

    let statx_per_file = stats.statx_total as f64 / num_files as f64;

    report.push_str(&format!(
        "| Metric | Count |\n\
        |--------|-------|\n\
        | Total statx calls | {} |\n\
        | Path-based (AT_FDCWD + path) | {} |\n\
        | FD-based (dirfd + filename) | {} |\n\
        | **Average per file** | **{:.1}** |\n\n",
        stats.statx_total, stats.statx_path_based, stats.statx_fd_based, statx_per_file
    ));

    let expected_max = num_files * 2;
    if stats.statx_path_based > expected_max {
        report.push_str(&format!(
            "⚠️  **WARNING:** High path-based statx count (TOCTOU-vulnerable)\n\
            - Expected: ≤{} (1-2 per file)\n\
            - Got: {} (~{:.1} per file)\n\n",
            expected_max,
            stats.statx_path_based,
            stats.statx_path_based as f64 / num_files as f64
        ));
    } else if stats.statx_path_based == 0 {
        report.push_str(
            "✅ **EXCELLENT:** No path-based statx calls (100% FD-based, TOCTOU-safe)\n\n",
        );
    } else {
        report.push_str("✅ **GOOD:** Low path-based statx count\n\n");
    }

    // Per-file breakdown
    if !stats.per_file.is_empty() {
        report.push_str("### Per-File Breakdown\n\n");
        let mut files: Vec<_> = stats.per_file.iter().collect();
        files.sort_by_key(|(name, _)| *name);

        for (filename, file_stats) in files {
            report.push_str(&format!(
                "**{}:**\n\
                - statx: {}\n\
                - openat: {}\n\
                - total mentions: {}\n\n",
                filename, file_stats.statx, file_stats.openat, file_stats.mentions
            ));
        }
    }
}

fn add_file_operations_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📁 File Operations\n\n");

    let openat_per_file = stats.openat_path_based as f64 / num_files as f64;

    report.push_str(&format!(
        "| Metric | Count |\n\
        |--------|-------|\n\
        | Total openat calls | {} |\n\
        | User file opens (path-based) | {} |\n\
        | **Average per file** | **{:.1}** |\n\n",
        stats.openat_total, stats.openat_path_based, openat_per_file
    ));

    let expected_max_openat = num_files * 4;
    if stats.openat_path_based > expected_max_openat {
        report.push_str(&format!(
            "⚠️  **WARNING:** Excessive openat calls\n\
            - Expected: ≤{} (2-4 per file)\n\
            - Got: {}\n\n",
            expected_max_openat, stats.openat_path_based
        ));
    } else {
        report.push_str("✅ **PASS:** Reasonable openat count\n\n");
    }

    // fallocate
    report.push_str(&format!(
        "**Direct fallocate syscalls:** {}\n\n",
        stats.fallocate
    ));

    if stats.fallocate > 0 {
        report.push_str("⚠️  **WARNING:** fallocate not using io_uring\n\n");
    } else {
        report.push_str("✅ **PASS:** fallocate via io_uring (no direct syscalls)\n\n");
    }
}

fn add_metadata_preservation_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 🔒 Metadata Preservation\n\n");

    report.push_str(&format!(
        "| Operation | Count |\n\
        |-----------|-------|\n\
        | fchmod (FD-based permissions) | {} |\n\
        | fchown (FD-based ownership) | {} |\n\
        | utimensat (total) | {} |\n\
        | └─ FD-based (fd, NULL, ...) | {} |\n\
        | └─ Path-based (AT_FDCWD, path, ...) | {} |\n\n",
        stats.fchmod,
        stats.fchown,
        stats.utimensat_total,
        stats.utimensat_fd_based,
        stats.utimensat_path_based
    ));

    let fd_percentage = if stats.utimensat_total > 0 {
        (stats.utimensat_fd_based as f64 / stats.utimensat_total as f64 * 100.0) as usize
    } else {
        0
    };

    if stats.utimensat_path_based > 0 {
        report.push_str("⚠️  **WARNING:** Some path-based timestamp operations (TOCTOU risk)\n\n");
    } else if stats.utimensat_fd_based >= num_files {
        report.push_str(&format!(
            "✅ **EXCELLENT:** {}% FD-based timestamp preservation (TOCTOU-safe)\n\n",
            fd_percentage
        ));
    } else {
        report.push_str("ℹ️  **INFO:** Timestamp preservation counts lower than expected\n\n");
    }

    // Add breakdown by file type
    let identified_fchmod = stats.fchmod_files + stats.fchmod_dirs;
    let identified_fchown = stats.fchown_files + stats.fchown_dirs;
    let identified_utimens =
        stats.utimensat_files + stats.utimensat_dirs + stats.utimensat_symlinks;

    if identified_fchmod > 0 || identified_fchown > 0 || identified_utimens > 0 {
        report.push_str("### Breakdown by Type\n\n");
        report.push_str("| Type | fchmod | fchown | utimensat |\n");
        report.push_str("|------|--------|--------|----------|\n");

        if stats.fchmod_files > 0 || stats.fchown_files > 0 || stats.utimensat_files > 0 {
            report.push_str(&format!(
                "| 📄 Files | {} | {} | {} |\n",
                stats.fchmod_files, stats.fchown_files, stats.utimensat_files
            ));
        }

        if stats.fchmod_dirs > 0 || stats.fchown_dirs > 0 || stats.utimensat_dirs > 0 {
            report.push_str(&format!(
                "| 📁 Directories | {} | {} | {} |\n",
                stats.fchmod_dirs, stats.fchown_dirs, stats.utimensat_dirs
            ));
        }

        if stats.utimensat_symlinks > 0 {
            report.push_str(&format!(
                "| 🔗 Symlinks | - | - | {} |\n",
                stats.utimensat_symlinks
            ));
        }

        // Show unidentified if any
        let unidentified_fchmod = stats.fchmod.saturating_sub(identified_fchmod);
        let unidentified_fchown = stats.fchown.saturating_sub(identified_fchown);
        let unidentified_utimens = stats.utimensat_total.saturating_sub(identified_utimens);

        if unidentified_fchmod > 0 || unidentified_fchown > 0 || unidentified_utimens > 0 {
            report.push_str(&format!(
                "| ❓ Unidentified | {} | {} | {} |\n",
                unidentified_fchmod, unidentified_fchown, unidentified_utimens
            ));
        }

        report.push_str("\n");

        if unidentified_fchmod > 0 || unidentified_fchown > 0 || unidentified_utimens > 0 {
            report.push_str(
                "> **Note:** Unidentified operations couldn't be categorized by file type \
                (ambiguous mode bits or incomplete strace data).\n\n",
            );
        }
    }
}

fn add_directory_operations_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 📁 Directory Creation\n\n");

    report.push_str(&format!(
        "| Operation | Count | Type |\n\
        |-----------|-------|------|\n\
        | mkdir | {} | Path-based |\n\
        | mkdirat | {} | FD-based |\n\
        | **Total directory creates** | **{}** | |\n\n",
        stats.mkdir_total,
        stats.mkdirat_total,
        stats.mkdir_total + stats.mkdirat_total
    ));

    let total_mkdir = stats.mkdir_total + stats.mkdirat_total;
    if total_mkdir > 0 {
        let fd_percentage = (stats.mkdirat_total as f64 / total_mkdir as f64 * 100.0) as usize;
        if stats.mkdirat_total == total_mkdir {
            report.push_str("✅ **EXCELLENT:** 100% FD-based directory creation (TOCTOU-safe)\n\n");
        } else if fd_percentage >= 80 {
            report.push_str(&format!(
                "✅ **GOOD:** {}% FD-based directory creation\n\n",
                fd_percentage
            ));
        } else {
            report.push_str(&format!(
                "⚠️  **WARNING:** Only {}% FD-based directory creation (TOCTOU risk)\n\n",
                fd_percentage
            ));
        }
    }
}

fn add_symlink_operations_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 🔗 Symlink Operations\n\n");

    report.push_str(&format!(
        "| Operation | Count | Type |\n\
        |-----------|-------|------|\n\
        | symlink | {} | Path-based |\n\
        | symlinkat | {} | FD-based |\n\
        | readlink | {} | Path-based |\n\
        | readlinkat | {} | FD-based |\n\
        | lstat | {} | Path-based (symlink metadata) |\n\n",
        stats.symlink_total,
        stats.symlinkat_total,
        stats.readlink_total,
        stats.readlinkat_total,
        stats.lstat_total
    ));

    let total_symlink_ops =
        stats.symlink_total + stats.symlinkat_total + stats.readlink_total + stats.readlinkat_total;

    if total_symlink_ops == 0 {
        report.push_str("ℹ️  **INFO:** No symlink operations detected in test\n\n");
    } else {
        let fd_based = stats.symlinkat_total + stats.readlinkat_total;
        let fd_percentage = (fd_based as f64 / total_symlink_ops as f64 * 100.0) as usize;

        if fd_percentage == 100 {
            report.push_str("✅ **EXCELLENT:** 100% FD-based symlink operations (TOCTOU-safe)\n\n");
        } else if fd_percentage >= 80 {
            report.push_str(&format!(
                "✅ **GOOD:** {}% FD-based symlink operations\n\n",
                fd_percentage
            ));
        } else {
            report.push_str(&format!(
                "⚠️  **WARNING:** Only {}% FD-based symlink operations (TOCTOU risk)\n\n",
                fd_percentage
            ));
        }
    }
}

fn add_unexpected_syscalls_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## ⚠️  Unexpected/Legacy Syscalls\n\n");

    let mut unexpected = Vec::new();

    if stats.open_total > 0 {
        unexpected.push(format!(
            "- `open()`: {} calls (use `openat()` instead)",
            stats.open_total
        ));
    }
    if stats.stat_total > 0 {
        unexpected.push(format!(
            "- `stat()`: {} calls (use `statx()` or `fstat()` instead)",
            stats.stat_total
        ));
    }
    if stats.chmod_total > 0 {
        unexpected.push(format!(
            "- `chmod()`: {} calls (use `fchmod()` instead)",
            stats.chmod_total
        ));
    }
    if stats.chown_total > 0 {
        unexpected.push(format!(
            "- `chown()`: {} calls (use `fchown()` instead)",
            stats.chown_total
        ));
    }
    if stats.utime_total > 0 {
        unexpected.push(format!(
            "- `utime()`: {} calls (use `utimensat()` instead)",
            stats.utime_total
        ));
    }
    if stats.utimes_total > 0 {
        unexpected.push(format!(
            "- `utimes()`: {} calls (use `utimensat()` instead)",
            stats.utimes_total
        ));
    }
    if stats.access_total > 0 {
        unexpected.push(format!(
            "- `access()`: {} calls (TOCTOU-vulnerable, avoid)",
            stats.access_total
        ));
    }
    if stats.creat_total > 0 {
        unexpected.push(format!(
            "- `creat()`: {} calls (deprecated, use `openat()` instead)",
            stats.creat_total
        ));
    }

    // Synchronous I/O (should be minimal with io_uring)
    // Note: read/write on low FDs are typically eventfd operations for thread sync (expected)
    // Only flag if we see them on high FDs (actual file I/O) or pread/pwrite
    if stats.pread_total > 0 {
        unexpected.push(format!(
            "- `pread()`: {} calls (use io_uring `read_at` instead)",
            stats.pread_total
        ));
    }
    if stats.pwrite_total > 0 {
        unexpected.push(format!(
            "- `pwrite()`: {} calls (use io_uring `write_at` instead)",
            stats.pwrite_total
        ));
    }

    // If we see a lot of read/write on high FDs, that might indicate file I/O not via io_uring
    // For now, read/write on low FDs (< 100) are considered normal (eventfd, pipes, etc.)

    if unexpected.is_empty() {
        report.push_str("✅ **EXCELLENT:** No unexpected or legacy syscalls detected!\n\n");
    } else {
        report.push_str("**Found unexpected syscalls:**\n\n");
        for item in unexpected {
            report.push_str(&format!("{}\n", item));
        }
        report
            .push_str("\n> These syscalls indicate potential performance or security issues.\n\n");
    }
}

fn add_all_syscalls_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 📊 All Syscalls (Complete Inventory)\n\n");

    if stats.all_syscalls.is_empty() {
        report.push_str("ℹ️  **INFO:** No syscalls parsed (trace may be empty)\n\n");
        return;
    }

    // Sort by count (descending)
    let mut syscalls: Vec<_> = stats.all_syscalls.iter().collect();
    syscalls.sort_by(|a, b| b.1.cmp(a.1));

    // Known/expected syscalls to categorize
    let known_io_uring = ["io_uring_setup", "io_uring_enter", "io_uring_register"];
    let known_file_ops = [
        "openat",
        "statx",
        "fallocate",
        "read",
        "write",
        "pread64",
        "pwrite64",
        "close",
        "lseek",
    ];
    let known_metadata = ["fchmod", "fchown", "utimensat", "fstat", "lstat"];
    let known_dir = ["mkdir", "mkdirat", "getdents64", "getcwd", "chdir"];
    let known_symlink = ["symlink", "symlinkat", "readlink", "readlinkat"];
    let known_process = ["clone3", "execve", "wait4", "exit", "exit_group"];
    let known_memory = ["mmap", "munmap", "mprotect", "brk", "madvise"];
    let known_thread = ["futex", "set_robust_list", "set_tid_address"];
    let known_signal = ["rt_sigaction", "rt_sigprocmask", "sigaltstack", "rseq"];
    let known_misc = [
        "getrandom",
        "sched_getaffinity",
        "prlimit64",
        "arch_prctl",
        "poll",
        "clock_gettime",
        "clock_nanosleep",
        "eventfd2",
        "sched_yield",
        "prctl",
    ];

    report.push_str("| Syscall | Count | Category |\n");
    report.push_str("|---------|-------|----------|\n");

    for (syscall, count) in &syscalls {
        let category = if known_io_uring.contains(&syscall.as_str()) {
            "🔄 io_uring"
        } else if known_file_ops.contains(&syscall.as_str()) {
            "📁 File I/O"
        } else if known_metadata.contains(&syscall.as_str()) {
            "📋 Metadata"
        } else if known_dir.contains(&syscall.as_str()) {
            "📂 Directory"
        } else if known_symlink.contains(&syscall.as_str()) {
            "🔗 Symlink"
        } else if known_process.contains(&syscall.as_str()) {
            "⚙️  Process"
        } else if known_memory.contains(&syscall.as_str()) {
            "💾 Memory"
        } else if known_thread.contains(&syscall.as_str()) {
            "🧵 Threading"
        } else if known_signal.contains(&syscall.as_str()) {
            "🚦 Signal"
        } else if known_misc.contains(&syscall.as_str()) {
            "🔧 System"
        } else {
            "❓ **Unknown**"
        };

        report.push_str(&format!("| `{}` | {} | {} |\n", syscall, count, category));
    }

    report.push_str("\n");

    // Highlight unknown syscalls
    let unknown: Vec<_> = syscalls
        .iter()
        .filter(|(name, _)| {
            !known_io_uring.contains(&name.as_str())
                && !known_file_ops.contains(&name.as_str())
                && !known_metadata.contains(&name.as_str())
                && !known_dir.contains(&name.as_str())
                && !known_symlink.contains(&name.as_str())
                && !known_process.contains(&name.as_str())
                && !known_memory.contains(&name.as_str())
                && !known_thread.contains(&name.as_str())
                && !known_signal.contains(&name.as_str())
                && !known_misc.contains(&name.as_str())
        })
        .collect();

    if !unknown.is_empty() {
        report.push_str("### ❓ Unknown/Uncategorized Syscalls\n\n");
        for (syscall, count) in unknown {
            report.push_str(&format!("- **`{}`**: {} calls\n", syscall, count));
        }
        report.push_str("\n> These syscalls are not in our expected categories. Review to ensure they're intentional.\n\n");
    }
}

fn add_directory_section(report: &mut String, stats: &SyscallStats, src: &PathBuf, dst: &PathBuf) {
    report.push_str("## 📂 Directory Traversal Details\n\n");

    report.push_str(&format!(
        "**Source directory** (`{}`):\n\
        - statx: {}\n\
        - openat (O_DIRECTORY): {}\n\
        - getdents64 (directory reads): {}\n\n",
        src.display(),
        stats.dir_stats.src_statx,
        stats.dir_stats.src_openat,
        stats.dir_stats.getdents
    ));

    report.push_str(&format!(
        "**Destination directory** (`{}`):\n\
        - fchmod: {} (includes files)\n\
        - fchown: {} (includes files)\n\n",
        dst.display(),
        stats.dir_stats.dst_fchmod,
        stats.dir_stats.dst_fchown
    ));
}

fn add_security_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 🔐 Security Assessment\n\n");

    let total_ops = stats.statx_total + stats.openat_total + stats.utimensat_total;
    let vulnerable_ops = stats.statx_path_based + stats.utimensat_path_based;
    let security_score = if total_ops > 0 {
        ((total_ops - vulnerable_ops) as f64 / total_ops as f64 * 100.0) as usize
    } else {
        100
    };

    report.push_str(&format!(
        "**Security Score:** {}/100 {}\n\n",
        security_score,
        match security_score {
            95..=100 => "🟢 Excellent",
            80..=94 => "🟡 Good",
            60..=79 => "🟠 Fair",
            _ => "🔴 Poor",
        }
    ));

    if stats.statx_path_based > 0 {
        report.push_str("⚠️  Path-based statx: TOCTOU risk\n");
    }
    if stats.utimensat_path_based > 0 {
        report.push_str("⚠️  Path-based utimensat: TOCTOU risk\n");
    }
    if vulnerable_ops == 0 {
        report.push_str("✅  100% FD-based operations: TOCTOU-safe\n");
    }
    report.push_str("\n");
}

fn add_recommendations_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 💡 Recommendations\n\n");

    let mut has_recommendations = false;

    if stats.statx_total > num_files * 2 {
        report.push_str(&format!(
            "- **Reduce redundant statx calls** (currently ~{:.1} per file)\n\
            - Target: 1 statx per file via `DirectoryFd::statx()`\n\n",
            stats.statx_total as f64 / num_files as f64
        ));
        has_recommendations = true;
    }

    if stats.statx_path_based > 0 || stats.openat_path_based > num_files {
        report.push_str(
            "- **Use dirfd-relative operations** instead of `AT_FDCWD` + absolute paths\n\
            - Benefits: TOCTOU-safe, potentially async via io_uring\n\n",
        );
        has_recommendations = true;
    }

    if !stats.io_uring_batch_sizes.is_empty() {
        let avg: f64 = stats.io_uring_batch_sizes.iter().sum::<usize>() as f64
            / stats.io_uring_batch_sizes.len() as f64;
        if avg < 2.0 {
            report.push_str(
                "- **Improve io_uring batching** (currently low batching efficiency)\n\
                - Target: Average batch size ≥2 ops/submit\n\n",
            );
            has_recommendations = true;
        }
    }

    if !has_recommendations {
        report.push_str("✅ No major issues detected. System is well-optimized!\n\n");
    }
}

fn add_summary_table(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📊 Summary Table\n\n");
    report.push_str("| Operation | Count | Target | Status |\n");
    report.push_str("|-----------|-------|--------|--------|\n");

    // io_uring metrics
    report.push_str(&format!(
        "| io_uring_enter | {} | >100 | {} |\n",
        stats.io_uring_enter,
        if stats.io_uring_enter > 100 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    // Show io_uring operations if available
    if !stats.io_uring_ops.is_empty() {
        let total_ops: usize = stats.io_uring_ops.values().sum();
        report.push_str(&format!(
            "| **io_uring ops submitted** | **{}** | | |\n",
            total_ops
        ));

        // Show top operations
        let mut ops: Vec<_> = stats.io_uring_ops.iter().collect();
        ops.sort_by(|a, b| b.1.cmp(a.1));
        for (op, count) in ops.iter().take(5) {
            report.push_str(&format!("| └─ {} | {} | | |\n", op, count));
        }
    }

    report.push_str(&format!(
        "| statx (total) | {} | <{} | {} |\n",
        stats.statx_total,
        num_files * 2,
        if stats.statx_total < num_files * 2 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| statx (path-based) | {} | =0 | {} |\n",
        stats.statx_path_based,
        if stats.statx_path_based == 0 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| openat (user files) | {} | <{} | {} |\n",
        stats.openat_path_based,
        num_files * 4,
        if stats.openat_path_based < num_files * 4 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| fallocate (direct) | {} | =0 | {} |\n",
        stats.fallocate,
        if stats.fallocate == 0 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    report.push_str(&format!(
        "| utimensat (path-based) | {} | =0 | {} |\n",
        stats.utimensat_path_based,
        if stats.utimensat_path_based == 0 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    report.push_str(&format!(
        "| utimensat (FD-based) | {} | ={} | {} |\n\n",
        stats.utimensat_fd_based,
        num_files,
        if stats.utimensat_fd_based == num_files {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));
}

fn determine_exit_code(stats: &SyscallStats, num_files: usize) -> ExitCode {
    // Critical failures
    if stats.io_uring_enter < 100 {
        return ExitCode::Failure;
    }
    if stats.fallocate > 0 {
        return ExitCode::Failure;
    }
    if stats.utimensat_path_based > 0 {
        return ExitCode::Failure;
    }

    // Warnings (but don't fail CI)
    if stats.statx_path_based > num_files * 2 {
        return ExitCode::Warning;
    }
    if stats.openat_path_based > num_files * 4 {
        return ExitCode::Warning;
    }

    ExitCode::Success
}
