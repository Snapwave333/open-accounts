# Monetization plan — Nova Vexley

Rule from the studio handbook: nothing public, paid, or sold without Eli. Every row below that
creates an account, publishes a product, or spends money is an approval gate in the app's
"Needs You" tab. The pipeline tracks funnel metrics in `data/analytics.json` so the Growth Analyst
can see which stream is working.

## Streams, in order of launch

| # | Stream | Why it fits | What exists already | Approval needed | Eli cost |
| --- | --- | --- | --- | --- | --- |
| 1 | **Free link-in-bio page** (Carrd free tier, Linktree free, or a static GitHub Pages page) — wallpapers teaser + socials | Zero monthly fee; bio can link somewhere before any store exists | — | Eli: publish the page URL | **Free** (start here) |
| 2 | **Digital products** — 4K wallpaper packs, phone lock-screen sets, printable A3 poster PDFs (digital only; **original Nova looks only — no famous/fan character cosplays**) | Zero fulfilment, 100% margin; the generated images *are* the product | Optional later: studio Shopify (`config.json` → `merch.store_url`) **or** Gumroad/Payhip free plans | Eli: product listing + price | **Shopify is a paid plan** if used — treat as optional/later; prefer a free host until revenue justifies it |
| 3 | **Fanvue subscription tier** (AI creators are explicitly allowed) — premium *suggestive, non-explicit* sets, early access, polls | Recurring revenue; fans pay Nova, Eli is not charged to run the IG pipeline | — | Eli: account creation, payout identity (his), pricing; same compliance as Instagram | Free to list; fans pay the tier |
| 4 | **Affiliate links** on cosplay gear, wigs, corsetry, platform boots | Captions already talk about gear | Email inbox watch for programme emails | Eli: programme sign-up | Free |
| 5 | **Brand collaborations** (alt-fashion, cosplay suppliers, gaming peripherals) | Inbound once the feed looks premium | `email_watch` + sponsor triage in the studio | Eli: every deal | Free |
| 6 | **Instagram native** (Subscriptions, Bonuses) | Later; needs follower thresholds and Eli's identity for payouts | `ig_relay.stats` already logs the follower milestones | Eli: enrolment | Free |

Pipeline rule: nothing above requires Eli to pre-pay to run the Instagram photo pipeline (local ComfyUI + free NIM + Chrome relay). Monetization is opt-in after the account is live.

## Funnel the Growth Analyst tracks (`py pipeline.py analytics`)

- followers, posts, likes (relay read of the public profile, every 6 h; photos only — no reel plays)
- per-post: likes/comments at 24 h (public post page) → pillar performance → planner weights
- store: product views/orders for Nova's products (Shopify, once products exist)
- weekly: `data/analytics/weekly-YYYY-WW.md` written by the analyst with the three changes it will
  make to the next week's calendar (and it makes them)

## Pricing starting points (Eli decides)

- Wallpaper pack (8 images, 4K): $4.99
- Poster PDF (single, A3 300 dpi): $6.99
- Fanvue tier: $7.99/month



## Character IP / famous cosplays — two-platform rule (updated 2026-11-03)

Eli's update: **cosplay goes to Fanvue too** — but only as **IP-safe parody**: the look
(silhouette, palette, props) *suggests* the character; the character's name, franchise,
studio or game never appear in Fanvue prompts, captions or tags. Instagram keeps the full
named-recreation treatment for feed engagement.

- **Instagram**: `adult fan cosplay of X` prompts + character names in captions — unchanged.
- **Fanvue**: `character.image_prompt(ip_safe=True)` + `strip_character_names()`; the
  banned-name list comes from the curated pool (`fan_cosplay_name_patterns()`), so
  `py pipeline.py addcosplay NAME` extends it automatically. The compliance gate blocks any
  caption/prompt that leaks a name before upload.
- **Never** render a replica of a protected design on the paid surface — homage styling only.
- The QC judge + weekly policy agent are the backstops.

## Not doing

- Explicit content anywhere; the character is suggestive, never explicit.
- Pretending she is real: the disclosure is in the bio and pinned post, and stays.
- Paid follower growth, engagement pods, or anything that bypasses Instagram automation policy.
