# Agent Teams & Subagents — Master Reference Guide

> A comprehensive guide for building effective agent teams and subagents in Claude Code.
> Covers architecture, configuration, patterns, and best practices.

---

## Table of Contents

1. [Decision Framework: Teams vs Subagents vs Single Session](#decision-framework)
2. [Agent Teams](#agent-teams)
3. [Custom Subagents](#custom-subagents)
4. [Subagent Configuration Reference](#subagent-configuration-reference)
5. [Hooks for Quality Gates](#hooks-for-quality-gates)
6. [Patterns & Recipes](#patterns--recipes)
7. [Best Practices](#best-practices)
8. [Pitfalls & Limitations](#pitfalls--limitations)

---

## Decision Framework

### When to Use What

| Scenario | Use | Why |
|:---------|:----|:----|
| Focused task, only result matters | **Subagent** | Lower tokens, result summarized back |
| Parallel independent research | **Subagents (background)** | Concurrent, no inter-agent communication needed |
| Complex work requiring discussion | **Agent Team** | Teammates message each other directly |
| Sequential tasks, shared files | **Single session** | No coordination overhead |
| Same-file edits | **Single session** | Avoids merge conflicts |
| Cross-layer coordination (FE/BE/tests) | **Agent Team** | Each teammate owns a layer |
| Competing hypotheses / debate | **Agent Team** | Teammates challenge each other |

### Comparison Table

| | Subagents | Agent Teams |
|:--|:----------|:------------|
| **Context** | Own context window; results return to caller | Own context window; fully independent |
| **Communication** | Report results back to main agent only | Teammates message each other directly |
| **Coordination** | Main agent manages all work | Shared task list with self-coordination |
| **Best for** | Focused tasks where only the result matters | Complex work requiring discussion and collaboration |
| **Token cost** | Lower: results summarized back | Higher: each teammate is a separate instance |

---

## Agent Teams

### Overview

Agent teams coordinate multiple Claude Code instances working together. One session is the **team lead**, others are **teammates**. Each runs in its own context window with independent state.

**Requirements:**
- Claude Code v2.1.32+
- Experimental feature flag must be enabled

### Enable Agent Teams

In `settings.json` or `settings.local.json`:

```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

Or set in your shell: `export CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`

### Architecture

| Component | Role |
|:----------|:-----|
| **Team lead** | Main session that creates the team, spawns teammates, coordinates work |
| **Teammates** | Separate Claude Code instances working on assigned tasks |
| **Task list** | Shared list of work items that teammates claim and complete |
| **Mailbox** | Messaging system for inter-agent communication |

**Storage locations:**
- Team config: `~/.claude/teams/{team-name}/config.json`
- Task list: `~/.claude/tasks/{team-name}/`

**Key behaviors:**
- Teammates inherit the lead's permission settings at spawn time
- Each teammate loads the same project context (CLAUDE.md, MCP servers, skills)
- Lead's conversation history does NOT carry over to teammates
- Messages delivered automatically; idle notifications sent
- `message` sends to one teammate; `broadcast` sends to all

### Starting a Team

Describe the task and team structure in natural language:

```text
Create an agent team to review PR #142. Spawn three reviewers:
- One focused on security implications
- One checking performance impact
- One validating test coverage
Have them each review and report findings.
```

### Display Modes

| Mode | Description | Navigation |
|:-----|:------------|:-----------|
| **in-process** | All teammates in one terminal | Shift+Down to cycle, Enter to view, Escape to interrupt |
| **split panes** | Each teammate gets own pane | Click into pane (requires tmux or iTerm2) |

Default is `"auto"`. Override in `~/.claude.json`:

```json
{
  "teammateMode": "in-process"
}
```

Or per session: `claude --teammate-mode in-process`

Toggle task list with **Ctrl+T** (in-process mode).

### Controlling Teammates

**Specify model per teammate:**
```text
Create a team with 4 teammates to refactor these modules in parallel.
Use Sonnet for each teammate.
```

**Require plan approval:**
```text
Spawn an architect teammate to refactor the authentication module.
Require plan approval before they make any changes.
```

**Task states:** pending → in progress → completed
- Tasks can have dependencies on other tasks
- Task claiming uses file locking to prevent race conditions

**Shutdown:**
```text
Ask the researcher teammate to shut down
```

**Cleanup (always use the lead):**
```text
Clean up the team
```

---

## Custom Subagents

### Overview

Subagents are specialized AI assistants that handle specific task types. Each runs in its own context window with a custom system prompt, specific tool access, and independent permissions.

**Benefits:**
- **Preserve context** — keep exploration out of main conversation
- **Enforce constraints** — limit available tools
- **Reuse configurations** — user-level subagents work across projects
- **Specialize behavior** — focused system prompts for specific domains
- **Control costs** — route to cheaper models like Haiku

### Built-in Subagents

| Subagent | Model | Tools | Purpose |
|:---------|:------|:------|:--------|
| **Explore** | Haiku | Read-only (no Edit/Write) | File discovery, code search, codebase exploration |
| **Plan** | Inherited | Read-only | Research agent for plan mode |
| **General-purpose** | Inherited | All tools | Complex multi-step tasks |

### Creating Subagents

**Interactive:** Run `/agents` → Create new agent → choose scope → describe behavior → select tools/model.

**File-based:** Create a markdown file with YAML frontmatter:

```markdown
---
name: code-reviewer
description: Reviews code for quality, security, and best practices. Use proactively after code changes.
tools: Read, Glob, Grep, Bash
model: sonnet
---

You are a senior code reviewer. When invoked:
1. Read all changed files
2. Check for security vulnerabilities, performance issues, and code smells
3. Provide specific, actionable feedback with file:line references
4. Suggest concrete improvements, not just problems
```

### Subagent File Locations (Priority Order)

| Priority | Location | Scope |
|:---------|:---------|:------|
| 1 (highest) | `--agents` CLI flag | Current session only |
| 2 | `.claude/agents/` | Current project |
| 3 | `~/.claude/agents/` | All your projects |
| 4 (lowest) | Plugin's `agents/` directory | Where plugin is enabled |

### Invocation Methods

| Method | Example |
|:-------|:--------|
| **Automatic** | Claude delegates based on `description` field |
| **Natural language** | "Ask the code-reviewer to look at auth changes" |
| **@-mention** | `@"code-reviewer (agent)" look at the auth changes` |
| **Session-wide** | `claude --agent code-reviewer` |
| **Settings** | `"agent": "code-reviewer"` in settings.json |

### Foreground vs Background

| Mode | Behavior | Use when |
|:-----|:---------|:---------|
| **Foreground** | Blocking; permission prompts pass through | Need results before proceeding |
| **Background** | Concurrent; permissions pre-approved | Independent work, parallel research |

- Set `background: true` in frontmatter to always run in background
- Press **Ctrl+B** to background a running subagent
- Disable background tasks: `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1`

---

## Subagent Configuration Reference

### All Frontmatter Fields

| Field | Required | Type | Description |
|:------|:---------|:-----|:------------|
| `name` | Yes | string | Unique identifier (lowercase letters and hyphens) |
| `description` | Yes | string | When Claude should delegate to this subagent. **Critical for automatic delegation.** |
| `tools` | No | list | Tools the subagent can use. Inherits all if omitted |
| `disallowedTools` | No | list | Tools to deny (inverse of `tools`) |
| `model` | No | string | `sonnet`, `opus`, `haiku`, full model ID, or `inherit` |
| `permissionMode` | No | string | `default`, `acceptEdits`, `dontAsk`, `bypassPermissions`, `plan` |
| `maxTurns` | No | number | Maximum agentic turns before stopping |
| `skills` | No | list | Skills loaded into context at startup |
| `mcpServers` | No | list | MCP servers available to this subagent |
| `hooks` | No | object | Lifecycle hooks scoped to this subagent |
| `memory` | No | string | Persistent memory scope: `user`, `project`, `local` |
| `background` | No | boolean | Always run as background task |
| `effort` | No | string | `low`, `medium`, `high`, `max` (Opus 4.6 only) |
| `isolation` | No | string | `worktree` for temporary git worktree |
| `initialPrompt` | No | string | Auto-submitted as first user turn (main session agent only) |

### Model Resolution Order

1. `CLAUDE_CODE_SUBAGENT_MODEL` environment variable (highest priority)
2. Per-invocation `model` parameter (in Agent tool call)
3. Subagent definition's `model` frontmatter
4. Main conversation's model (lowest priority)

### Tool Control Examples

**Allowlist (only these tools):**
```yaml
---
name: safe-researcher
tools: Read, Grep, Glob, Bash
---
```

**Denylist (everything except these):**
```yaml
---
name: no-writes
disallowedTools: Write, Edit
---
```

**Restrict which subagents it can spawn:**
```yaml
---
name: coordinator
tools: Agent(worker, researcher), Read, Bash
---
```

### MCP Server Scoping

```yaml
---
name: browser-tester
mcpServers:
  - playwright:
      type: stdio
      command: npx
      args: ["-y", "@playwright/mcp@latest"]
  - github
---
```

### Persistent Memory

| Scope | Storage Location |
|:------|:-----------------|
| `user` | `~/.claude/agent-memory/<agent-name>/` |
| `project` | `.claude/agent-memory/<agent-name>/` |
| `local` | `.claude/agent-memory-local/<agent-name>/` |

```yaml
---
name: project-expert
memory: project
---
```

### Preloaded Skills

```yaml
---
name: api-developer
skills:
  - api-conventions
  - error-handling-patterns
---
```

### Hooks in Subagent Frontmatter

All standard hook events are supported. The `Stop` event is automatically converted to `SubagentStop`.

```yaml
---
name: validated-worker
hooks:
  SubagentStop:
    - command: "run-tests.sh"
      timeout: 30000
---
```

### Disabling Specific Subagents

In settings.json:
```json
{
  "permissions": {
    "deny": ["Agent(Explore)", "Agent(my-custom-agent)"]
  }
}
```

---

## Hooks for Quality Gates

### Team-Relevant Hook Events

| Hook | Fires When | Exit Code 2 Behavior |
|:-----|:-----------|:---------------------|
| `TeammateIdle` | A teammate is about to go idle | Sends feedback to the teammate |
| `TaskCreated` | A task is being created | Prevents task creation |
| `TaskCompleted` | A task is marked complete | Prevents completion (sends back for rework) |
| `SubagentStart` | A subagent is spawned | Can block or modify spawning |
| `SubagentStop` | A subagent finishes | Can trigger post-processing |

### Example: Enforce Test Coverage on Task Completion

```json
{
  "hooks": {
    "TaskCompleted": [
      {
        "command": "bash -c 'cargo test 2>&1 | tail -5'",
        "timeout": 60000
      }
    ]
  }
}
```

Exit code 2 from a hook sends the stderr/stdout as feedback, preventing completion until the issue is resolved.

---

## Patterns & Recipes

### Pattern 1: Parallel Code Review Team

```text
Create an agent team to review PR #142. Spawn three reviewers:
- One focused on security implications
- One checking performance impact
- One validating test coverage
Have them each review and report findings.
```

**Why it works:** Each reviewer has a clear, non-overlapping scope. No shared file edits. Results synthesized by the lead.

### Pattern 2: Competing Hypotheses (Debugging)

```text
Users report the app crashes on startup after the latest update.
Spawn 4 agent teammates to investigate different hypotheses:
1. Configuration/environment changes
2. Dependency version conflicts
3. Database migration issues
4. Memory/resource exhaustion
Have them talk to each other to disprove theories. Update findings.
```

**Why it works:** Parallel investigation with built-in adversarial review. Converges faster than sequential debugging.

### Pattern 3: Cross-Layer Feature Implementation

```text
Create an agent team for the new inventory system:
- Backend teammate: implement the ECS components and systems
- Frontend teammate: build the UI panels
- Test teammate: write integration tests as the others work
Require plan approval for the backend teammate.
```

**Why it works:** Clear ownership per layer. Plan approval on the riskiest part. Test teammate can start writing test scaffolding immediately.

### Pattern 4: Research Subagents (No Team Needed)

When you just need parallel information gathering without inter-agent discussion, use background subagents instead of a full team:

```markdown
<!-- .claude/agents/researcher.md -->
---
name: researcher
description: Deep codebase research agent for investigating specific questions
tools: Read, Grep, Glob, Bash
model: haiku
background: true
---

You are a research agent. Investigate the given question thoroughly.
Search for all relevant code, configuration, and documentation.
Return a structured summary of your findings.
```

### Pattern 5: Validated Worker Subagent

A subagent that can't mark work done until tests pass:

```markdown
<!-- .claude/agents/validated-worker.md -->
---
name: validated-worker
description: Implementation agent that must pass tests before completing
tools: Read, Write, Edit, Glob, Grep, Bash
model: sonnet
isolation: worktree
hooks:
  SubagentStop:
    - command: "cargo test 2>&1"
      timeout: 120000
---

You are an implementation agent. After making changes:
1. Run tests to verify your work
2. Fix any failures before reporting completion
3. Summarize what you changed and test results
```

### Pattern 6: Coordinator with Specialized Workers

```markdown
<!-- .claude/agents/coordinator.md -->
---
name: coordinator
description: Orchestrates complex tasks by delegating to specialized workers
tools: Agent(researcher, implementer, reviewer), Read, Glob, Grep
model: opus
---

You coordinate complex tasks by:
1. Breaking the task into independent pieces
2. Delegating research to the researcher agent
3. Delegating implementation to the implementer agent
4. Running the reviewer agent on completed work
5. Synthesizing results and reporting back
```

### Pattern 7: Cost-Optimized Pipeline

Route different task types to appropriate models:

```markdown
<!-- .claude/agents/quick-search.md -->
---
name: quick-search
description: Fast codebase search for simple lookups
tools: Read, Grep, Glob
model: haiku
effort: low
---
```

```markdown
<!-- .claude/agents/deep-analyzer.md -->
---
name: deep-analyzer
description: Deep analysis of complex architectural questions
tools: Read, Grep, Glob, Bash
model: opus
effort: high
---
```

---

## Best Practices

### Team Composition
- **3–5 teammates** for most workflows; more adds coordination overhead
- **5–6 tasks per teammate** is a good sizing target
- Give each teammate a **clear, non-overlapping scope**
- Start with **research and review** before implementation teams

### Prompting Teammates
- Provide **enough context** in the spawn prompt — teammates don't inherit your conversation
- Be **specific about deliverables** — what should they produce?
- Specify **file ownership** to avoid conflicts
- Tell teammates about each other's roles so they know who to message

### Task Sizing
- Tasks should be **self-contained units** that one agent can complete independently
- If a task requires output from another task, **model the dependency explicitly**
- Prefer many small tasks over few large ones — easier to track and recover from failure

### File Conflict Avoidance
- **Assign file ownership** — each teammate owns specific files/directories
- Use **git worktree isolation** (`isolation: worktree`) for implementation subagents
- For agent teams, establish conventions upfront: "Backend teammate owns `src/game/`, frontend owns `src/ui/`"
- Never have two agents edit the same file simultaneously

### Token Cost Management
- Use **Haiku for research/exploration** subagents
- Use **Sonnet for implementation** (good cost/quality balance)
- Reserve **Opus for coordination, architecture, and complex reasoning**
- Prefer **subagents over teams** when you don't need inter-agent communication
- Set `maxTurns` to prevent runaway token usage

### Subagent Description Writing
The `description` field is the single most important field for automatic delegation. Write it as:
- **When** Claude should use this agent (trigger conditions)
- **What** it does (capabilities)
- Include the word "proactively" if it should fire without being asked

Good: `"Expert code reviewer. Use proactively after code changes to check quality, security, and performance."`
Bad: `"Reviews code"` (too vague, won't trigger reliably)

---

## Pitfalls & Limitations

### Agent Teams
- **No session resumption** with in-process teammates — if the lead dies, teammates are orphaned
- **Task status can lag** — teammates may not see updates immediately
- **Shutdown can be slow** — teammates need time to wrap up
- **One team per session** — cannot run multiple teams concurrently
- **No nested teams** — a teammate cannot spawn its own team
- **Lead is fixed** — cannot promote a teammate to lead
- **Permissions set at spawn** — cannot change teammate permissions after creation
- **Split panes require tmux or iTerm2** — in-process mode is the only universal option

### Subagents
- **Auto-compaction** triggers at ~95% context capacity — long-running subagents may lose earlier context
- **Background subagents pre-approve permissions** — be careful with `bypassPermissions` mode
- **Subagent transcripts** stored at `~/.claude/projects/{project}/{sessionId}/subagents/agent-{agentId}.jsonl`

### Common Mistakes
| Mistake | Fix |
|:--------|:----|
| Using a team for sequential tasks | Use a single session or chained subagents |
| Two agents editing the same file | Assign file ownership or use worktree isolation |
| Vague teammate spawn prompts | Include full context — teammates don't inherit conversation |
| Not waiting for teammates to finish | Always wait; partial results cause coordination bugs |
| Overusing teams for simple tasks | Single session or one subagent is often enough |
| No quality gates | Use hooks (TaskCompleted, SubagentStop) to enforce checks |
| Forgetting maxTurns | Runaway agents waste tokens — set reasonable limits |

---

## Quick Reference Card

### Enable Teams
```json
{ "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
```

### Subagent File Template
```markdown
---
name: my-agent
description: When to use this agent and what it does
tools: Read, Grep, Glob, Bash, Edit, Write
model: sonnet
permissionMode: acceptEdits
maxTurns: 30
memory: project
---

System prompt goes here. Be specific about:
1. What the agent should do
2. How it should approach the work
3. What output format to produce
```

### Keyboard Shortcuts (In-Process Teams)
| Key | Action |
|:----|:-------|
| Shift+Down | Cycle through teammates |
| Enter | View selected teammate |
| Escape | Interrupt / go back |
| Ctrl+T | Toggle task list |
| Ctrl+B | Background current subagent |

### Hook Exit Codes
| Code | Meaning |
|:-----|:--------|
| 0 | Success, continue |
| 1 | Error, abort operation |
| 2 | Feedback — sends output back as correction |
