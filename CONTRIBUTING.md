# Contributing to Vyse

> **Built with effort. Broken by patterns. Protected by Vyse.**

First off — thank you. Vyse is a security tool protecting real ML APIs from real attackers, and every contribution matters. This document is the single source of truth for contributing code, documentation, tests, plugins, or ideas.

Please read it fully before opening your first issue or PR. It will save everyone time.

---

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [The Architecture — Know What You're Touching](#the-architecture)
3. [Ways to Contribute](#ways-to-contribute)
4. [Security Vulnerability Disclosure](#security-vulnerability-disclosure)
5. [Getting Started — Development Environment](#getting-started)
6. [Branching & Commit Conventions](#branching--commit-conventions)
7. [Pull Request Process](#pull-request-process)
8. [Testing Requirements](#testing-requirements)
9. [Code Style & Linting](#code-style--linting)
10. [Contributing ML Models or Scoring Changes](#contributing-ml-models-or-scoring-changes)
11. [Contributing a Plugin (Threat Detector)](#contributing-a-plugin)
12. [Documentation Contributions](#documentation-contributions)
13. [Issue Labels Reference](#issue-labels-reference)
14. [Review SLA & What to Expect](#review-sla)
15. [License & Developer Certificate of Origin](#license--dco)

---

## Code of Conduct

This project follows the [Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

**TL;DR:** Be direct, be kind, disagree on ideas not on people. Maintainers may remove, edit, or reject contributions that violate this, no questions asked.

Report violations privately to: `conduct@vyse-security.dev` (or open a private GitHub Security Advisory if that address is unavailable).

---

## The Architecture

Before contributing, understand the three moving parts. A change to one can silently break another.

```
┌─────────────────────────────────────────────────────┐
│  gateway/          Node.js / Express                │
│  Auth, rate-limiting, session injection             │
│  Talks to: engine via internal HTTP                 │
├─────────────────────────────────────────────────────┤
│  engine/           Python / FastAPI                 │
│  Scoring, tier logic, defence pipeline, Rekor       │
│  Talks to: gateway (receives), Rekor, DB            │
├─────────────────────────────────────────────────────┤
│  dashboard/        React SPA                        │
│  Admin UI, live WebSocket feed                      │
│  Talks to: gateway admin endpoints                  │
└─────────────────────────────────────────────────────┘
```

**Critical paths** — changes here require extra scrutiny and dedicated tests:

| Path | Why It's Critical |
|---|---|
| `engine/app/scoring/` | Determines tier. Wrong logic = false negatives (attacks slip through) or false positives (legit users degraded). |
| `engine/app/defence/pipeline.py` | Applies perturbation. Broken pipeline = clean responses served to Tier 3 actors. |
| `engine/app/store/rekor.py` | Audit immutability. A bug here silently loses the forensic trail. |
| `gateway/src/middleware/auth.js` | Auth bypass = full admin exposure. |

If your PR touches any of the above, expect a slower review and more questions. This is intentional.

---

## Ways to Contribute

### Not a coder? Still useful.

- **Reproduce a bug** — add a comment to an existing issue with a fresh reproduction
- **Improve docs** — spotted an ambiguity, wrong command, or missing step? Fix it
- **Test an attack pattern** — run the attack simulator and report unexpected tier behaviour
- **Review open PRs** — community review is explicitly welcomed and appreciated
- **Translate** — Vyse should be accessible globally

### Coder? Pick your track.

| Track | Good entry points |
|---|---|
| Bug fixes | Issues labelled `bug` + `good first issue` |
| Scoring improvements | Issues labelled `scoring` — requires ML understanding |
| Defence / perturbation | Issues labelled `defence` |
| Plugin development | See [Contributing a Plugin](#contributing-a-plugin) |
| Gateway / infra | Issues labelled `gateway` |
| Dashboard / frontend | Issues labelled `dashboard` |
| Tests | Issues labelled `test-coverage` — always needed |

### Not sure where to start?

Look for [`good first issue`](../../labels/good%20first%20issue) — these are intentionally scoped to be completable with a 1–2 hour time investment and don't touch security-critical paths.

---

## Security Vulnerability Disclosure

> Vyse is a security tool. We take our own security seriously.

**Do NOT open a public GitHub issue for security vulnerabilities.** This exposes the flaw to attackers before a fix exists.

Instead:

1. Go to **Security → Report a Vulnerability** on the GitHub repo page
2. Or email `security@vyse-security.dev` with PGP encryption (key on our website)
3. Include: affected component, reproduction steps, and your assessment of severity

We will acknowledge within **48 hours**, provide a fix timeline within **5 business days**, and credit you in the security advisory unless you prefer anonymity.

### Special case: Bypass techniques

If you discover a technique that reliably evades Vyse's tier classification — a "bypass" — this is a **security vulnerability**, not a feature request. Please report it privately using the process above. We will treat it with the same urgency as a code vulnerability, fix it, and credit you.

---

## Getting Started

### Prerequisites

| Tool | Minimum Version | Purpose |
|---|---|---|
| Python | 3.10+ | Engine |
| Node.js | 18+ | Gateway & Dashboard |
| Docker + Docker Compose | v2+ | Full stack local dev |
| Git | 2.35+ | For `--force-with-lease` support |

### Fork & Clone

```bash
# Fork the repo on GitHub first, then:
git clone https://github.com/YOUR_USERNAME/vyse.git
cd vyse
git remote add upstream https://github.com/vyse-security/vyse.git
```

### Engine (Python)

```bash
cd engine

# Create virtual environment
python -m venv .venv
source .venv/bin/activate  # Windows: .venv\Scripts\activate

# Install with all dev dependencies
pip install -e ".[dev,test,lint]"

# Download required ML models (first time only, ~125MB)
python scripts/download_models.py

# Copy and configure environment
cp .env.example .env
# Edit .env — minimum required: GROQ_API_KEY

# Run the engine
uvicorn app.main:app --reload --port 8000
```

### Gateway (Node.js)

```bash
cd gateway
npm install
cp .env.example .env
node src/index.js
```

### Dashboard (React)

```bash
cd dashboard
npm install
npm start   # http://localhost:3000
```

### Full Stack (Docker — Recommended for Integration Testing)

```bash
cp .env.example .env
# Fill in GROQ_API_KEY, SERVER_SECRET, ADMIN_PASSWORD

docker compose up --build
# Dashboard: http://localhost:3000
# Engine API: http://localhost:8000/docs
```

### Run the Attack Simulator

```bash
# In a separate terminal while the stack is running:
cd tools
node attack-simulator.js --user attack-test-001 --duration 15
# Watch the dashboard escalate through tiers in real time
```

---

## Branching & Commit Conventions

### Branch Naming

Branches **must** follow this pattern: `<type>/<short-description>`

```
feat/isolation-forest-a-score
fix/tier3-rekor-submission-timeout
docs/plugin-contribution-guide
test/scoring-e-score-edge-cases
chore/upgrade-sentence-transformers-3.x
refactor/defence-pipeline-seed-locking
```

Work exclusively on feature branches. **Never push directly to `main`.** All changes enter through pull requests.

### Commit Message Format

Vyse follows [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

```
<type>(<scope>): <short summary in imperative mood>

[optional body — wrap at 72 chars]

[optional footer: BREAKING CHANGE / Closes #issue]
```

**Allowed types:**

| Type | When to Use |
|---|---|
| `feat` | A new feature visible to users or API consumers |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `test` | Adding or fixing tests — no production code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `chore` | Build process, dependency updates, CI changes |
| `security` | Security hardening that isn't a bug fix (e.g. tightening CORS) |

**Allowed scopes:** `engine`, `gateway`, `dashboard`, `scoring`, `defence`, `rekor`, `auth`, `db`, `plugin`, `docker`, `docs`, `ci`

**Examples:**

```
feat(scoring): add E-Score bigram entropy signal

Adds a third scoring signal measuring session query entropy.
Low entropy across the last 20 queries indicates repetitive
extraction patterns that V-Score and D-Score can miss.

Closes #42
```

```
fix(rekor): handle submission timeout with exponential backoff

Rekor submissions that time out now retry up to 3 times with
exponential backoff (1s, 2s, 4s) before enqueuing for async
retry. Prevents silent audit trail gaps under network load.
```

```
security(auth): reject JWTs signed with none algorithm

Closes #89
```

**Rules:**
- Use **imperative mood** in the summary: "add", "fix", "remove" — not "added", "fixes", "removing"
- Keep the summary line under **72 characters**
- Reference issues in the footer with `Closes #N` or `Ref #N`
- Breaking changes must include `BREAKING CHANGE:` in the footer with migration notes

### Keeping Your Branch Updated

```bash
# Fetch latest from upstream
git fetch upstream

# Rebase your branch — do NOT merge main into your feature branch
git rebase upstream/main

# If force-push is needed after rebase, always use --force-with-lease
# (protects against overwriting someone else's commits on the same branch)
git push origin your-branch --force-with-lease
```

Never use `git push --force`. Only ever `git push --force-with-lease`.

---

## Pull Request Process

### Before Opening a PR

- [ ] Run the full test suite locally: `make test`
- [ ] Run the linter: `make lint`
- [ ] If you changed `engine/`, verify the engine starts cleanly: `uvicorn app.main:app`
- [ ] If your change affects tier logic, run the attack simulator and check the output makes sense
- [ ] Update `CHANGELOG.md` under the `[Unreleased]` heading
- [ ] Update relevant documentation if behaviour changed

### PR Title

PR titles follow the same Conventional Commits format as commit messages.

### PR Description Template

When you open a PR, a template will be provided. Fill in all sections — don't delete them. The template asks for:

- **What** — a description of the change
- **Why** — the motivation; link to the issue if one exists
- **How** — any non-obvious implementation decisions
- **Test plan** — what you tested, how to reproduce your test
- **Security considerations** — how does this change affect the threat model? (Required for changes to `scoring/`, `defence/`, `auth/`, `rekor/`)
- **Breaking changes** — if any

### PR Scope

**Keep PRs focused.** One logical change per PR. If you find unrelated issues while working, open a separate issue (or PR) rather than bundling fixes.

A PR that touches `scoring/`, `defence/`, and `dashboard/` all at once will be asked to split up unless the changes are genuinely inseparable.

### Draft PRs

Open a draft PR early if you want early feedback on direction. Maintainers will comment on design without expecting the code to be complete. Move to "Ready for Review" when all checklist items are met.

### Review Process

- At least **1 maintainer approval** is required to merge
- PRs touching security-critical paths require **2 approvals**
- CI must pass (tests, lint, docker build)
- All reviewer comments must be resolved or explicitly acknowledged

Once approved and CI is green, a maintainer will merge. Contributors do not merge their own PRs.

---

## Testing Requirements

Vyse has three test layers. All three must pass before merging.

### Unit Tests (Python — pytest)

Location: `engine/tests/`

Run: `cd engine && pytest tests/unit/ -v`

**Required coverage for new code:** All new functions in `scoring/`, `defence/`, and `store/` must have unit tests. The CI enforces a minimum of **80% line coverage** on the `engine/app/` package.

```bash
# Run with coverage report
pytest tests/unit/ --cov=app --cov-report=term-missing
```

### Integration Tests

Location: `engine/tests/integration/`

These tests start a real FastAPI app, make requests, and verify the full request lifecycle — including tier escalation and Rekor submission (mocked by default, real Rekor optional via `REKOR_URL` env var).

Run: `cd engine && pytest tests/integration/ -v`

### End-to-End Tests

Run: `make test-e2e` — starts the full Docker Compose stack and runs the attack simulator, then asserts on the database state.

These are slower (~2 min) and run in CI on every PR. Run locally before any PR that touches the Docker configuration.

### What to Test

When adding a new scoring signal:
- Test with zero previous context (first query)
- Test at the exact tier transition boundaries (score == threshold)
- Test with malformed / empty input
- Test that legitimate-looking queries don't score above Tier 1 thresholds

When adding a new perturbation technique:
- Test that the output is different from the input
- Test that the seed-locked version produces identical output on re-run with the same seed
- Test on very short inputs (1 word) and very long inputs (1000+ tokens)

---

## Code Style & Linting

### Python (Engine)

Vyse uses **ruff** for linting and formatting (replaces flake8 + black + isort in one tool).

```bash
# Check
cd engine && ruff check app/ tests/

# Auto-fix
ruff check app/ tests/ --fix

# Format
ruff format app/ tests/
```

Configuration is in `engine/pyproject.toml`. Do not override it in your PR.

Key rules:
- Type hints are **required** on all public functions
- Async functions must have `async` in the signature — no sync DB calls in async handlers
- `print()` is banned in production code — use `structlog.get_logger()`
- No bare `except:` — always catch specific exceptions

### Node.js (Gateway)

```bash
cd gateway && npm run lint     # ESLint
cd gateway && npm run format   # Prettier
```

### React (Dashboard)

```bash
cd dashboard && npm run lint
cd dashboard && npm run format
```

### General Rules Across All Components

- No hardcoded credentials, API keys, tokens, or secrets anywhere in code
- No commented-out code in merged PRs — delete it or open a follow-up issue
- Environment-specific configuration belongs in `.env` files, not in source code
- All user-facing strings must be in English

---

## Contributing ML Models or Scoring Changes

This section applies to PRs that change, replace, or add ML models used in Vyse's scoring or defence pipeline. These changes have outsized security impact and require extra rigour.

### Why This Is Different

Vyse's effectiveness depends entirely on its scoring accuracy. A change to the embedding model, anomaly detector, or NLI classifier can:

- Increase false positives (legitimate users get degraded responses)
- Increase false negatives (attackers evade detection)
- Introduce adversarial exploitability (a new model may be more easily fooled)

### Required Before Proposing a Model Change

1. **Open an issue first.** Label it `scoring` + `rfc`. Describe the proposed change, your motivation, and your preliminary evidence. Wait for a maintainer to signal interest before writing code.

2. **Benchmark against the current model.** Run `tools/scoring_benchmark.py` and include the output in your PR. This script tests both models against the standard attack corpus and reports precision/recall on tier assignment.

3. **Check the model's license.** Vyse is GNU GENERAL PUBLIC licensed. The model must be Apache-2, MIT, or BSD licensed. Include the license in your PR description.

4. **CPU performance.** The model must be runnable on CPU within 200ms per request on a 2-core machine. Include your benchmark results.

5. **Size constraint.** The model must be under 500MB. Models requiring GPU are not suitable for the default deployment.

### Acceptable Model Proposals

- Replacing `all-MiniLM-L6-v2` for D-Score: only with a model that demonstrably improves semantic separation between attack and non-attack queries on the benchmark
- New scoring signal (new score type): must have a corresponding issue, RFC, and benchmark
- Updating a model version: straightforward — verify the API is unchanged and all tests pass

---

## Contributing a Plugin

Vyse's plugin system allows external threat detector modules to be loaded at startup from the `plugins/` directory. A plugin adds a new score signal, input filter, or post-processing step without modifying core engine code.

### Plugin Interface

```python
# All plugins must implement this ABC
from vyse.plugin import VyseThreatPlugin, PluginContext, PluginResult

class MyDetector(VyseThreatPlugin):
    name = "my-detector"          # unique slug, lowercase, hyphens only
    version = "1.0.0"
    author = "Your Name"
    license = "GNU GENERAL PUBLIC LICENSE"       # must be MIT / Apache-2 / BSD

    async def evaluate(self, ctx: PluginContext) -> PluginResult:
        """
        ctx contains: prompt, user_id_hash, session_history, base_scores
        Return: score in [0.0, 1.0] and optional metadata dict
        """
        ...
```

### Plugin PR Requirements

- The plugin must live in `plugins/<your-plugin-name>/`
- Include a `README.md` in the plugin directory explaining what it detects and why
- Include unit tests in `plugins/<your-plugin-name>/tests/`
- The plugin must not make external network requests without explicit documentation and opt-in configuration
- Declare all third-party dependencies in `plugins/<your-plugin-name>/requirements.txt`
- Plugins that use ML models: follow the [ML model requirements](#contributing-ml-models-or-scoring-changes) above

### Good Plugin Ideas

- IP reputation lookup (offline, using MaxMind GeoLite2 or similar FOSS DB)
- Prompt injection pattern detection (regex + NLP hybrid)
- Time-of-day anomaly detection
- Cross-session correlation (same attacker, different IDs)

---

## Documentation Contributions

Documentation lives in `docs/` (MkDocs source) and inline in code (docstrings).

### What Counts as Documentation

- `docs/` — user-facing guides, architecture explanations, deployment recipes
- `CONTRIBUTING.md` — this file
- Docstrings on all public functions in `engine/app/`
- Inline comments explaining *why*, not *what* (the code shows what)
- `CHANGELOG.md` — updated with every PR

### Documentation PRs

Pure documentation PRs (no code changes) are fast-tracked for review. They do not require the full test suite to pass, only the `docs/` build check:

```bash
cd docs && mkdocs build --strict
```

Spell-checking is enforced by CI using `cspell`. If you use a domain-specific term that triggers a false positive, add it to `docs/.cspell.json`.

### Changelog Format

Vyse follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

```markdown
## [Unreleased]

### Added
- E-Score entropy signal as third scoring dimension (#42)

### Fixed
- Rekor submission timeout now retries with exponential backoff (#89)

### Security
- Admin endpoints now require JWT; previously unauthenticated (#101)

### Breaking Changes
- `POST /api/chat` now requires `X-Vyse-Key` header (#95)
```

---

## Issue Labels Reference

| Label | Meaning |
|---|---|
| `bug` | Something is broken |
| `good first issue` | Self-contained, no security-critical paths, clear scope |
| `help wanted` | Maintainers want community input |
| `scoring` | Relates to V/D/E/A score or tier logic |
| `defence` | Relates to perturbation pipeline |
| `rekor` | Relates to transparency log / audit trail |
| `gateway` | Gateway / auth / rate-limiting |
| `dashboard` | React dashboard |
| `plugin` | Plugin system |
| `rfc` | Request for comments — design discussion before implementation |
| `security` | Security issue — maintainer-only until patched |
| `test-coverage` | Missing or insufficient tests |
| `docs` | Documentation improvement |
| `breaking` | Introduces a breaking API or config change |
| `needs-repro` | Bug report needs a reproduction case |
| `wontfix` | Out of scope or intentionally not changed |

---

## Review SLA

Maintainers aim for:

| PR Type | First response | Merge target |
|---|---|---|
| Documentation only | 2 business days | 3 business days |
| Bug fix (non-critical path) | 3 business days | 1 week |
| Bug fix (critical path) | 1 business day | 3 business days |
| New feature | 5 business days | 2 weeks |
| Scoring / model change | 1 week | After RFC discussion |
| Security fix | 24 hours | ASAP |

These are targets, not guarantees. If your PR has not received a response past the SLA, comment on it or tag a maintainer. We want to keep the review queue clear.

If no one has reviewed your PR within the SLA:
- Post a comment on the PR
- Tag `@vyse-security/maintainers`

---

## License & DCO

### License

Vyse is licensed under the **GNU GENERAL PUBLIC LICENSE**. By contributing, you agree that your contributions will be licensed under the same terms.

All plugin contributions must also be MIT, Apache-2, or GNU GENERAL PUBLIC LICENSE licensed.

### Developer Certificate of Origin (DCO)

All commits must be signed off, certifying that you wrote the code and have the right to contribute it under the MIT license.

```bash
git commit --signoff -m "feat(scoring): add E-Score signal"
# or shorthand:
git commit -s -m "feat(scoring): add E-Score signal"
```

This adds `Signed-off-by: Your Name <your@email.com>` to the commit. The DCO check is enforced by CI — unsigned commits will block the PR.

Full DCO text: [developercertificate.org](https://developercertificate.org/)

### No CLA Required

Vyse does not require a Contributor License Agreement. The DCO sign-off is sufficient. We believe CLAs create unnecessary friction for open-source contributors.

---

*Thank you for reading this far. That already puts you ahead of most contributors. Now go build something.*
