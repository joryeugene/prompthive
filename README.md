# PromptHive 🐝

[![Crates.io](https://img.shields.io/crates/v/prompthive.svg)](https://crates.io/crates/prompthive)
[![Downloads](https://img.shields.io/crates/d/prompthive.svg)](https://crates.io/crates/prompthive)
[![License](https://img.shields.io/crates/l/prompthive.svg)](LICENSE)
[![Documentation](https://img.shields.io/docsrs/prompthive)](https://docs.rs/prompthive)

**Lightning-fast open source** prompt manager for developers. Terminal-native, sub-15ms operations, works with any AI tool. **1000+ organic downloads.**

> **🚀 Website**: [prompthive.sh](https://prompthive.sh) | **🤝 Companion Tool**: [CalmHive](https://calmhive.com) for background AI processing

## TL;DR

```bash
# Install (Latest: v0.2.8)
cargo install prompthive

# Multiple Workflows - Choose Your Style

# 1. Perfect commit messages (PRODUCTION READY ✅)
git diff --staged | ph use essentials/commit              # See prompt output
claude "$(git diff --staged | ph use essentials/commit)"  # Interactive AI

# 2. Instant debugging (BATTLE TESTED ✅)
cat error.log | ph use essentials/debug | llm -p > analysis.md    # Non-interactive
claude "$(cat error.log | ph use essentials/debug)"              # Interactive debug session

# 3. Complete PR workflows
git diff main...HEAD | ph use essentials/review | claude -p | gh pr create --body-file -  # Full automation
```

**Why?** Because copy-pasting from ChatGPT history takes 30+ seconds. PromptHive operations average 8ms.

**Current Version**: 0.2.8 - Open source with community registry, instant authentication, team collaboration, and prompt sharing. All features included, no paid tiers.

> **🚀 Philosophy**: [Terminal-First Development](http://jorypestorious.com/blog/terminal-velocity/) | [Spec-Driven AI Engineering](http://jorypestorious.com/blog/ai-engineer-spec/)
> **📁 Complete Guide**: [prompthive.sh](https://prompthive.sh) | **🤖 Background AI**: [calmhive.com](https://calmhive.com)

**Core Promise**: Every command under 15ms. It's a TOOL, not a LIBRARY.

> **Unix Philosophy**: We don't run AI. We manage prompts perfectly, and pipe them to ANY AI tool you prefer. Like `npm` doesn't run JavaScript, we don't run prompts. We just make them instantly accessible.

```bash
# PromptHive + Your Favorite AI Tools - Three Patterns:

# 1. Command Substitution (Interactive AI):
claude "$(git diff | ph use essentials/review)"          # Interactive session
llm "$(ph use essentials/debug)" error.log              # Chat with context

# 2. Piped with -p flag (Non-interactive/Scripting):
git diff --staged | ph use essentials/commit | claude -p | git commit -F -
cat api.py | ph use essentials/review | llm -p > api-docs.md

# 3. Direct Output (Auto-clipboard):
ph use essentials/debug      # Output + clipboard for pasting into ChatGPT UI
ph use essentials/commit -q  # Quiet mode (no clipboard)
```

## 🚀 Installation

### Cargo (Rust) - Available Now ✅
```bash
# Install from crates.io
cargo install prompthive

# Verify installation
ph --version
```

**Current Status**: Version 0.2.8 is available on crates.io and installs instantly via cargo.

### Shell Completions

After installation, set up shell completions for the best experience:

#### Bash
```bash
ph completion bash > ~/.bash_completion.d/prompthive
source ~/.bash_completion.d/prompthive
```

#### Zsh
```bash
ph completion zsh > ~/.zsh_completions/_prompthive
# Add to ~/.zshrc: fpath=(~/.zsh_completions $fpath)
```

#### Fish
```bash
ph completion fish > ~/.config/fish/completions/ph.fish
```

## Quickstart

```bash
# Use built-in essentials (ready to go!)
git diff --staged | ph use essentials/commit | llm   # Perfect commit messages
ph use essentials/debug "Fix auth timeout error" | claude # Analyze any error

# Use immediately with real workflows
git diff main...HEAD | ph use essentials/review | llm     # Generate PR descriptions
cat main.py | ph use essentials/review | llm    # Review any code

# Use with ANY AI tool (copies to clipboard if terminal, pipes if piped)
ph use essentials/debug "Fix auth timeout" | llm     # LLM by Simon Willison
git diff --staged | ph use essentials/commit | aichat    # AIChat
git diff main...HEAD | ph use essentials/review | claude  # Interactive PR description
cat legacy-code.js | ph use 10x/refactor | mods    # Mods
cat api.py | ph use essentials/review | sgpt      # Shell GPT

# Lightning-fast operations
ph ls                                      # List all prompts
ph f commit                                # Fuzzy search essentials
ph u essentials/debug-error                # Short aliases work

# Share and organize with banks
ph bank publish my-workflow-prompts
# Create custom banks from your favorite prompts
mkdir -p ~/.prompthive/banks/team
cp ~/.prompthive/prompts/*.md ~/.prompthive/banks/team/
```

## "But Why Not Just..."

*1000+ developers chose PromptHive. Here's why existing alternatives don't scale:*

### ...use text files in ~/prompts/?
**Reality check**: You tried this. It works until you have 20+ prompts, then:
- Can't fuzzy search across them in 8ms (try `find` + `grep`—it's slow)
- No smart matching (`ph u au` → `auth-basic` instantly)
- No metadata, descriptions, or organization
- No team sharing or version control
- **Biggest issue**: You stop using them because access isn't instant

### ...use shell aliases?
```bash
# This worked for 5 prompts. What about 50?
alias debug="echo 'Debug this error and suggest a fix'"
alias commit="echo 'Generate a commit message from the diff'"
alias review="echo 'Review this code for issues'"
alias api="echo 'Design a REST API for...'"
# Your .bashrc becomes unmaintainable

# vs PromptHive:
ph f debug   # Fuzzy finds essentials/debug-error in 8ms
ph f api     # Finds all API-related prompts instantly
```

### ...use Neovim with snippets?
**For developers who live in Neovim**: You *could* build this with:
- Snippet plugins (20 minutes setup)
- Custom fuzzy search (another plugin)
- Template variables (more configuration)
- Team sharing (Git repos + more setup)

**Reality**: Most developers want prompts to "just work" across all contexts—terminal, browser, any AI tool. PromptHive is purpose-built for this.

### ...use ChatGPT/Claude history search?
- **30+ seconds** to find that perfect prompt from last week
- Lost when you clear browser history
- **Can't compose**: No `cat error.log | [your-prompt] | claude`
- **No iteration**: Can't improve prompts over time
- **No sharing**: Your team reinvents the same prompts

### ...use GitHub Gists or company wikis?
- **Network dependency**: Fails when WiFi is down
- **No terminal integration**: Can't pipe directly
- **Context switching**: Breaks flow between terminal and browser
- **Slow**: 2-5 seconds minimum to access

## The Real Differentiator: Workflow Velocity

**When prompt access drops below 80ms, something profound happens**: You stop thinking about the tool and start thinking *with* it.

This isn't about saving 30 seconds once. It's about saving 5 minutes, 20 times per day, while maintaining flow state.

## Commands (v0.2.8)

```bash
ph use <name>         # Use a prompt (u) - auto-clipboard, save, append, file
ph show <name>        # Display prompt (s) - with I/O options
ph new <name>         # Create prompt (n) - smart detection
ph edit <name>        # Edit prompt (e) - opens $EDITOR
ph find <query>       # Search prompts (f) - fuzzy matching
ph ls                 # List prompts (l) - see all
ph delete <name>      # Delete prompt (d) - with confirm
ph clean <text>       # Clean and format text with AI-style processing

# Just run 'ph' to launch the TUI (like lazygit)
ph                    # Launch interactive TUI

### Advanced Features
```bash
ph compose <prompts>  # Chain prompts together
ph stats              # Usage analytics dashboard
ph completion <shell> # Generate shell completions
ph login              # Authenticate with registry for sync
ph sync               # Sync prompts with cloud ✅ WORKING
ph sync status         # Check sync status and conflicts ✅ WORKING
ph sync push          # Push local changes to cloud ✅ WORKING
ph sync pull          # Pull cloud changes locally ✅ WORKING
```

## 🚀 Power User Examples

### Built-in Template Variables
```bash
# Prompts automatically support built-in variables
ph new standup "Daily standup for {date} by {user} on {hostname}"
ph new bug-report "Bug in {pwd} on branch {git_branch} ({git_status})"

# Environment variables work too
ph new deploy "Deploy to {env:AWS_REGION} in {env:ENVIRONMENT}"

# Variables are replaced when you use the prompt
ph use standup | llm
# Output includes current date, username, and hostname automatically
```

### Version Control - Never Lose a Good Prompt
```bash
# Tag important versions of your prompts
ph version api-design v1.0 -m "Initial stable API prompt"
ph version api-design v1.1 -m "Added error handling section"
ph version api-design v2.0 -m "Complete rewrite for REST best practices"

# View history
ph versions api-design
# 📚 Version history for 'api-design'
# 📌 v2.0 (a3b4c5d6) - Complete rewrite for REST best practices
# 📌 v1.1 (87654321) - Added error handling section
# 📌 v1.0 (12345678) - Initial stable API prompt

# Rollback when needed
ph rollback api-design v1.1
# ✅ Rolled back 'api-design' to version 'v1.1'
```


### Diff & Merge - Collaborate on Prompts
```bash
# Compare prompt versions
ph diff api-v1 api-v2
ph diff api-v1 api-v2 --format side-by-side
ph diff local-prompt team/shared-prompt --context 10

# Merge improvements
ph merge team/api-enhanced api-design --backup
ph merge experimental-fix stable-prompt --preview
```

### Web Dashboard - Visual Analytics
```bash
# Open interactive dashboard
ph web                    # Full dashboard
ph web stats             # Usage analytics
ph web prompts           # Browse all prompts
ph web --no-browser      # Generate HTML only
```


## 🎯 Unified I/O Design - Smart Defaults

PromptHive uses intelligent defaults based on context, reducing typing while maintaining flexibility:

### Text Transformation Commands (use, show, clean)
**In Terminal (TTY):**
- Auto-copies to clipboard by default
- No stdout output (content is on clipboard)
- Use `-q` to suppress clipboard and output to stdout instead

**When Piping:**
- Outputs to stdout for pipe compatibility
- No auto-clipboard (use `-c` to force clipboard)

```bash
# Terminal usage - auto-clipboard
ph clean "messy text"                    # ✓ Copied to clipboard (2ms)
ph use api-design "Create user endpoint" # ✓ Copied to clipboard (3ms)

# Piping - outputs to stdout
echo "messy text" | ph clean | ph use formatter | claude -p

# Force clipboard even when piping
echo "important" | ph clean -c          # Copies AND outputs

# Quiet mode in terminal
ph clean "text" -q                      # Outputs to stdout, no clipboard
```

### Query Commands (ls, find)
**Always outputs to stdout** (auto-clipboard would be too noisy)
- Use `-c` to explicitly copy results
- Use `-s` to save as a new prompt

```bash
ph ls                                    # Lists to stdout
ph find "api" -c                        # Find and copy results
ph ls -s "my-prompt-list"               # Save list as a new prompt
```

### Universal I/O Flags
Every command supports these consistent flags:
- `-s NAME` - Save output as a new prompt
- `-a NAME` - Append output to existing prompt
- `-c` - Force copy to clipboard
- `-f PATH` - Write to file (with bidirectional sync)
- `-q` - Quiet mode (suppress default behaviors)
- `-e` - Edit before output (where applicable)

### Smart File Operations
The `-f` flag creates bidirectional sync by default:

```bash
# Create prompt with automatic file sync
ph new "API guidelines" -f              # Creates ./api-guidelines.md
ph new "API guidelines" -f api.md       # Creates ./api.md

# Smart naming when path not provided
ph use api-design "endpoints" -f        # Creates ./api-design-output.md

# Bidirectional sync means:
# - Edit the file → prompt updates
# - Edit the prompt → file updates
# - Always stay in sync!
```

### Composable Flags
All flags work together sensibly:

```bash
# Clean, save, append, copy, and write to file
ph clean "text" -s cleaned -a log -c -f output.md

# Use prompt, edit result, save, and sync to file
ph use template -e -s edited -f template-output.md

# Find prompts, save results, copy to clipboard
ph find "api" -s search-results -c
```

### Design Philosophy
- **Smart defaults**: Do the right thing based on context
- **Explicit overrides**: Flags always override defaults
- **Composability**: All flags combine logically
- **No surprises**: Predictable behavior in all contexts

## Configuration

### Disable Logging
```bash
# Temporary (this session only)
export PROMPTHIVE_LOG_LEVEL=error    # Only errors
export PROMPTHIVE_LOG_LEVEL=off      # Complete silence

# Permanent (add to ~/.zshrc or ~/.bashrc)
echo 'export PROMPTHIVE_LOG_LEVEL=error' >> ~/.zshrc
source ~/.zshrc
```

### Other Environment Variables
```bash
PROMPTHIVE_BASE_DIR=/custom/path     # Change storage location
PROMPTHIVE_EDITOR=code               # Set preferred editor
PROMPTHIVE_LOG_FORMAT=json           # JSON logs for production
```

## Creative Usage Patterns

### 🎯 Daily Journaling & Reflection
```bash
# Create a journal bank
ph new journal/daily "Date: {date}\n\nToday I learned: "
ph new journal/gratitude "Three things I'm grateful for:\n1. "
ph new journal/standup "Yesterday: \nToday: \nBlockers: "

# Daily workflow
claude "$(date | ph use journal/daily)"
ph use journal/gratitude -s entries/$(date +%Y-%m-%d)
```

### 🧠 Learning & Study Assistant
```bash
# Create study prompts
ph new study/explain "Explain {input} like I'm 5"
ph new study/test "Create 5 quiz questions about: {input}"
ph new study/summarize "Key points from this text: {input}"

# Study session
cat lecture-notes.md | ph use study/summarize | claude -p > summary.md
claude "$(cat chapter-3.txt | ph use study/test)"
```

### 💼 Meeting & Communication Templates
```bash
# Professional templates
ph new work/email-followup "Subject: Follow-up from our {date} meeting\n\nHi {name},\n\nThank you for..."
ph new work/1-on-1 "1-on-1 with {manager}\n\nAgenda:\n- Career development\n- Current projects\n- Feedback\n\nNotes: "
ph new work/proposal "Proposal: {title}\n\nProblem: \nSolution: \nImpact: \nTimeline: "

# Quick usage
ph use work/email-followup "meeting yesterday" | pbcopy
claude "$(ph use work/proposal 'Implement CI/CD Pipeline')"
```

### 🎨 Creative Writing & Content
```bash
# Writing helpers
ph new write/character "Create a character profile:\nName: {input}\nTraits: "
ph new write/plot-twist "Given this plot: {input}\n\nSuggest 3 unexpected twists:"
ph new write/blog-outline "Blog post about: {input}\n\nOutline with sections:"

# Content creation
claude "$(ph use write/blog-outline 'Terminal productivity')"
echo "Detective Jane Smith" | ph use write/character | llm -p
```

### 🔧 DevOps & Operations
```bash
# Runbook templates
ph new ops/incident "Incident: {title}\nSeverity: \nImpact: \nMitigation: "
ph new ops/postmortem "Postmortem for: {incident}\n\nWhat happened: \nRoot cause: \nLessons learned: "
ph new ops/deploy-checklist "Deploy {service} to {env}\n\n[ ] Tests pass\n[ ] Migrations run\n[ ] Monitoring updated\n[ ] Rollback plan"

# Incident response
ph use ops/incident "Database connection timeout" -s incidents/$(date +%s)
claude "$(kubectl logs -n prod app-pod --tail=100 | ph use ops/debug)"
```

### 📊 Data Analysis & Research
```bash
# Analysis templates
ph new data/hypothesis "Hypothesis: {input}\n\nTest with: \nExpected outcome: "
ph new data/findings "Analysis of: {dataset}\n\nKey findings: \nLimitations: \nNext steps: "
ph new data/visualize "Data: {input}\n\nSuggest 3 visualization types and why:"

# Research workflow
cat experiment-results.csv | ph use data/findings | claude -p > analysis.md
claude "$(ph use data/hypothesis 'User engagement increases with dark mode')"
```

### 🎮 Personal Productivity
```bash
# Life management
ph new life/decision "Decision: {input}\n\nPros:\nCons:\nAlternatives:"
ph new life/habit "Track habit: {habit}\n\nDate: {date}\nCompleted: [ ]\nNotes: "
ph new life/goals "Goal: {input}\n\nWhy it matters: \nSuccess criteria: \nFirst step: "

# Daily use
claude "$(ph use life/decision 'Accept job offer at startup')"
ph use life/habit "Morning meditation" -a habits/meditation-log
```

### 🚀 API & Integration Patterns
```bash
# Combine with other tools
alias morning='claude "$(git log --since=yesterday | ph use journal/standup)"'
alias debug='ph use essentials/debug | tee debug.log | claude'
alias review='gh pr view --json files | ph use essentials/review | claude -p'

# Scheduled prompts
crontab -e
# 0 9 * * 1 ph use work/weekly-goals | mail -s "Weekly Goals" me@example.com
# 0 17 * * * git log --since="9am" | ph use journal/daily | ph save today
```

## Performance

PromptHive operations are optimized for **instant prompt access**:

```bash
$ time ph use essentials/commit
✓ Prompt loaded and copied to clipboard (3ms)
real    0m0.045s

$ ph ls
📋 Your prompts: (12ms)
  essentials/commit       - Semantic commit messages
  essentials/debug-error  - Error analysis & fixes
  essentials/pr          - Pull request descriptions
  essentials/code-review - Code review analysis
  essentials/refactor    - Code refactoring
```

**What these times represent:**
- **PromptHive operations**: File I/O, prompt processing, fuzzy matching (~2-50ms)
- **AI processing time**: Separate - depends on your AI tool (5-30+ seconds)
- **Value proposition**: Zero-latency prompt access vs. writing prompts from scratch

**The real speed gain**: Instead of spending 2-5 minutes crafting the perfect prompt, you get battle-tested prompts instantly.

## Partial Matching (Like JJ)

PromptHive uses smart partial matching:

```bash
$ ph use d
# If unique, uses the match
# If ambiguous, shows options:
Error: Multiple matches. Did you mean:
  essentials/debug-error    (de)   - Error analysis & fixes
  essentials/document       (do)   - Documentation generation
  custom/deploy            (dep)  - Deployment scripts

$ ph use de  # Now unique - uses 'essentials/debug-error'
```

## The Magic: Composition

PromptHive is the ONLY tool that lets you chain prompts:

```bash
# Fix a bug completely
cat error.log | \
  ph use analyze-error | \
  ph use find-root-cause | \
  ph use generate-fix | \
  ph use add-tests | \
  claude

# Each prompt transforms the previous output
# Total time: 250ms for 5 prompts
```

## Real-World Usage

### Daily Developer Workflows

```bash
# 🔥 Auto-generate perfect commit messages
git diff --staged | ph use essentials/commit | llm

# 🐛 Debug errors instantly
cat error.log | ph use essentials/debug | aichat
ph use essentials/debug "Fix timeout in API calls" | claude

# 📝 Generate PR descriptions
git diff main...HEAD | ph use essentials/review | claude

# 🧪 Create tests from code
cat src/api.js | ph use essentials/review | llm > src/api.test.js

# 📚 Document your code
cat complex-function.py | ph use essentials/docstring | llm

# 🔍 Code review helper
git show HEAD | ph use essentials/review | aichat > review-notes.md
```

### Advanced Workflows

```bash
# Chain multiple prompts for complex tasks
cat buggy-code.js | \
  ph use essentials/debug | llm | \
  ph use essentials/refactor | llm | \
  ph use essentials/generate-tests | llm > fixed-code.js

# Morning standup automation
git log --since=yesterday --oneline | \
  ph use essentials/standup | llm > standup.md

# Refactor legacy code
cat old-api.py | \
  ph use essentials/refactor | llm | \
  ph use essentials/add-types | llm > modern-api.py
```

### 🧙 Workflow Automation Examples

```bash
# AI-Powered Git Workflow
alias smart-commit='git diff --staged | ph use commit-message | llm | git commit -F -'
alias smart-pr='git diff main...HEAD | ph use essentials/review | claude -p | gh pr create --body-file -'

# Automated Code Review Pipeline
function ai-review() {
  git diff main...HEAD | \
    ph compose check-style,find-bugs,suggest-improvements | \
    llm > review-$(date +%Y%m%d).md
}

# Daily Standup Generator
function standup() {
  echo "Yesterday: $(git log --since=yesterday --author=$(git config user.name) --oneline)" | \
    ph use standup --edit | \
    pbcopy
}

# Smart Documentation Generator
function doc-this() {
  cat "$1" | \
    ph compose analyze-code,generate-docs,add-examples | \
    llm > "${1%.*}.docs.md"
}
  ph use modernize-code | llm | \
  ph use add-types | llm > modern-api.py
```

## 🔰 Shell Pipes Primer (for Terminal Newbies)

### The Basics
```bash
# The pipe | sends output from one command to another
echo "hello" | cat           # Sends "hello" to cat

# Redirect > saves output to a file
echo "hello" > file.txt      # Saves "hello" to file.txt

# Append >> adds to a file without overwriting
echo "world" >> file.txt     # Adds "world" to file.txt

# Input < reads from a file
cat < file.txt               # Reads content from file.txt
```

### With PromptHive
```bash
# Simple: Copy to clipboard (when in terminal)
ph use api-design           # Just copies, no pipe needed

# Pipe to AI: Send prompt to AI tool
ph use api-design | llm     # Prompt → AI tool

# Chain: Multiple operations
cat code.js | ph use review | llm | ph new review-result

# Save: Capture output
ph use api | llm > response.txt

# Combine: Mix different sources
ph use debug "Fix this: $(cat error.log)" | aichat
```

### When to Use What
- **No pipe**: When you just want to copy a prompt
- **Single pipe |**: When sending to one AI tool
- **Multiple pipes |**: When chaining operations
- **Redirect >**: When saving results
- **Cat/Echo**: When adding context to prompts

## 🏦 Prompt Banks - Instant Productivity

### Built-in Prompts Ready to Use
```bash
# essentials/ - Core developer workflows
ph use essentials/commit         # Generate conventional commit message from diff
ph use essentials/debug          # Analyze errors and suggest fixes
ph use essentials/review         # Comprehensive code review

# 10x/ - Advanced productivity workflows
ph use 10x/afk-task             # Long-running autonomous task for calmhive
ph use 10x/fix-tests            # Systematically fix all failing tests
ph use 10x/refactor             # Refactor code for clarity and performance
```

### Create Your Own Banks
```bash
# Option 1: From existing prompts
mkdir -p ~/.prompthive/banks/myteam
cp ~/.prompthive/prompts/*.md ~/.prompthive/banks/myteam/

# Option 2: From Claude commands (if you have them)
mkdir -p ~/.prompthive/banks/claude
cp ~/.claude/commands/*.md ~/.prompthive/banks/claude/
# Now use: ph use claude/command-name

# Option 3: Create specialized banks
mkdir -p ~/.prompthive/banks/{backend,frontend,devops}
ph new backend/api-design "Design REST APIs..."
ph new frontend/component "React component template..."
ph new devops/deploy "Deployment checklist..."
```

### Sharing Banks with Your Team

```bash
# Share via Git (recommended)
cd ~/.prompthive/banks/myteam
git init && git add .
git commit -m "feat: team prompt collection"
git remote add origin github.com/yourteam/prompts
git push

# Team members clone it
cd ~/.prompthive/banks
git clone github.com/yourteam/prompts team
# Now everyone can: ph use team/api-design

# Keep banks in sync
cd ~/.prompthive/banks/team && git pull

# Or use PromptHive's built-in sharing
ph bank publish myteam        # Publish to registry
ph bank install @user/team    # Install from registry
```

### Real Workflows with Banks
```bash
# Developer workflows
git diff --staged | ph use essentials/commit | llm
cat error.log | ph use essentials/debug-error | claude
git diff main | ph use essentials/review | llm > review.md

# Professional communication
ph use professional/email-reply "Thanks for your proposal..." | llm
ph use professional/meeting-notes "Sprint planning discussion" | llm
ph use professional/status-update "Project Alpha Week 3" | claude

# Infrastructure & DevOps
ph use devops/dockerfile "Node.js microservice" | llm > Dockerfile
ph use devops/ci-pipeline "React app with tests" | claude > .github/workflows/ci.yml
ph use devops/kubernetes "web API with Redis" | llm > k8s-manifests.yaml

# Design patterns
ph use coding-patterns/api-design "User management system" | claude
ph use coding-patterns/error-handling "Payment processing" | llm

# Advanced workflows
ph use 10x/afk-task "Build authentication system" | claude
ph use workflow/analyze "Review codebase structure" | llm
ph use claude-commands/workflow-manager "Deploy to production" | claude

# Create aliases for common workflows
alias commit='git diff --staged | ph use essentials/commit | llm'
alias email='ph use professional/email-reply'
alias dockerize='ph use devops/dockerfile'
```

## Building PromptHive with Itself

We built PromptHive using PromptHive:

```bash
# Bootstrap phase
ph new rust-cli
ph use rust-cli | cursor

# Every feature after
ph new add-command
ph use add-command | claude

# Test and iterate
ph search "rust testing"
ph use rust-test | cursor
```

## File Structure

Dead simple, just markdown files:

```
~/.prompthive/
├── prompts/          # Your personal prompts
│   ├── api.md
│   ├── auth.md
│   └── test.md
├── banks/           # Organized prompt collections
│   ├── essentials/  # Built-in essentials
│   ├── 10x/        # Productivity workflows
│   ├── workflow/   # Complex processes
│   └── myteam/     # Your custom banks
├── registry/        # Cached registry packages
└── config.toml      # Configuration
```

## Prompt Format

Just markdown with optional frontmatter:

```markdown
---
id: api
description: REST API design
---

Design a REST API with these requirements:
- Resource: {resource}
- Operations: {operations}
- Include error handling
- Follow REST best practices
```

## Why PromptHive?

1. **Speed**: 8ms operations are addictive (ChatGPT: 30+ seconds)
2. **Simple**: Just like `ls` and `cat` - no learning curve
3. **Universal**: Works with ALL AI tools, not locked to one
4. **Offline**: Your prompts work without internet
5. **Composable**: Chain prompts like Unix commands
6. **Smart I/O**: Auto-clipboard, save, append - works how you think

**8ms vs 30 seconds. Every time. That's the difference.**

## Features

### Core Features (Production Ready)
- ✅ **Lightning-fast prompt management** - Create, edit, search, and use prompts
- ✅ **Intelligent fuzzy matching** - `ph u ap` → `api-design` instantly
- ✅ **Universal shell integration** - Copy to clipboard or pipe to ANY tool
- ✅ **Smart I/O operations** - Auto-clipboard, save, append, file output
- ✅ **Text cleaning & formatting** - AI-style text processing with `ph clean`
- ✅ **Smart completions** - Tab completion for bash, zsh, fish
- ✅ **Cross-platform** - Works on macOS, Linux, Windows
- ✅ **Prompt composition** - Chain prompts for complex workflows
- ✅ **Magic link authentication** - Secure login for registry sync
- ✅ **Performance guarantee** - All operations under 15ms

### Advanced Features (All Included - Free & Open Source)
- ✅ **Registry sync** - Cloud backup and device sync
- ✅ **Magic link authentication** - Passwordless secure login
- ✅ **Team collaboration** - Private prompt banks and sharing
- ✅ **Prompt sharing** - Public and invite-based prompt sharing
- ✅ **Community registry** - Growing library of quality prompts
- ✅ **Usage analytics** - Productivity tracking with achievements
- ✅ **Web dashboard** - Visual prompt management and statistics

## Development Build

```bash
# Development build (all features)
cargo build --release

# Run tests
cargo test --release

# Performance benchmark
cargo test --release test_performance
```

## Troubleshooting

### Command not found
```bash
# Ensure binary is in PATH
echo $PATH
which ph

# If using cargo install
cargo install --list | grep prompthive
```

### Clipboard not working
```bash
# If terminal, it copies automatically
ph use api  # Copies to clipboard

# Or pipe explicitly
ph use api | pbcopy  # macOS
ph use api | xclip  # Linux
```

### Shell completions not working
```bash
# Regenerate completions
ph completion bash > ~/.bash_completion.d/prompthive
source ~/.bashrc
```

## 🤝 The Complete Terminal AI Stack

**PromptHive + CalmHive = Ultimate AI Development Workflow**

PromptHive handles instant prompt access (8ms), while [CalmHive](https://calmhive.com) handles background processing, voice control, and smart execution management.

```bash
# Install the complete stack
cargo install prompthive
npm install -g @calmhive/calmhive-cli

# Use together for powerful workflows
calmhive afk "$(ph use essentials/refactor)" --iterations 20  # Background processing
calmhive voice "$(ph use essentials/debug)"                  # Voice control
ph use essentials/commit | calmhive process --smart-retry    # Smart execution
```

**Perfect Division of Labor:**
- **PromptHive**: Instant prompt management, universal AI tool compatibility
- **CalmHive**: Background processing, usage limit handling, voice control

Learn more: [prompthive.sh](https://prompthive.sh) | [calmhive.com](https://calmhive.com)

## Performance

PromptHive is engineered for sub-15ms performance across all operations. Built with Rust for maximum efficiency and reliability, it handles thousands of prompts without slowing down your workflow.

**Tested on version 0.2.8**: All core operations (new, use, show, edit, ls, find) complete in under 15ms on modern hardware.

## Join the Community 🐝

### 🌟 **100% Open Source & Free**
- ✅ Unlimited local prompts
- ✅ Lightning-fast performance (<15ms)
- ✅ Compose & chain prompts
- ✅ Cross-platform support
- ✅ Text cleaning & formatting
- ✅ Magic link authentication
- ✅ Cloud sync across devices
- ✅ Private prompt banks
- ✅ Team collaboration & sharing
- ✅ Prompt sharing features
- ✅ Advanced analytics & insights
- ✅ Community registry access
- ✅ Self-hostable infrastructure

*"Professional prompt management for everyone"*

### 🤝 **Contributing**
- Report bugs and request features
- Submit prompts to the community registry
- Contribute code improvements
- Help with documentation
- Join discussions and share workflows

Visit our [GitHub repository](https://github.com/joryeugene/prompthive) to get involved!

## 📚 Resources & Links

- **[PromptHive.sh](https://prompthive.sh/)** - Official website with documentation and installation guides
- **[CalmHive.com](https://calmhive.com/)** - Perfect companion for background AI processing and voice control
- **[GitHub Repository](https://github.com/joryeugene/prompthive)** - Source code, issues, and contributions
- **[Crates.io Package](https://crates.io/crates/prompthive)** - Rust package with version history
- **[Blog](http://jorypestorious.com/blog/)** - Terminal velocity philosophy and AI development insights

---

**"Prompts that just work."**
