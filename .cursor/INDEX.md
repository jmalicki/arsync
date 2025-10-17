# Cursor Commands Index

Complete reference for all custom Cursor slash commands in this project.

## 🆕 New Commands (This Session)

### `/restack` - Break Down Large PRs
Transform a massive PR into a stack of reviewable, dependent PRs.

- **File:** [restack.md](./restack.md) (467 lines)
- **Quick Ref:** [../RESTACK_QUICKREF.md](../RESTACK_QUICKREF.md) (220 lines)
- **When:** PR has >20 commits or >50 files
- **Result:** Multiple small PRs that build on each other

### Enhanced: `/branch` 
Updated documentation for creating branches from remote base.

- **File:** [branch.md](./branch.md) (75 lines)
- **Enhancement:** Better integration with `/restack` workflow

## 📚 Documentation Added

### Workflow Guide
Complete end-to-end examples showing how commands work together.

- **File:** [WORKFLOW_GUIDE.md](./WORKFLOW_GUIDE.md) (393 lines)
- **Covers:** Simple features, restacking, mid-development corrections
- **Includes:** Timelines, decision trees, best practices

### Updated Index
Enhanced main README with restacking workflow.

- **File:** [../README_COMMANDS.md](../README_COMMANDS.md) (187 lines)
- **Sections:** Command overview, naming conventions, workflows, tips

## 📋 Existing Commands Reference

### Branch & PR Management
- `/branch` - Create branch from remote base
- `/pr` - Create pull request
- `/pr-ready` - Prepare PR for submission
- `/pr-checks` - Monitor CI and auto-fix issues
- **🆕 `/restack`** - Break down large PRs into stack

### Development
- `/build` - Build the project
- `/test` - Run tests
- `/bench` - Run benchmarks
- `/smoke` - Quick smoke tests
- `/clean` - Clean build artifacts

### Code Quality
- `/fmt` - Format code
- `/clippy` - Run clippy lints
- `/commit` - Create conventional commits
- `/review` - Review code changes

### Planning & Design
- `/plan` - Create implementation plan
- `/design` - Design architecture
- `/implement` - Implement planned features
- `/debug` - Debug issues

### Documentation
- `/docs` - Generate documentation

### CI/CD
- `/ci-latest` - Check latest CI status
- `/workflow-audit` - Audit GitHub workflows
- `/release-check` - Pre-release validation

## 🔄 Command Relationships

### Linear Workflow
```
/branch → code → /pr → /pr-checks
```

### Restacking Workflow
```
/restack
  ↓
Creates multiple branches using /branch
  ↓
Creates PRs using /pr
  ↓
Validates each with /pr-checks
```

### Quality Workflow
```
/fmt → /clippy → /test → /commit → /pr
```

## 📖 Documentation Structure

```
.cursor/
├── README_COMMANDS.md              - Overview and quick start
├── RESTACK_QUICKREF.md             - Quick reference card
└── commands/
    ├── INDEX.md (this file)        - Complete command index
    ├── WORKFLOW_GUIDE.md           - End-to-end examples
    │
├── branch.md                       - /branch command
├── restack.md                      - /restack command (NEW)
├── pr.md                          - /pr command
├── pr-ready.md                    - /pr-ready command
├── pr-checks.md                   - /pr-checks command
│
├── build.md                       - /build command
├── test.md                        - /test command
├── bench.md                       - /bench command
├── smoke.md                       - /smoke command
├── clean.md                       - /clean command
│
├── fmt.md                         - /fmt command
├── clippy.md                      - /clippy command
├── commit.md                      - /commit command
├── review.md                      - /review command
│
├── plan.md                        - /plan command
├── design.md                      - /design command
├── implement.md                   - /implement command
├── debug.md                       - /debug command
│
├── docs.md                        - /docs command
├── ci-latest.md                   - /ci-latest command
├── workflow-audit.md              - /workflow-audit command
└── release-check.md               - /release-check command
```

## 🎯 Quick Command Reference

| Task | Command | Doc |
|------|---------|-----|
| Start new feature | `/branch "area/feat-name"` | [branch.md](./branch.md) |
| Break down large PR | `/restack` | [restack.md](./restack.md) |
| Create PR | `/pr` | [pr.md](./pr.md) |
| Monitor CI | `/pr-checks` | [pr-checks.md](./pr-checks.md) |
| Format code | `/fmt` | [fmt.md](./fmt.md) |
| Run lints | `/clippy` | [clippy.md](./clippy.md) |
| Run tests | `/test` | [test.md](./test.md) |
| Conventional commit | `/commit "message"` | [commit.md](./commit.md) |
| Debug issue | `/debug "description"` | [debug.md](./debug.md) |
| Plan feature | `/plan` | [plan.md](./plan.md) |

## 📏 Command Statistics

| Category | Count |
|----------|-------|
| Branch/PR Management | 5 |
| Development Tools | 5 |
| Code Quality | 4 |
| Planning/Design | 4 |
| Documentation | 4 |
| CI/CD | 3 |
| **Total Commands** | **25** |

## 🚀 Getting Started

### For Simple Features
1. Read: [../README_COMMANDS.md](../README_COMMANDS.md) - Overview
2. Start: `/branch "area/feat-name"`
3. Work: Code, test, commit
4. Submit: `/pr`
5. Monitor: `/pr-checks`

### For Large PRs
1. Read: [../RESTACK_QUICKREF.md](../RESTACK_QUICKREF.md) - Quick guide
2. Start: `/restack`
3. Follow: AI guides you through the process
4. Result: Multiple small PRs

### For Complete Understanding
Read: [WORKFLOW_GUIDE.md](./WORKFLOW_GUIDE.md) - Complete examples

## 💡 Tips

### Command Discovery
- Type `/` in Cursor to see all available commands
- Each command has detailed documentation in its `.md` file
- Start with README.md for overview

### Best Practices
- Use `/branch` to start all new work (never checkout main locally)
- Run `/pr-checks` after every PR (auto-fixes CI issues)
- Use `/restack` early when PR grows (don't wait until 200 files)
- Follow naming conventions (see ../README_COMMANDS.md)

### Learning Path
1. **Basic:** `/branch` → `/pr` → `/pr-checks`
2. **Intermediate:** Add `/fmt`, `/clippy`, `/test`
3. **Advanced:** Use `/restack` for large changes
4. **Expert:** Combine with `/plan`, `/design`, `/implement`

## 🔍 Finding Information

### "How do I...?"
- Create a branch? → [branch.md](./branch.md)
- Break down large PR? → [restack.md](./restack.md)
- Fix CI issues? → [pr-checks.md](./pr-checks.md)
- See full workflow? → [WORKFLOW_GUIDE.md](./WORKFLOW_GUIDE.md)

### "What does...?"
- `/restack` do? → [../RESTACK_QUICKREF.md](../RESTACK_QUICKREF.md)
- Each command do? → [../README_COMMANDS.md](../README_COMMANDS.md)
- The workflow look like? → [WORKFLOW_GUIDE.md](./WORKFLOW_GUIDE.md)

### "When should I...?"
- Use `/restack`? → When PR >20 commits or >50 files
- Use `/branch`? → Always, for any new work
- Use `/pr-checks`? → After every PR creation

## 📝 Notes

- All commands are project-specific to this workspace
- Commands are defined in `.cursor/commands/*.md`
- Cursor automatically discovers and loads them
- You can add custom commands by creating new `.md` files

## 🔗 Related Resources

### External
- [Conventional Commits](https://www.conventionalcommits.org/)
- [GitHub CLI](https://cli.github.com/)
- [Git Branching Model](https://nvie.com/posts/a-successful-git-branching-model/)

### Internal Project Docs
- [DEVELOPER.md](../../docs/DEVELOPER.md) - Developer guide
- [CONTRIBUTING.md](../../CONTRIBUTING.md) - Contribution guidelines
- [README.md](../../README.md) - Project README

## 📊 Command Usage Frequency

Based on typical development workflow:

| Frequency | Commands |
|-----------|----------|
| **Every feature** | `/branch`, `/pr`, `/pr-checks` |
| **Daily** | `/test`, `/fmt`, `/clippy`, `/commit` |
| **Weekly** | `/restack`, `/bench`, `/review` |
| **Monthly** | `/release-check`, `/workflow-audit` |
| **As needed** | `/debug`, `/plan`, `/design`, `/implement` |

---

## 🆕 What's New in This Session

### Created
- ✨ `/restack` command (467 lines)
- 📖 Quick reference card (220 lines)
- 📚 Complete workflow guide (393 lines)
- 📇 This index (you're reading it!)

### Enhanced
- 🔧 `/branch` documentation
- 📋 Main README with restack workflow
- 🔗 Command cross-references

### Total Added
- **4 new files**
- **~1,300 lines of documentation**
- **Integration of restacking workflow**

---

**Last Updated:** October 16, 2025 (Restack Feature Addition)

**Quick Links:**
- [Start Here](../README_COMMANDS.md)
- [Restack Guide](./restack.md)
- [Quick Ref](../RESTACK_QUICKREF.md)
- [Full Workflow](./WORKFLOW_GUIDE.md)

