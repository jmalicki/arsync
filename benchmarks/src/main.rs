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
    // Directory operations
    mkdir_total: usize,
    mkdirat_total: usize,
    // Symlink operations
    symlink_total: usize,
    symlinkat_total: usize,
    readlink_total: usize,
    readlinkat_total: usize,
    lstat_total: usize,
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
    // Run strace on arsync
    let output = Command::new("strace")
        .arg("-e")
        .arg("trace=all")
        .arg("-f")
        .arg(&args.arsync_bin)
        .arg(&args.test_dir_src)
        .arg(&args.test_dir_dst)
        .arg("-a")
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

    // Parse all syscalls first (to build complete inventory)
    // Match format: "syscall_name(...)" or "PID syscall_name(...)"
    let syscall_re = Regex::new(r"(?:^\d+\s+)?(\w+)\(").unwrap();
    for line in raw_content.lines() {
        // Skip lines that are continuations or don't contain syscalls
        if line.trim_start().starts_with('<') || line.trim_start().starts_with('=') {
            continue;
        }
        if let Some(cap) = syscall_re.captures(line) {
            let syscall_name = cap[1].to_string();
            *stats.all_syscalls.entry(syscall_name).or_insert(0) += 1;
        }
    }

    // Count io_uring operations
    stats.io_uring_setup = raw_content.matches("io_uring_setup").count();
    stats.io_uring_enter = raw_content.matches("io_uring_enter").count();

    // Parse io_uring batch sizes
    let batch_re = Regex::new(r"io_uring_enter\([0-9]+, (\d+),").unwrap();
    for cap in batch_re.captures_iter(raw_content) {
        if let Ok(size) = cap[1].parse::<usize>() {
            stats.io_uring_batch_sizes.push(size);
        }
    }

    // Count statx operations
    stats.statx_total = raw_content.matches("statx(").count();
    stats.statx_path_based = raw_content.matches("statx(AT_FDCWD").count()
        - raw_content.matches("statx(AT_FDCWD, \"\"").count();
    stats.statx_fd_based = raw_content.matches("statx(").count() - stats.statx_path_based;

    // Count openat operations
    stats.openat_total = raw_content.matches("openat(").count();
    let openat_user_re = Regex::new(r#"openat\(AT_FDCWD, "/[^"]*""#).unwrap();
    for line in raw_content.lines() {
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

    // Count other operations
    stats.fallocate = raw_content.matches("fallocate(").count();
    stats.fchmod = raw_content.matches("fchmod").count();
    stats.fchown = raw_content.matches("fchown").count();
    stats.utimensat_total = raw_content.matches("utimensat").count();

    // Count FD-based vs path-based utimensat
    let utimens_fd_re = Regex::new(r"utimensat\(\d+, NULL").unwrap();
    stats.utimensat_fd_based = utimens_fd_re.find_iter(raw_content).count();
    let utimens_path_re = Regex::new(r#"utimensat\(AT_FDCWD, "/"#).unwrap();
    stats.utimensat_path_based = utimens_path_re.find_iter(raw_content).count();

    // Count directory operations
    stats.mkdir_total = raw_content.matches("mkdir(").count();
    stats.mkdirat_total = raw_content.matches("mkdirat(").count();

    // Count symlink operations
    stats.symlink_total = raw_content.matches("symlink(").count();
    stats.symlinkat_total = raw_content.matches("symlinkat(").count();
    stats.readlink_total = raw_content.matches("readlink(").count();
    stats.readlinkat_total = raw_content.matches("readlinkat(").count();
    stats.lstat_total = raw_content.matches("lstat(").count();

    // Count unexpected/legacy syscalls
    stats.open_total = raw_content.matches("open(\"").count(); // Exclude openat
    stats.stat_total = raw_content.matches("stat(\"").count(); // Exclude statx, fstat
    stats.chmod_total = raw_content.matches("chmod(\"").count(); // Exclude fchmod
    stats.chown_total = raw_content.matches("chown(\"").count(); // Exclude fchown
    stats.utime_total = raw_content.matches("utime(").count();
    stats.utimes_total = raw_content.matches("utimes(").count();
    stats.access_total = raw_content.matches("access(").count();
    stats.creat_total = raw_content.matches("creat(").count();

    // Count synchronous read/write (should be rare with io_uring)
    // Use more precise regex to avoid matching pread/pwrite
    let read_re = Regex::new(r"\bread\(").unwrap();
    let write_re = Regex::new(r"\bwrite\(").unwrap();
    stats.read_total = read_re.find_iter(raw_content).count();
    stats.write_total = write_re.find_iter(raw_content).count();
    stats.pread_total = raw_content.matches("pread").count();
    stats.pwrite_total = raw_content.matches("pwrite").count();

    // Per-file breakdown (first 3 files)
    for i in 1..=3.min(args.num_files) {
        let filename = format!("file{}.bin", i);
        let mut file_stats = FileStats::default();

        file_stats.statx = raw_content.matches(&format!("statx.*{}", filename)).count();
        file_stats.openat = raw_content
            .matches(&format!("openat.*{}", filename))
            .count();
        file_stats.mentions = raw_content.matches(&filename).count();

        stats.per_file.insert(filename, file_stats);
    }

    // Directory stats
    let src_path = args.test_dir_src.to_string_lossy();
    stats.dir_stats.src_statx = raw_content
        .matches(&format!("statx.*\"{}\"", src_path))
        .count();
    stats.dir_stats.src_openat = raw_content
        .matches(&format!("openat.*\"{}\".*O_DIRECTORY", src_path))
        .count();
    stats.dir_stats.getdents = raw_content.matches("getdents").count();
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

    // io_uring Usage
    add_io_uring_section(&mut report, stats, args.num_files);

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
    if stats.read_total > 50 {
        unexpected.push(format!(
            "- `read()`: {} calls (high count, should use io_uring)",
            stats.read_total
        ));
    }
    if stats.write_total > 50 {
        unexpected.push(format!(
            "- `write()`: {} calls (high count, should use io_uring)",
            stats.write_total
        ));
    }
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

    report.push_str("<details>\n<summary>Click to expand full syscall list</summary>\n\n");
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

    report.push_str("\n</details>\n\n");

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

    report.push_str(&format!(
        "| io_uring_enter | {} | >100 | {} |\n",
        stats.io_uring_enter,
        if stats.io_uring_enter > 100 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

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
