# Nova Vexley — operations

An AI-generated adult cosplay influencer run by the OpenAccounts agent runtime. Fictional,
labelled as AI everywhere, never explicit. Owner: Eli. Nothing here is simulated: every step
runs locally (ComfyUI) or on free tiers, or stops with a reason.

## Layout

| Path | What |
| --- | --- |
| `character/identity.md`, `character/character.json` | Creative Director + Character Designer package: who she is and the reproducible appearance tokens every prompt starts with |
| `brand/instagram.md` | handle candidates, bio, profile image, formats, posting rhythm |
| `brand/monetization.md` | revenue streams in launch order, approval gates, pricing starting points |
| `oa_influencer/` | the pipeline (Python 3.13, stdlib + Pillow + imagehash) |
| `pipeline.py` | CLI the runtime calls (`py pipeline.py --help`) |
| `data/` (git-ignored) | calendar, assets, reference set, approvals, ledger, analytics, logs |

## Services

| Need | Service | Key | Cost |
| --- | --- | --- | --- |
| Photos | Local ComfyUI at `C:\Users\chrom\ComfyUI` (venv `C:\Users\chrom\ComfyUI-venv`): RealVisXL 5 (SDXL) + IP-Adapter FaceID Plus v2 for identity; started per job with `--lowvram` and stopped after; the studio's on-demand voice server is stopped first when it holds the VRAM | none | free (RTX 4070, ~1–3 min per image) |
| QC judge, captions | NVIDIA NIM `nemotron-3.5-super-vl` | `NVIDIA_API_KEY` (already set) | free |
| Posting, profile stats, handle checks | the studio's Chrome relay (`StickExplainers/ig_relay.py`) on Nova's account via Instagram's profile switcher | Eli signed in once | free |

## Human gates (Needs You tab in the app, or `py pipeline.py approve <gate>`)

1. `reference_approved` — the six reference images in `data/reference/` look right
2. `account_created` — Eli finishes the sign-up form the agent opened (terms, CAPTCHA, email code); the agent verifies and grants this one itself
3. Publish gate — removed (Eli, 2026-11-03): QC-approved ready posts go out on cadence with no human step.

Each gate blocks only what depends on it. Revoke with `py pipeline.py revoke <gate>`.
`wrong-account` from the relay (Nova not signed in) revokes `account_created` automatically.

## What runs when (OpenAccounts autonomy runtime, `apps/desktop/src-tauri/src/autonomy.rs`)

| Job | Cadence | Does |
| --- | --- | --- |
| Nova plan | every cycle (30 min) | rolling 30-day calendar, Needs You list |
| Nova handles | 6 h until chosen | read-only profile lookups to pick the first free handle |
| Nova account | hourly until done | writes `data/account_kit.md`, opens sign-up in Chrome (once per 6 h), verifies the account is signed in, grants `account_created` itself, fills name/bio/photo |
| Nova reference | hourly until done | the 6-image reference set, then waits for gate 1 |
| Nova generate | 6 h | max-size carousel shoots: 20 distinct full-body/seductive poses per post, one outfit, frames rendered + QC'd incrementally (8/run) across runs → captions (gate 1) |
| Nova publish | 50 min | the next due ready slot posts a FULL 20-image carousel (one relay upload call), 1 feed post/day, ≥20 h apart — autonomous, no publishing gate (Eli, 2026-11-03) |
| Nova replies | 1 h | comments on Nova's posts → drafted + classified by the judge in Nova's exact voice (sign-off, AI disclosure), deterministic `safe()` last word, ≤10 sends/day; spam/toxic/personal-info are never answered. Then one growth tick: follows curated female cosplay creators (≤5/day, verified live profiles only) |
| Nova analytics | 6 h | followers/posts snapshot; weekly pillar-weight review |
| Nova GC | 24 h | deletes posted media 24 h after confirmation and stale rejects; keeps thumbnails + metadata |
| Nova fanvue policy | 24 h* | re-fetches Fanvue's ToS/community guidelines from legal.fanvue.com and re-distills the compliance ruleset (internal 7-day freshness gate; *the command exits instantly while the policy is fresh, so the daily cadence only pays when research is due or the last run failed) |
| Nova fanvue calendar | 24 h | rolling 30-day Fanvue calendar: 1–2 posts/day, 6 days/week, 5–10-image shoots, 3–5 PPV releases/week, 11:00–13:00 / 18:00–21:00 America/Denver test windows |
| Nova fanvue generate | 6 h | shoots for due Fanvue slots (PPV sets assemble from the inventory first, GPU only when the pool is thin) |
| Nova fanvue publish | 50 min | real API publish of the next due slot: policy gate → multipart upload → create-post (PPV price + preview + native scheduling) → verification against Fanvue's own post list |
| Nova fanvue report | 24 h | weekly CEO report from real earnings/subscriber/publishing data (data/fanvue_weekly_report.md) |

## Two platforms, one Nova (Eli, 2026-11-03)

| | Instagram | Fanvue |
| --- | --- | --- |
| Role | top of funnel: 20-image carousels, engagement, growth | the spicy, monetized surface fans "have to go to" |
| Cadence | 1 feed post/day (≥20 h gap) | 1–2 posts/day, 6 days/week (≥4 h gap), 3–5 PPV/week |
| Shoots | max-size carousels | 5–10-image themed shoots; leftovers feed the inventory |
| Cosplay IP | full named recreations | **IP-safe parody only** — the look suggests the character, names never appear |
| Monetization | none (funnel) | subscriptions, PPV unlocks, tips via the official API only |
| Policy | identity.md + IG rules | `character/fanvue/policy.json` (see `character/fanvue/README.md`), enforced by `oa_influencer/fanvue_policy.py` before every upload — fail closed |

Human gates (Needs You tab): Fanvue publishing authorization, subscription/pricing changes,
custom-content requests, unusual and mass messaging. The Fanvue view (Ctrl+6) shows the
calendar, approval queue, compliance status and the weekly report.

## QC (every image, `oa_influencer/qc.py`)

Technical (size, aspect), near-duplicate (perceptual hash vs everything ever approved), and the
vision judge against the approved reference set: identity ≥ 75, reads as adult, no nudity or
explicit pose, hands intact, no text/watermark, quality ≥ 70, outfit matches. The judge fails
closed: if NIM is down the image stays unapproved.

## Compliance

See `character/identity.md` → Compliance rules. Prompts are checked for forbidden words before
generation; the vision judge rejects any nudity or explicit pose after it.

## Photos only

No Reels, no video. Photos only (Eli, 2026-11-02). The Open Higgsfield clone in `../open-higgsfield` is a UI
over the paid Higgsfield API and is not used by the pipeline.
