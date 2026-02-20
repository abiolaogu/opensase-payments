# PLAN.md

## Repo
- Name: `opensase-payments`
- Vertical: `FinTech`
- Core language: `Rust`
- Benchmark targets: `Stripe, Adyen`

## Audit Summary (2026-02-20)
- Pulsar references found: `0`
- Quickwit references found: `0`
- REST/polling indicators found: `0`

## Engineering Plan
1. Standardize event boundaries on Apache Pulsar topics and remove high-frequency REST polling paths.
2. Standardize observability through Quickwit with a shared log schema and index strategy.
3. Align Kubernetes manifests for Harvester HCI with Mayastor/Vitastor compatible storage classes.
4. Add compliance and API contracts as code artifacts.

## Tech Stack Evolution
- Recommendation: Current core stack acceptable; prioritize Pulsar-native async boundaries and Quickwit ingestion.

## Autonomous Feature Expansion
- Proposed capability: Autonomous payment routing + real-time fraud graph scoring

## Figma Make Prompt
Task:
Build a premium enterprise dashboard for this repository with real-time Pulsar health and Quickwit global search.
Output:
Desktop + mobile-ready information architecture and component specification in Figma Make TOKEN format.
Key Elements:
Tenant-aware navigation, live event stream indicators, compliance evidence timeline, global search with filters.
Expected Behaviors:
Low-latency updates, resilient loading states, actionable alerts, and audit-first interactions.
Notable Constraints:
Use the repository's actual domain model, preserve existing design language, and keep performance-first rendering.

## Phase 2 Sovereign Execution

### Deep Audit Findings
- Risk score: 31 (low)
- Polling refs: 25
- N+1 refs: 6
- Sync IO refs: 0
- CPU hot refs: 0

### Tech Stack Evolution
- Proposed best stack: Go/Rust transaction services + Pulsar risk events + Quickwit evidence search
- Build baseline command: `cargo check --all-targets`
- Benchmark baseline command: `cargo bench (or criterion harness)`

### X+1 Feature Expansion
- Autonomous dispute+refund orchestration with real-time risk signal overlays

### Quality Gate Upgrades
1. Enforce perf profile execution in CI (`scripts/perf/profile.sh`).
2. Add language-appropriate security/dependency checks in CI.
3. Maintain docs-as-code synchronization and architecture drift checks.

### Rollout Strategy
1. Optimize top hotspots without API/event contract breaks.
2. Stage migration for only critical high-concurrency paths first.
3. Validate via benchmark deltas and rollback-safe deploys.

