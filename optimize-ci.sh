#!/bin/bash

# Script to optimize GitHub Actions CI setup
# This script will:
# 1. Disable old CI workflows
# 2. Enable the optimized CI workflow
# 3. Clean up redundant files

set -e

echo "🔧 Optimizing GitHub Actions CI setup..."

# Backup old workflows
echo "📦 Creating backup of old workflows..."
mkdir -p .github/workflows/backup
cp .github/workflows/ci.yml .github/workflows/backup/ci.yml.backup
cp .github/workflows/ci-improved.yml .github/workflows/backup/ci-improved.yml.backup
cp .github/workflows/release.yml .github/workflows/backup/release.yml.backup
cp .github/workflows/release-improved.yml .github/workflows/backup/release-improved.yml.backup

# Disable old workflows by renaming them
echo "🚫 Disabling old CI workflows..."
mv .github/workflows/ci.yml .github/workflows/ci.yml.disabled
mv .github/workflows/ci-improved.yml .github/workflows/ci-improved.yml.disabled

# Enable optimized workflow
echo "✅ Enabling optimized CI workflow..."
mv .github/workflows/ci-optimized.yml .github/workflows/ci.yml

# Create optimized release workflow
echo "📦 Creating optimized release workflow..."
cat > .github/workflows/release.yml << 'EOF'
name: Release (Optimized)

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'  # Match semver tags only

permissions:
  contents: write
  discussions: write

env:
  CARGO_TERM_COLOR: always
  RUSTC_WRAPPER: sccache
  SCCACHE_GHA: true

jobs:
  # Validate the release before building
  validate:
    name: Validate Release
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4
      
      - name: Extract version from tag
        id: version
        run: |
          VERSION=${GITHUB_REF#refs/tags/v}
          echo "version=$VERSION" >> $GITHUB_OUTPUT
          echo "Release version: $VERSION"
      
      - name: Verify Cargo.toml version matches tag
        run: |
          CARGO_VERSION=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
          TAG_VERSION="${{ steps.version.outputs.version }}"
          if [ "$CARGO_VERSION" != "$TAG_VERSION" ]; then
            echo "ERROR: Cargo.toml version ($CARGO_VERSION) doesn't match tag ($TAG_VERSION)"
            exit 1
          fi
          echo "✓ Version validation passed"

  # Build binaries for multiple platforms
  build:
    name: Build - ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    needs: validate
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            asset_name: io-uring-sync-linux-x86_64
            cross: false
          
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            asset_name: io-uring-sync-linux-aarch64
            cross: true
          
          - os: ubuntu-latest
            target: x86_64-unknown-linux-musl
            asset_name: io-uring-sync-linux-x86_64-musl
            cross: false
    
    steps:
    - uses: actions/checkout@v4
    
    - name: Setup Rust
      uses: ./.github/actions/setup-rust-optimized
      with:
        toolchain: stable
    
    - name: Install target
      run: rustup target add ${{ matrix.target }}
    
    - name: Install cross-compilation tools
      if: matrix.cross
      run: |
        sudo apt-get update
        sudo apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu
    
    - name: Install musl tools
      if: contains(matrix.target, 'musl')
      run: |
        sudo apt-get update
        sudo apt-get install -y musl-tools
    
    - name: Configure cross-compilation
      if: matrix.cross
      run: |
        mkdir -p .cargo
        cat >> .cargo/config.toml << EOF
        [target.aarch64-unknown-linux-gnu]
        linker = "aarch64-linux-gnu-gcc"
        EOF
    
    - name: Build release binary
      run: cargo build --release --target ${{ matrix.target }} --locked
      env:
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: aarch64-linux-gnu-gcc
    
    - name: Strip binary
      if: "!matrix.cross"
      run: strip target/${{ matrix.target }}/release/io-uring-sync
    
    - name: Strip binary (cross)
      if: matrix.cross && matrix.target == 'aarch64-unknown-linux-gnu'
      run: aarch64-linux-gnu-strip target/${{ matrix.target }}/release/io-uring-sync
    
    - name: Create archive
      run: |
        cd target/${{ matrix.target }}/release
        tar -czf ../../../${{ matrix.asset_name }}.tar.gz io-uring-sync
        cd ../../..
        
        # Generate checksums
        sha256sum ${{ matrix.asset_name }}.tar.gz > ${{ matrix.asset_name }}.tar.gz.sha256
        
        # Display build info
        ls -lh ${{ matrix.asset_name }}.tar.gz
        cat ${{ matrix.asset_name }}.tar.gz.sha256
    
    - name: Upload build artifact
      uses: actions/upload-artifact@v4
      with:
        name: binary-${{ matrix.target }}
        path: |
          ${{ matrix.asset_name }}.tar.gz
          ${{ matrix.asset_name }}.tar.gz.sha256
        retention-days: 5

  # Create GitHub Release
  release:
    name: Create GitHub Release
    runs-on: ubuntu-latest
    needs: [validate, build]
    steps:
      - uses: actions/checkout@v4
      
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
      
      - name: Prepare release assets
        run: |
          mkdir -p release-assets
          find artifacts -name '*.tar.gz*' -exec cp {} release-assets/ \;
          ls -la release-assets/
      
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          name: Release v${{ needs.validate.outputs.version }}
          files: release-assets/*
          draft: false
          prerelease: ${{ contains(needs.validate.outputs.version, '-') }}
          generate_release_notes: true
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      
      - name: Attest build provenance
        uses: actions/attest-build-provenance@v1
        with:
          subject-path: 'release-assets/*.tar.gz'
EOF

echo "🎉 CI optimization complete!"
echo ""
echo "📋 Summary of changes:"
echo "  ✅ Disabled old ci.yml and ci-improved.yml workflows"
echo "  ✅ Enabled optimized ci.yml workflow with sccache"
echo "  ✅ Created optimized release.yml workflow"
echo "  ✅ Added sccache for faster compilation"
echo "  ✅ Replaced tool compilation with marketplace actions"
echo "  ✅ Enhanced caching strategy"
echo ""
echo "🚀 Next steps:"
echo "  1. Commit these changes"
echo "  2. Push to trigger the new optimized CI"
echo "  3. Monitor build times and performance"
echo "  4. Remove backup files after confirming everything works"
echo ""
echo "📊 Expected improvements:"
echo "  • 30-50% faster compilation with sccache"
echo "  • Reduced CI time by using pre-built tools"
echo "  • Better caching for dependencies and build artifacts"
echo "  • Eliminated duplicate CI runs"