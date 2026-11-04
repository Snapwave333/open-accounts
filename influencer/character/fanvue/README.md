# Fanvue compliance — research corpus + machine ruleset

The system that keeps Nova's Fanvue account alive. Nothing that would go to Fanvue is
generated or published without passing through it, and it fails closed.

## What lives here

| Path | What |
| --- | --- |
| `raw/*.md` | The actual Fanvue policy documents (Community Guidelines, Acceptable Use Policy, Creator Terms, Marketing Policy, API Access & Usage Policy, Content Moderation & Protection Policy, Creator Earnings & Payouts), fetched from legal.fanvue.com, which serves agent-friendly markdown. Re-fetched by the research job; content hashes detect changes. |
| `raw/sources.json` | Provenance: URL, fetch time, bytes, sha256 per document. |
| `policy.json` | The machine-enforceable ruleset distilled from those documents: banned text rules (with citations), required disclosures, public-media standards, messaging rules, API constraints, money facts. Every rule cites the document section it came from. |
| `review.md` | The latest agent research review: what changed, required actions for Eli, notes. |

## How it runs

- **The agents** (`oa_influencer/fanvue_policy.py` + NIM `nemotron-3.5-super-vl`) re-fetch the
  corpus, verify every rule against the raw documents, add missing rules, and write
  `review.md`. Runs on the **"Nova fanvue policy"** autonomy job (daily cadence, 7-day
  internal freshness gate — cheap unless research is due or the last run failed).
- **The gate** audits every Fanvue caption, message, bio and prompt **before** upload:
  banned text (minors/age-bait, external payments, platform migration, weapons, extreme
  themes, explicit text per Nova's house rule, PII, AI-denial), missing AI disclosure,
  missing 18+ markers for Intimate styling (Fanvue counts lingerie/underwear/see-through),
  and IP-safe cosplay naming. Missing/corrupt policy ⇒ everything blocks.
- **`py pipeline.py fanvue-check "text" --kind caption`** audits any text on demand.
- **`py pipeline.py fanvue-policy --force`** runs the full research cycle now.

## The two-platform rule (Eli, 2026-11-03)

- **Instagram**: unchanged 20-image carousel cadence + engagement. Full named fan-cosplay
  recreations stay allowed there for feed engagement.
- **Fanvue**: the spicy, monetized surface. Cosplay IS allowed here — but only as **IP-safe
  parody**: the look (silhouette, palette, props) suggests the character; the character's
  name, franchise, studio or game never appear in prompts, captions or tags. Enforced by
  `character.fan_cosplay_name_patterns()` (derived from the curated pool, so `addcosplay`
  extends it) + defensive stripping after caption generation. Source: Fanvue CG A.1.2 (IP).

## Hard account facts the agents must respect

- KYC (government ID + liveness) is Eli's, done manually once. Never automated.
- The explicit-content declaration made at onboarding is **locked** — it must match what
  the pipeline actually publishes (default: non-explicit, suggestive only).
- Public media (profile picture, banner, intro, public bio) is SFW-only, stricter than posts.
- Off-platform payments = suspension. Platform-migration talk = prohibited. Never.
- Appeals: 14 days, support@fanvue.com. Moderation can deduct balance.
