#!/bin/bash
# PromptHive Creative Usage Patterns Examples

echo "=== PromptHive Creative Patterns Demo ==="
echo

# Set up
PH="./target/release/ph"
export PROMPTHIVE_LOG_LEVEL=error  # Quiet mode for demo

# 1. Daily Journaling
echo "1. Creating journal prompts..."
$PH new journal/daily "Date: {date}

Today I learned: " -q
$PH new journal/gratitude "Three things I'm grateful for:
1. " -q
echo "✓ Journal prompts created"
echo

# 2. Study Assistant
echo "2. Creating study prompts..."
$PH new study/explain "Explain {input} like I'm 5:

Simple explanation: " -q
$PH new study/test "Create 5 quiz questions about: {input}

Questions:
1. " -q
echo "✓ Study prompts created"
echo

# 3. Work Templates
echo "3. Creating work templates..."
$PH new work/email-followup "Subject: Follow-up from our {date} meeting

Hi {name},

Thank you for " -q
$PH new work/1-on-1 "1-on-1 with {manager}

Agenda:
- Career development
- Current projects
- Feedback

Notes: " -q
echo "✓ Work templates created"
echo

# 4. Creative Writing
echo "4. Creating writing helpers..."
$PH new write/character "Create a character profile:
Name: {input}
Traits: " -q
$PH new write/blog-outline "Blog post about: {input}

Outline with sections:
1. Introduction
2. " -q
echo "✓ Writing prompts created"
echo

# 5. DevOps Templates
echo "5. Creating DevOps runbooks..."
$PH new ops/incident "Incident: {title}
Severity: 
Impact: 
Mitigation: " -q
$PH new ops/deploy-checklist "Deploy {service} to {env}

[ ] Tests pass
[ ] Migrations run
[ ] Monitoring updated
[ ] Rollback plan" -q
echo "✓ DevOps templates created"
echo

# 6. Data Analysis
echo "6. Creating data analysis templates..."
$PH new data/hypothesis "Hypothesis: {input}

Test with: 
Expected outcome: " -q
$PH new data/findings "Analysis of: {dataset}

Key findings: 
Limitations: 
Next steps: " -q
echo "✓ Data analysis templates created"
echo

# 7. Personal Productivity
echo "7. Creating productivity tools..."
$PH new life/decision "Decision: {input}

Pros:
- 

Cons:
- 

Alternatives:
- " -q
$PH new life/goals "Goal: {input}

Why it matters: 
Success criteria: 
First step: " -q
echo "✓ Productivity prompts created"
echo

# Demo usage
echo "=== Example Usage ==="
echo
echo "Journal entry:"
$PH use journal/daily -q
echo
echo "Study helper:"
echo "quantum physics" | $PH use study/explain -q
echo
echo "Work template:"
$PH use work/1-on-1 -q | head -n 5
echo "..."
echo

echo "=== All Creative Prompts ==="
$PH ls | grep -E "journal/|study/|work/|write/|ops/|data/|life/"
echo

echo "=== Demo Complete ==="
echo "Try: claude \"\$(ph use journal/daily)\""
echo "Or:  ph use life/decision \"Accept new job offer\" | claude -p"