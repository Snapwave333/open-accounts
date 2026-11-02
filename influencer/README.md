# Nova Vexley — operations

An AI-generated adult cosplay influencer run by the OpenAccounts agent runtime. Fictional,
labelled as AI everywhere, never explicit. Owner: Eli. Nothing here is simulated: every step
calls a real service or stops with a reason.

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
| Photos (Soul 2) and reels (Kling image-to-video) | Higgsfield platform API, `https://api.higgsfield.ai` — the same API the Open Higgsfield studio in `../open-higgsfield` submits to (`pnpm dev` there for a UI with the same key) | `HIGGSFIELD_API_KEY` = `id:secret`, user environment variable, from console.higgsfield.ai | paid; every request is estimated via `/estimate/{model}` and refused past `OPENACCOUNTS_INFLUENCER_DAILY_USD` (default $3) or `_RUN_USD` ($1.50) |
| QC judge, captions | NVIDIA NIM `nemotron-3.5-super-vl` | `NVIDIA_API_KEY` (already set) | free |
| Posting, profile stats, handle checks | the studio's Chrome relay (`StickExplainers/ig_relay.py`) on Nova's account via Instagram's profile switcher | Eli signed in once | free |

## Human gates (Needs You tab in the app, or `py pipeline.py approve <gate>`)

1. `spend_authorized` — key added and credits funded
2. `reference_approved` — the six reference images in `data/reference/` look right
3. `account_created` — Eli created the Instagram account for the chosen handle in his Chrome
4. `publishing_authorized` — Eli looked at the first ready posts

Each gate blocks only what depends on it. Revoke with `py pipeline.py revoke <gate>`.
`wrong-account` from the relay (Nova not signed in) revokes `account_created` automatically.

## What runs when (OpenAccounts autonomy runtime, `apps/desktop/src-tauri/src/autonomy.rs`)

| Job | Cadence | Does |
| --- | --- | --- |
| Nova plan | every cycle (30 min) | rolling 30-day calendar, Needs You list |
| Nova handles | 6 h until chosen | read-only profile lookups to pick the first free handle |
| Nova reference | hourly until done | the 6-image reference set (gate 1) |
| Nova generate | 6 h | images for the next 3 days of slots → QC → captions (gates 1+2) |
| Nova reels | 12 h | image-to-video for ready reel slots |
| Nova publish | 50 min | the next due ready slot, 1 feed post/day, ≥20 h apart (gates 3+4) |
| Nova analytics | 6 h | followers/posts snapshot; weekly pillar-weight review |
| Nova GC | 24 h | deletes posted media 24 h after confirmation and stale rejects; keeps thumbnails + metadata |

## QC (every image, `oa_influencer/qc.py`)

Technical (size, aspect), near-duplicate (perceptual hash vs everything ever approved), and the
vision judge against the approved reference set: identity ≥ 75, reads as adult, no nudity or
explicit pose, hands intact, no text/watermark, quality ≥ 70, outfit matches. The judge fails
closed: if NIM is down the image stays unapproved.

## Compliance

See `character/identity.md` → Compliance rules. Prompts are checked for forbidden words before
submission; a provider `nsfw` verdict discards the request (unbilled) and is logged.
