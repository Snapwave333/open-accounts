# Monetization plan — Nova Vexley

Rule from the studio handbook: nothing public, paid, or sold without Eli. Every row below that
creates an account, publishes a product, or spends money is an approval gate in the app's
"Needs You" tab. The pipeline tracks funnel metrics in `data/analytics.json` so the Growth Analyst
can see which stream is working.

## Streams, in order of launch

| # | Stream | Why it fits | What exists already | Approval needed |
| --- | --- | --- | --- | --- |
| 1 | **Digital products on the studio's Shopify store** — 4K wallpaper packs, phone lock-screen sets, printable A3 poster PDFs (not physical prints: per the studio rule, digital only) | Zero fulfilment, 100% margin, the generated images *are* the product; "wallpapers ↓" in the bio converts fans who like a look | Shopify store (studio `config.json` → `merch.store_url`), Shopify MCP for product creation | Eli: product listing + price |
| 2 | **Link-in-bio hub as a page on that Shopify store** (`/pages/nova`) | No new account, no Linktree fees; one link, tracked by Shopify analytics | Shopify store | Eli: page publish |
| 3 | **Fanvue subscription tier** (AI creators are explicitly allowed there) — premium *suggestive, non-explicit* sets, early access, polls on next cosplay | Recurring revenue; audience that follows an AI alt model is already there | — | Eli: account creation, payout identity (his), pricing; content stays within the same compliance rules as Instagram |
| 4 | **Affiliate links** on cosplay gear, wigs, corsetry, platform boots (Amazon Associates / brand programmes) | Captions already talk about gear; "the harness is linked" is natural | Email inbox watch for programme emails | Eli: programme sign-up |
| 5 | **Brand collaborations** (alt-fashion, cosplay suppliers, gaming peripherals) | Inbound once the feed looks premium; the studio's `sponsors` inbox flow already exists | `email_watch` + sponsor triage in the studio | Eli: every deal |
| 6 | **Instagram native** (Subscriptions, Bonuses) | Later; needs follower thresholds and Eli's identity for payouts | `ig_relay.stats` already logs the follower milestones | Eli: enrolment |

## Funnel the Growth Analyst tracks (`py pipeline.py analytics`)

- followers, posts, reel plays (relay read of the public profile, every 6 h)
- per-post: likes/comments at 24 h (public post page) → pillar performance → planner weights
- store: product views/orders for Nova's products (Shopify, once products exist)
- weekly: `data/analytics/weekly-YYYY-WW.md` written by the analyst with the three changes it will
  make to the next week's calendar (and it makes them)

## Pricing starting points (Eli decides)

- Wallpaper pack (8 images, 4K): $4.99
- Poster PDF (single, A3 300 dpi): $6.99
- Fanvue tier: $7.99/month

## Not doing

- Explicit content anywhere; the character is suggestive, never explicit.
- Pretending she is real: the disclosure is in the bio and pinned post, and stays.
- Paid follower growth, engagement pods, or anything that bypasses Instagram automation policy.
