"""Fanvue Content Strategist agent: the Fanvue-only publishing calendar, monetization mix,
pricing recommendations and schedule optimization.

Two-platform funnel (Eli, 2026-11-03): Instagram keeps the 20-image carousel cadence and
platform engagement as the top of the funnel; Fanvue is where the spicy, monetized content
lives - fans who want more than the feed go there. This agent plans that surface:

- 1-2 posts daily, 5-7 days a week (default 6, Sunday dark)
- every post is a themed photoshoot of 5-10 images with varied poses, expressions,
  outfits, settings and camera angles under the locked identity (shared refs + QC)
- 3-5 premium pay-per-view releases weekly, curated from the content inventory
  (fanvue_inventory) rather than flooding subscribers
- tier mix: free promotional posts, subscriber-only posts, separately priced PPV
- test posting windows 11:00-13:00 and 18:00-21:00 America/Denver; the weekly optimizer
  shifts weights from real engagement/unlock data once it exists
- cosplay runs on Fanvue as IP-safe parody only: the look suggests the character, the
  name/franchise never appears (fanvue_policy enforces it on every caption and prompt)

Nothing here simulates publishing: the calendar is a plan; the Fanvue publisher agent
executes real API posts against it.
"""

from __future__ import annotations

import datetime as dt
import json
import random
from pathlib import Path
from typing import Any

from . import character, config, fanvue_client, fanvue_inventory, fanvue_policy, nim, planner, state

try:
    from zoneinfo import ZoneInfo
    _DENVER = ZoneInfo(config.FANVUE_TIMEZONE)
except Exception:  # Windows without tzdata: fall back to the fixed US Mountain DST rule
    _DENVER = None


# ---- America/Denver helpers ---------------------------------------------------

def _us_mountain_offset(utc: dt.datetime) -> dt.timedelta:
    """US Mountain Time offset for a UTC moment (DST: 2nd Sun Mar 09:00Z - 1st Sun Nov 08:00Z)."""
    if _DENVER is not None:
        return utc.astimezone(_DENVER).utcoffset() or dt.timedelta(hours=-7)
    year = utc.year

    def nth_sunday(month: int, n: int) -> int:
        d = dt.date(year, month, 1)
        return d.day + (6 - d.weekday()) % 7 + 7 * (n - 1)

    start = dt.datetime(year, 3, nth_sunday(3, 2), 9, 0)   # 02:00 MST == 09:00 UTC
    end = dt.datetime(year, 11, nth_sunday(11, 1), 8, 0)   # 02:00 MDT == 08:00 UTC
    return dt.timedelta(hours=-6) if start <= utc < end else dt.timedelta(hours=-7)


def denver_wall(utc: dt.datetime) -> dt.datetime:
    """Naive wall-clock time in America/Denver for a UTC moment."""
    return utc + _us_mountain_offset(utc)


def schedule_at(day: dt.date, hour: int, minute: int) -> str:
    """Aware ISO timestamp for a Denver wall-clock time on `day` (correct DST offset)."""
    if _DENVER is not None:
        return dt.datetime.combine(day, dt.time(hour, minute), _DENVER).isoformat(timespec="seconds")
    noon_utc = dt.datetime(day.year, day.month, day.day, 19, 0)
    off = _us_mountain_offset(noon_utc)
    tz = dt.timezone(off)
    return dt.datetime.combine(day, dt.time(hour, minute), tz).isoformat(timespec="seconds")


def window_slot_time(day: dt.date, window: str, rng: random.Random) -> tuple[int, int]:
    """A minute inside one of the test windows: midday 11:00-13:00, evening 18:00-21:00."""
    lo_h, lo_m, hi_h, hi_m = config.FANVUE_TEST_WINDOWS[window]
    start = lo_h * 60 + lo_m
    end = hi_h * 60 + hi_m
    minute = rng.randint(start, end - 1)
    return minute // 60, minute % 60


# ---- strategy constants --------------------------------------------------------

STRATEGY = {
    "posts_per_day": (1, 2),
    "posting_days_per_week": 6,          # 5-7 allowed; 6 => Sunday dark
    "double_days": (4, 5),               # Fri, Sat get the second post
    "shoot_images": (config.FANVUE_SHOOT_MIN_IMAGES, config.FANVUE_SHOOT_MAX_IMAGES),
    "ppv_per_week": 4,                   # 3-5 allowed
    "ppv_days": (1, 3, 5, 6),            # Tue, Thu, Sat, Sun evenings
    "ppv_window": "evening",
    "tier_weights": {"free": 60, "subscriber": 40},
    # Spicier mix than Instagram (Eli: "they have to go to Fanvue for spicy"):
    # glam/lingerie-adjacent styling leads; cosplay stays via IP-safe parody.
    "pillar_weights": {"glam": 30, "cosplay": 20, "famous_cosplays": 20,
                       "altfashion": 10, "bts": 10, "community": 10},
}


class FanvueContentStrategist:
    """Plans the Fanvue publishing calendar and monetization mix."""

    FANVUE_STATE = config.DATA / "fanvue_calendar.json"

    def __init__(self) -> None:
        self.client = fanvue_client.FanvueClient()

    # ---- Fanvue content calendar (separate from Instagram) -------------------

    def load_calendar(self) -> dict[str, Any]:
        return state.read(self.FANVUE_STATE,
                          {"slots": [], "generated_at": None, "weights": {}})

    def save_calendar(self, cal: dict[str, Any]) -> None:
        state.write_atomic(self.FANVUE_STATE, cal)

    # -- slot construction -------------------------------------------------------

    def _base_slot(self, day: dt.date, window: str, rng: random.Random, spec: dict) -> dict:
        pillar_id = planner._pick_pillar(rng, STRATEGY["pillar_weights"])
        theme_id = self._pick_theme(spec, pillar_id, rng)
        ip_safe = pillar_id == "famous_cosplays" or theme_id.startswith("fc:")
        pose = character.pick_pose(spec, rng)
        shoot_target = rng.randint(*STRATEGY["shoot_images"])
        hour, minute = window_slot_time(day, window, rng)
        return {
            "id": "",
            "platform": "fanvue",
            "kind": "feed",
            "tier": self._pick_tier(rng),
            "date": day.isoformat(),
            "scheduled_at": schedule_at(day, hour, minute),
            "window": window,
            "pillar": pillar_id,
            "theme": theme_id,
            "ip_safe": ip_safe,
            "pose": pose,
            "glam": rng.random() < 0.45,
            "aspect_ratio": spec["aspect_ratios"]["feed"],
            "shoot": {"target": shoot_target},
            "images": shoot_target,
            "status": "planned",
            "assets": [],
            "caption": None,
            "story": self._story(spec, pillar_id, theme_id, rng),
            "hashtags": self._hashtags(spec, pillar_id, rng, ip_safe),
            "unlock_price_cents": None,
            "promo_copy": None,
            "attempts": 0,
        }

    def _pick_theme(self, spec: dict, pillar_id: str, rng: random.Random) -> str:
        ids = character.theme_ids(spec, pillar_id)
        return rng.choice(ids) if ids else pillar_id

    def _pick_tier(self, rng: random.Random) -> str:
        w = STRATEGY["tier_weights"]
        return rng.choices(list(w), weights=list(w.values()))[0]

    def _story(self, spec: dict, pillar_id: str, theme_id: str, rng: random.Random) -> str:
        t = character.theme(spec, pillar_id, theme_id)
        hook = rng.choice(spec["voice"]["catchphrases"])
        return f"{t['mood']} set in {t['setting']}. {hook}"

    def _hashtags(self, spec: dict, pillar_id: str, rng: random.Random, ip_safe: bool) -> list[str]:
        pools = spec["hashtag_pools"]
        core = rng.sample(pools["core"], min(4, len(pools["core"])))
        own = rng.sample(pools.get(pillar_id, []), min(3, len(pools.get(pillar_id, []))))
        tags = core + own
        if not ip_safe:
            return tags
        # IP-safe slots never carry character/franchise tags.
        cleaned = []
        for tag in tags:
            if not character.fan_cosplay_name_patterns(spec) or not _hits_any(tag, spec):
                cleaned.append(tag)
        return cleaned

    # -- calendar build ------------------------------------------------------------

    def build_calendar(self, today: dt.date | None = None) -> dict[str, Any]:
        """Rolling 30-day Fanvue calendar: 1-2 feed posts/day on 6 days (Sun dark) plus
        3-5 curated PPV releases/week in the evening window."""
        spec = character.spec()
        cal = self.load_calendar()
        slots = {s["id"]: s for s in cal["slots"]}
        today = today or denver_wall(state.now().astimezone(dt.timezone.utc)).date()
        horizon = today + dt.timedelta(days=30)
        days_per_week = STRATEGY["posting_days_per_week"]
        dark_days = set(range(7 - days_per_week)) if days_per_week < 7 else set()
        dark = {6} if days_per_week == 6 else dark_days  # Sunday dark on a 6-day week

        added = 0
        for offset in range(31):
            day = today + dt.timedelta(days=offset)
            if day > horizon:
                break
            weekday = day.weekday()
            day_rng = random.Random(f"nova-fanvue:{day.isoformat()}")
            # feed slots: 1-2 per posting day
            count = 1 + (1 if weekday in STRATEGY["double_days"] else 0)
            windows = ["midday", "evening"][:count] if count == 1 else ["midday", "evening"]
            for i, window in enumerate(windows):
                slot_id = f"fanvue-{day.isoformat()}-feed-{i + 1}"
                if slot_id in slots or weekday in dark:
                    continue
                slot = self._base_slot(day, window, day_rng, spec)
                slot["id"] = slot_id
                slots[slot_id] = slot
                added += 1
            # PPV slots: 3-5/week on the designated evenings
            if weekday in STRATEGY["ppv_days"] and weekday not in dark:
                week_index = (day - today).days // 7
                slot_id = f"fanvue-{day.isoformat()}-ppv"
                if slot_id in slots:
                    continue
                slot = self._base_slot(day, STRATEGY["ppv_window"], day_rng, spec)
                slot["id"] = slot_id
                slot["kind"] = "ppv"
                slot["tier"] = "ppv"
                slot["scheduled_at"] = schedule_at(day, 19, 30)
                slot["unlock_price_cents"] = self.recommend_ppv_price_cents(slot["images"])
                slots[slot_id] = slot
                added += 1

        cal["slots"] = sorted(slots.values(), key=lambda s: s["scheduled_at"])
        cal["generated_at"] = state.iso()
        cal["weights"] = {"tiers": STRATEGY["tier_weights"], "pillars": STRATEGY["pillar_weights"],
                          "posting_days_per_week": days_per_week, "ppv_per_week": STRATEGY["ppv_per_week"]}
        self.save_calendar(cal)
        return {"added": added, "total": len(cal["slots"]), "weights": cal["weights"]}

    # -- generation pipeline -----------------------------------------------------

    def due_for_generation(self, cal: dict[str, Any] | None = None) -> list[dict[str, Any]]:
        """Slots inside the lead window that still need work: planned feed shoots and
        PPV slots not yet assembled from inventory."""
        cal = cal or self.load_calendar()
        horizon = state.now() + dt.timedelta(days=config.GENERATION_LEAD_DAYS)
        out = []
        for s in cal["slots"]:
            if s["status"] not in ("planned", "rejected", "assembled"):
                continue
            when = dt.datetime.fromisoformat(s["scheduled_at"])
            if when <= horizon:
                out.append(s)
        return out

    def assemble_ppv(self, slot: dict) -> dict:
        """Curates a PPV release from the inventory (5-10 carefully selected frames of one
        theme, near-duplicate filtered). Falls back to a fresh shoot when the pool is thin."""
        target = slot.get("images") or STRATEGY["shoot_images"][0]
        got = fanvue_inventory.available(target, theme=slot.get("theme"))
        if len(got) < config.FANVUE_SHOOT_MIN_IMAGES:
            got = fanvue_inventory.available(target)
        if len(got) < config.FANVUE_SHOOT_MIN_IMAGES:
            slot["status"] = "planned"   # no pool: generate_slot will shoot it fresh
            return slot
        slot["assets"] = [{
            "id": a["id"], "path": a["path"], "approved": True, "reasons": [],
            "verdict": "inventory", "prompt": "", "pose": "inventory",
        } for a in got]
        slot["images"] = len(got)
        slot["unlock_price_cents"] = self.recommend_ppv_price_cents(len(got))
        slot["promo_copy"] = (f"premium set: {len(got)} frames, {slot['theme']}")
        fanvue_inventory.reserve([a["id"] for a in got], slot["id"])
        slot["status"] = "assembled"
        slot["inventory_ids"] = [a["id"] for a in got]
        state.log(f"ppv: {slot['id']} assembled {len(got)} frames from inventory")
        return slot

    def prepare_due(self, cal: dict[str, Any] | None = None) -> dict:
        """Assembles PPV releases from inventory before the GPU loop runs."""
        cal = cal or self.load_calendar()
        assembled = 0
        for slot in self.due_for_generation(cal):
            if slot["kind"] == "ppv" and slot["status"] == "planned":
                self.assemble_ppv(slot)
                assembled += 1
        self.save_calendar(cal)
        return {"assembled": assembled}

    # -- pricing -------------------------------------------------------------------

    def recommend_ppv_price_cents(self, image_count: int) -> int:
        """Cents for a PPV set, scaled by size, inside Fanvue's 300-50000 range."""
        low, high = config.FANVUE_PPV_PRICE_CENTS
        span = high - low
        size = max(config.FANVUE_SHOOT_MIN_IMAGES, min(config.FANVUE_SHOOT_MAX_IMAGES, image_count))
        price = low + int(span * (size - config.FANVUE_SHOOT_MIN_IMAGES) /
                          max(1, config.FANVUE_SHOOT_MAX_IMAGES - config.FANVUE_SHOOT_MIN_IMAGES))
        return max(300, min(50000, price))

    def subscription_price_cents(self) -> int:
        """Nova's monthly Fanvue subscription price (USD cents)."""
        st = state.state()
        return int(st.get("fanvue_subscription_price_cents", config.FANVUE_SUBSCRIPTION_CENTS))

    def set_subscription_price(self, cents: int) -> dict:
        """Applies a subscription price change. Financial action: requires the
        fanvue_pricing_change gate and uses the official API (forceOptIn stays False so
        existing subscribers keep their price until they opt in)."""
        if not state.granted("fanvue_pricing_change"):
            raise PermissionError("waiting for approval: fanvue_pricing_change (pricing is a financial decision)")
        if not (399 <= cents <= 10000):
            raise ValueError("Fanvue subscription price must be 399-10000 USD cents")
        old = self.subscription_price_cents()
        report = self.client.update_subscription_price(cents)
        st = state.state()
        st["fanvue_subscription_price_cents"] = cents
        st["fanvue_subscription_price_changed_at"] = state.iso()
        state.save_state(st)
        state.log(f"fanvue strategist: subscription price {old} -> {cents} cents")
        return {"old": old, "new": cents, "fanvue": report}

    # -- weekly optimization ---------------------------------------------------------

    def optimize(self) -> dict:
        """Shifts window/double-day weights from REAL performance data (written by the
        revenue report). No data yet -> no changes, stated honestly."""
        data = state.read(state.ANALYTICS, {}).get("fanvue", {})
        posts = data.get("posts") or []
        by_window: dict[str, list[int]] = {"midday": [], "evening": []}
        for p in posts:
            score = int(p.get("unlocks") or 0) * 10 + int(p.get("likes") or 0)
            if p.get("window") in by_window and (p.get("unlocks") is not None or p.get("likes") is not None):
                by_window[p["window"]].append(score)
        if len(by_window["midday"]) + len(by_window["evening"]) < 6:
            return {"result": "no-change", "reason": f"need >=6 scored posts for optimization, have "
                                                     f"{len(by_window['midday']) + len(by_window['evening'])}"}
        med = {w: (sorted(v)[len(v) // 2] if v else 0) for w, v in by_window.items()}
        better = max(med, key=med.get)
        changes = []
        if better != "evening":  # default: PPV rides the evening window
            STRATEGY["ppv_window"] = better
            changes.append(f"PPV window -> {better} (median score {med[better]})")
        state.log(f"fanvue strategist: optimization {changes or 'none'}")
        return {"result": "changed", "changes": changes, "medians": med}


def _hits_any(tag: str, spec: dict) -> bool:
    lowered = tag.lower()
    return any(p in lowered for p in character.fan_cosplay_name_patterns(spec))
