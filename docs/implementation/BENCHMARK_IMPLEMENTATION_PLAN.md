# Benchmark Implementation Plan

**Branch**: `benchmarks/initial-run` (stacked on `fix/remove-unverified-benchmark-claims`)

**Goal**: Run benchmarks, collect verified data, update README.md with real numbers

---

## Prerequisites Check

**Before starting, verify**:
1. ✅ Two NVMe RAID arrays available
   - Source array: Has space for ~1TB test data
   - Dest array: Can be wiped between tests
2. ✅ Arrays capable of >15 GB/s
3. ✅ Root access available
4. ✅ No other I/O workloads running
5. ✅ Python 3 with scipy, numpy installed
6. ✅ rsync installed (for comparison)
7. ✅ GNU parallel installed (optional, for parallel rsync tests)

---

## Step-by-Step Execution Plan

### Phase 1: Setup & Validation (30 min)

#### Step 1.1: Verify System State
```bash
# Check array health
cat /proc/mdstat  # Should show no rebuild/resync

# Check available space
df -h /mnt/source-nvme
df -h /mnt/dest-nvme

# Verify arrays are actually NVMe RAID
lsblk
sudo hdparm -t /dev/md0  # Or relevant device
```

**Validation**: Arrays healthy, sufficient space, no I/O activity

#### Step 1.2: Build arsync in Release Mode
```bash
cd /home/jmalicki/src/io_uring_sync
cargo build --release

# Verify binary
ls -lh target/release/arsync
./target/release/arsync --help
```

**Validation**: Binary exists, runs, shows help

#### Step 1.3: Install Python Dependencies
```bash
pip3 install numpy scipy pandas  # pandas optional but helpful
```

**Validation**: `python3 -c "import scipy, numpy"`

---

### Phase 2: Generate Test Data (~2-3 hours)

#### Step 2.1: Run Test Data Generator
```bash
sudo ./benchmarks/generate_testdata.sh /mnt/source-nvme

# Monitor progress (in another terminal)
watch -n 5 "du -sh /mnt/source-nvme/benchmark-data/*"
```

**Expected output**:
- Large files: 100GB, 200GB, 500GB
- Small files: Multiple directories with 10K-1M files
- Total: ~1TB

**Validation**: 
- All directories created
- File counts match expected
- No errors in output

#### Step 2.2: Verify Test Data Integrity
```bash
# Count files
find /mnt/source-nvme/benchmark-data -type f | wc -l

# Check sizes
du -sh /mnt/source-nvme/benchmark-data/*

# Verify large files exist
ls -lh /mnt/source-nvme/benchmark-data/single-large-files/
```

**Validation**: Files exist, sizes reasonable, no corruption

---

### Phase 3: Run Quick Smoke Test (15 min)

**Before full benchmark, test a subset to catch issues early**

#### Step 3.1: Test Single Scenario
```bash
# Set environment
export RSYNC_BIN=$(which rsync)
export ARSYNC_BIN=/home/jmalicki/src/io_uring_sync/target/release/arsync

# Manual test: Copy 10K small files
echo "Testing rsync..."
sudo sync && sudo sh -c 'echo 3 > /proc/sys/vm/drop_caches'
time sudo rsync -a /mnt/source-nvme/benchmark-data/tiny-files-10k/ /mnt/dest-nvme/test-rsync/

echo "Testing arsync..."
sudo rm -rf /mnt/dest-nvme/test-rsync/
sudo sync && sudo sh -c 'echo 3 > /proc/sys/vm/drop_caches'
time sudo $ARSYNC_BIN -a --source /mnt/source-nvme/benchmark-data/tiny-files-10k/ --destination /mnt/dest-nvme/test-arsync/

# Verify identical output
diff -r /mnt/dest-nvme/test-rsync/ /mnt/dest-nvme/test-arsync/
```

**Validation**:
- Both tools complete successfully
- Outputs are identical (diff shows nothing)
- arsync should be faster
- No errors

**If smoke test fails, stop and debug before full benchmark!**

---

### Phase 4: Run Full Benchmark Suite (4-6 hours)

#### Step 4.1: Prepare System
```bash
# Disable background services
sudo systemctl stop unattended-upgrades packagekit

# Set CPU governor to performance
for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
    echo performance | sudo tee $cpu
done

# Verify no other I/O
iostat -x 1 2
```

#### Step 4.2: Run Benchmark Suite
```bash
# Create results directory with timestamp
RESULTS_DIR=/home/jmalicki/src/io_uring_sync/benchmark-results-$(date +%Y%m%d_%H%M%S)

# Run full suite (4-6 hours)
sudo ./benchmarks/run_benchmarks.sh \
    /mnt/source-nvme/benchmark-data \
    /mnt/dest-nvme/benchmark-output \
    $RESULTS_DIR

# Optional: Run with power monitoring
# ENABLE_POWER_MONITORING=yes sudo ./benchmarks/run_benchmarks.sh ...
```

**What happens**:
- 30+ test scenarios
- 5 runs each (first discarded)
- ~150 individual benchmark runs
- Cache dropped between each run
- iostat monitoring
- Progress shown in real-time

**Monitoring** (in another terminal):
```bash
# Watch progress
watch -n 10 "ls $RESULTS_DIR/ | wc -l"

# Check temperature
watch -n 5 "sensors | grep Core"

# Check system load
htop
```

**Important**: 
- Let it run uninterrupted
- Don't run other workloads
- Monitor for thermal throttling
- Check for disk space

#### Step 4.3: Verify Benchmark Completion
```bash
# Check all tests completed
ls $RESULTS_DIR/

# Should see directories like:
# 01_rsync_100gb/
# 02_arsync_100gb/
# 03_rsync_200gb/
# etc...

# Check for errors
grep -r "ERROR\|error" $RESULTS_DIR/ || echo "No errors found"
```

**Validation**: 
- All test directories exist
- No error messages
- Results files present (elapsed.txt, throughput.txt, etc.)

---

### Phase 5: Analyze Results (15 min)

#### Step 5.1: Run Statistical Analysis
```bash
cd /home/jmalicki/src/io_uring_sync

python3 ./benchmarks/analyze_results.py $RESULTS_DIR
```

**Output**:
- `final_report.txt` - Human-readable summary
- `results.json` - Machine-readable data
- Statistical significance for each test
- README.md template

#### Step 5.2: Review Results
```bash
# View summary
cat $RESULTS_DIR/final_report.txt

# Key sections:
# - Summary table (all scenarios)
# - Detailed analysis (per scenario)
# - Statistical significance (p-values)
# - README.md template
```

#### Step 5.3: Sanity Check Results
```bash
# Verify results make sense
grep -A 20 "## SUMMARY TABLE" $RESULTS_DIR/final_report.txt

# Look for:
# - arsync generally faster than rsync ✓
# - Small files show bigger speedup ✓
# - p-values < 0.05 (statistically significant) ✓
# - No outliers or unexpected results
```

**Red flags to check**:
- arsync slower than rsync → investigate
- Huge variance (CV > 10%) → run again
- p-values > 0.05 → not statistically significant
- Unrealistic speeds (>20 GB/s on 15 GB/s array) → error

---

### Phase 6: Update Documentation (30 min)

#### Step 6.1: Extract Key Numbers
```bash
# README.md needs these key scenarios:
# - 200GB single file
# - 100K × 1KB files (or 10K × 10KB)
# - Deep directory tree
# - Mixed workload (photo library)

# Extract from report
grep -A 1 "200gb\|100k\|10k_small\|photo_library" $RESULTS_DIR/final_report.txt
```

#### Step 6.2: Update README.md

**Find these sections in README.md**:
1. Line ~60: "Real-world impact" (io_uring section)
2. Line ~490: "Performance Characteristics" table
3. Line ~620: "Performance Benchmarks" table

**Replace TBD values with actual numbers from report**

#### Step 6.3: Update Other Claims

**Throughout README.md, replace**:
- "TBD throughput" → actual speedup (e.g., "2.3x faster")
- "TBD% better throughput" → actual percentage (e.g., "18% better")
- "TBD faster on small files" → actual speedup (e.g., "3.1x faster")
- "benchmarks pending" → remove this phrase

**Add benchmark details**:
```markdown
## Performance Benchmarks

Benchmarked on: [Your System Specs]
- CPU: [model, cores]
- RAM: [amount]
- Storage: [NVMe RAID specs]
- OS: Ubuntu XX.XX, Linux kernel X.X
- Date: [YYYY-MM-DD]

| Workload | rsync | arsync | Speedup |
|----------|-------|--------|---------|
| 200 GB file | X.Xs (Y.Y GB/s) | X.Xs (Y.Y GB/s) | X.XXx |
| ... | ... | ... | ... |
```

#### Step 6.4: Add Power Efficiency (if measured)

If you ran with `ENABLE_POWER_MONITORING=yes`:
```markdown
### Energy Efficiency

| Workload | rsync Energy (J) | arsync Energy (J) | Savings |
|----------|-----------------|------------------|---------|
| 100K files | XXXX | XXXX | XX% |
```

---

### Phase 7: Commit & Push (15 min)

#### Step 7.1: Review Changes
```bash
git status
git diff README.md | head -100
```

#### Step 7.2: Commit with Detailed Message
```bash
git add README.md

git commit -m "feat: add verified benchmark results from NVMe RAID testing

Results from comprehensive benchmarking suite on [system specs]:

Performance improvements:
- Large files (200GB): X.XXx speedup
- Small files (100K × 1KB): X.XXx speedup
- Medium files (10K × 10KB): X.XXx speedup
- Hardlink scenarios: X.XXx speedup
- Overall mixed workload: X.XXx speedup

All results statistically significant (p < 0.05).
Complete test methodology documented in docs/BENCHMARKING_PLAN.md.

Test details:
- 30+ scenarios tested
- 5 runs each (first discarded as warm-up)
- Cache dropped between all runs
- CPU governor set to performance
- Total runtime: ~X hours

Raw data available in: [link to results archive if hosted]"
```

#### Step 7.3: Archive Results
```bash
# Create tarball of raw results for future reference
tar czf benchmark-results-$(date +%Y%m%d).tar.gz $RESULTS_DIR/

# Optional: Upload to GitHub releases or store elsewhere
```

#### Step 7.4: Push Branch
```bash
git push -u origin benchmarks/initial-run
```

---

### Phase 8: Create PR (10 min)

#### Step 8.1: Create Pull Request
```bash
gh pr create \
    --base fix/remove-unverified-benchmark-claims \
    --title "feat: add verified benchmark results" \
    --body "Adds verified benchmark results from comprehensive testing suite.

**System Configuration**:
- [List your specs]

**Key Results**:
- Large files: X.XXx faster
- Small files: X.XXx faster
- Statistically significant (p < 0.05)

**Files Changed**:
- README.md: Updated with verified numbers

**Raw Data**:
- Results archived: benchmark-results-YYYYMMDD.tar.gz
- Available on request

Stacked on PR #28"
```

---

## Timeline Summary

| Phase | Duration | Can Run Unattended |
|-------|----------|-------------------|
| 1. Setup & Validation | 30 min | No - requires verification |
| 2. Generate Test Data | 2-3 hours | Yes - can walk away |
| 3. Smoke Test | 15 min | No - need to verify |
| 4. Run Full Benchmarks | 4-6 hours | Yes - monitor occasionally |
| 5. Analyze Results | 15 min | No - need to review |
| 6. Update Docs | 30 min | No - manual editing |
| 7. Commit & Push | 15 min | No - review changes |
| 8. Create PR | 10 min | No - write description |
| **Total** | **8-11 hours** | ~7 hours unattended |

---

## Quick Command Reference

```bash
# Full benchmark run (copy-paste ready)
cd /home/jmalicki/src/io_uring_sync

# 1. Build
cargo build --release

# 2. Generate data (2-3 hours)
sudo ./benchmarks/generate_testdata.sh /mnt/source-nvme

# 3. Run benchmarks (4-6 hours)
sudo ./benchmarks/run_benchmarks.sh \
    /mnt/source-nvme/benchmark-data \
    /mnt/dest-nvme/benchmark-output \
    ./benchmark-results-$(date +%Y%m%d)

# 4. Analyze
python3 ./benchmarks/analyze_results.py ./benchmark-results-*/

# 5. Review
cat ./benchmark-results-*/final_report.txt

# 6. Update README.md (manual)
vim README.md

# 7. Commit
git add README.md
git commit -m "feat: add verified benchmark results"
git push
```

---

## Success Criteria

✅ **Benchmark Successful If**:
1. All 30+ test scenarios complete without errors
2. arsync shows improvement over rsync in most scenarios
3. Results are statistically significant (p < 0.05)
4. Variance is reasonable (CV < 10%)
5. README.md updated with verified numbers
6. Raw data archived for reproducibility

---

## Contingency Plans

### If smoke test fails:
- Check rsync/arsync produce identical output
- Verify permissions (running as root?)
- Check array health
- Review logs for errors

### If benchmarks show unexpected results:
- Check thermal throttling: `sensors`
- Verify cache was dropped: `free -h`
- Check for background I/O: `iostat -x 1`
- Review individual test logs

### If high variance (CV > 10%):
- Run benchmarks again during quieter time
- Check for background services
- Verify RAID not rebuilding
- Consider longer runs (increase file sizes)

### If arsync slower than rsync:
- Verify release build: `ls -l target/release/arsync`
- Check CPU governor: `cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor`
- Review io_uring setup
- This is a RED FLAG - investigate before merging!

---

## Ready to Start?

**Checklist before beginning**:
- [ ] Arrays available and healthy
- [ ] Root access confirmed
- [ ] No other workloads running
- [ ] Have 8-11 hours available (mostly unattended)
- [ ] Python dependencies installed
- [ ] Cargo release build works
- [ ] Smoke test passed

**When ready**:
```bash
# Start with smoke test
cd /home/jmalicki/src/io_uring_sync
./benchmarks/generate_testdata.sh /mnt/source-nvme  # Start here!
```

🚀 **Let's get those verified numbers!**

