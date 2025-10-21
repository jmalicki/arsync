//! Common test argument builders for use across test files

use arsync::cli::{
    Args, ConcurrencyConfig, CopyMethod, IoConfig, MetadataConfig, OutputConfig, PathConfig,
};
use std::path::PathBuf;

/// Create minimal test Args for basic testing
pub fn create_minimal_test_args() -> Args {
    Args {
        paths: PathConfig {
            source: PathBuf::from("/test/source"),
            destination: PathBuf::from("/test/dest"),
        },
        io: IoConfig {
            queue_depth: 4096,
            buffer_size_kb: 64,
            copy_method: CopyMethod::Auto,
            cpu_count: 1,
            parallel: super::disabled_parallel_config(),
        },
        concurrency: ConcurrencyConfig {
            max_files_in_flight: 1024,
            no_adaptive_concurrency: false,
        },
        metadata: MetadataConfig {
            archive: false,
            recursive: false,
            links: false,
            perms: false,
            times: false,
            group: false,
            owner: false,
            devices: false,
            xattrs: false,
            acls: false,
            fsync: false,
            hard_links: false,
            atimes: false,
            crtimes: false,
            preserve_xattr: false,
            preserve_acl: false,
        },
        output: OutputConfig {
            dry_run: false,
            progress: false,
            verbose: 0,
            quiet: false,
            pirate: false,
        },
    }
}

/// Create test Args with archive mode enabled (full metadata preservation)
#[allow(dead_code)]
pub fn create_archive_test_args() -> Args {
    let mut args = create_minimal_test_args();
    args.metadata.archive = true;
    args
}

/// Builder for test Args with fluent API
#[allow(dead_code)]
pub struct ArgsBuilder {
    args: Args,
}

#[allow(dead_code)]
impl ArgsBuilder {
    pub fn new() -> Self {
        Self {
            args: create_minimal_test_args(),
        }
    }

    pub fn source(mut self, path: PathBuf) -> Self {
        self.args.paths.source = path;
        self
    }

    pub fn destination(mut self, path: PathBuf) -> Self {
        self.args.paths.destination = path;
        self
    }

    pub fn archive(mut self, enabled: bool) -> Self {
        self.args.metadata.archive = enabled;
        self
    }

    pub fn recursive(mut self, enabled: bool) -> Self {
        self.args.metadata.recursive = enabled;
        self
    }

    pub fn perms(mut self, enabled: bool) -> Self {
        self.args.metadata.perms = enabled;
        self
    }

    pub fn times(mut self, enabled: bool) -> Self {
        self.args.metadata.times = enabled;
        self
    }

    pub fn owner(mut self, enabled: bool) -> Self {
        self.args.metadata.owner = enabled;
        self
    }

    pub fn group(mut self, enabled: bool) -> Self {
        self.args.metadata.group = enabled;
        self
    }

    pub fn xattrs(mut self, enabled: bool) -> Self {
        self.args.metadata.xattrs = enabled;
        self
    }

    pub fn hard_links(mut self, enabled: bool) -> Self {
        self.args.metadata.hard_links = enabled;
        self
    }

    pub fn verbose(mut self, level: u8) -> Self {
        self.args.output.verbose = level;
        self
    }

    pub fn progress(mut self, enabled: bool) -> Self {
        self.args.output.progress = enabled;
        self
    }

    pub fn build(self) -> Args {
        self.args
    }
}

impl Default for ArgsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
