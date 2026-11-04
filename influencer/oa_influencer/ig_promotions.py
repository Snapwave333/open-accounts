"""Coordinated Instagram -> Fanvue promotional campaigns.

Builds a rolling 30-day promotional calendar that ties Instagram surfaces
(Reel, Story, feed, bio link, pinned post) to specific Fanvue events:

* fanvue_release      - a new PPV set or subscriber-only post drops on Fanvue
* subscriber_milestone - a subscriber count milestone is reached (50, 100, 250...)
* intro_offer         - a limited-time introductory subscription discount
* themed_collection   - an exclusive themed content collection (holiday, event)

Each campaign registers UTM-tagged links per surface so ig_to_fanvue
attribution can credit clicks and conversions back to the campaign, and so the
weekly IG-to-Fanvue report can say which Reel, Story, theme or CTA drove
actual Fanvue visits and paid subscriptions.

Instagram stays the free discovery channel; Fanvue is the paid destination.
Every promo is Instagram-compliant: no explicit imagery, no prohibited
solicitation, no misleading claims, no spam, clear AI-generated disclosure,
and tasteful "link in bio -> Fanvue" CTAs. The two-platform rule holds:
Instagram content and Fanvue content remain in separate libraries with
separate compliance checks (Eli, 2026-11-03).
"""
from __future__ import annotations

import datetime as dt
import json
from pathlib import Path
from typing import Any

from . import character, config, fanvue_strategist, ig_to_fanvue, state

PROMOS_STATE = config.DATA / "ig_promotions.json"
CAMPAIGN_HORIZON_DAYS = 30

# How many days before a Fanvue release the Instagram promo push starts.
PROMO_LEAD_DAYS = 3

CAMPAIGN_TYPES = {
    "fanvue_release": "New PPV set or subscriber-only post drops on Fanvue",
    "subscriber_milestone": "Subscriber milestone reached (50 / 100 / 250 ...)",
    "intro_offer": "Limited-time introductory subscription discount",
    "themed_collection": "Exclusive themed content collection (holiday / event)",
}

# Tasteful, Instagram-compliant CTA copy per campaign kind + surface.
# Never mentions prices in captions (Nova voice rule). Always points to
# "link in bio" rather than dropping raw URLs in captions/comments.
CTA_COPY = {
    "fanvue_release": {
        "reel": "Full set drops on Fanvue — link in bio for early access 🖤",
        "story": "Subscribers see the full set first — link in bio 🖤",
        "feed": "The full gallery is waiting on Fanvue. Link in bio. 🖤",
        "bio": "Subscribe on Fanvue — link below 🖤",
        "highlight": "Fanvue exclusives — link in bio 🖤",
    },
    "subscriber_milestone": {
        "reel": "Thank you for making this possible — subscriber-only drops continue on Fanvue 🖤",
        "story": "Milestone celebration — subscriber thank-you set on Fanvue 🖤",
        "feed": "Grateful for every one of you. Subscriber exclusives live on Fanvue 🖤",
        "bio": "Join the Fanvue family — link in bio 🖤",
        "highlight": "Subscriber exclusives on Fanvue 🖤",
    },
    "intro_offer": {
        "reel": "Limited-time welcome offer on Fanvue — link in bio 🖤",
        "story": "Introductory pricing for new subscribers — link in bio 🖤",
        "feed": "Welcome offer running now on Fanvue — link in bio 🖤",
        "bio": "Introductory Fanvue offer — link below 🖤",
        "highlight": "Fanvue welcome offer — link in bio 🖤",
    },
    "themed_collection": {
        "reel": "A new themed collection lands on Fanvue — link in bio for the full set 🖤",
        "story": "Themed drop, subscribers only — link in bio 🖤",
        "feed": "The complete themed collection is Fanvue-exclusive. Link in bio 🖤",
        "bio": "Fanvue themed collections — link below 🖤",
        "highlight": "Themed collections on Fanvue 🖤",
    },
}

# Cliffhanger teaser copy per campaign kind (curiosity, never the full reveal).
TEASER_COPY = {
    "fanvue_release": "Something new is coming to Fanvue this week — subscribers see it first.",
    "subscriber_milestone": "A thank-you gift for my subscribers is landing on Fanvue.",
    "intro_offer": "A welcome offer is waiting for new subscribers on Fanvue.",
    "themed_collection": "An exclusive themed collection is dropping on Fanvue — subscribers only.",
}

AI_DISCLOSURE = "AI-generated character · stay cursed 🖤"


class PromoCampaign:
    """One coordinated Instagram->Fanvue campaign."""

    def __init__(self, campaign_id: str, kind: str, title: str,
                 start_date: dt.date, end_date: dt.date,
                 fanvue_slot_id: str | None = None,
                 surfaces: list[str] | None = None):
        self.id = campaign_id
        self.kind = kind
        self.title = title
        self.start_date = start_date
        self.end_date = end_date
        self.fanvue_slot_id = fanvue_slot_id
        self.surfaces = surfaces or ["reel", "story", "bio"]
        self.status = "planned"
        self.created_at = state.iso()
        self.instagram_slots: list[str] = []  # ids of IG slots tied to this campaign

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "kind": self.kind,
            "title": self.title,
            "start_date": self.start_date.isoformat(),
            "end_date": self.end_date.isoformat(),
            "fanvue_slot_id": self.fanvue_slot_id,
            "surfaces": self.surfaces,
            "status": self.status,
            "instagram_slots": self.instagram_slots,
            "created_at": self.created_at,
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "PromoCampaign":
        c = cls(
            d["id"], d["kind"], d["title"],
            dt.date.fromisoformat(d["start_date"]),
            dt.date.fromisoformat(d["end_date"]),
            d.get("fanvue_slot_id"),
            d.get("surfaces", ["reel", "story", "bio"]),
        )
        c.status = d.get("status", "planned")
        c.instagram_slots = d.get("instagram_slots", [])
        c.created_at = d.get("created_at", state.iso())
        return c

    def utm_links(self, attribution: ig_to_fanvue.IGFanvueAttribution) -> list[dict[str, Any]]:
        """Register (or return existing) UTM-tagged Fanvue links for each surface."""
        links = []
        for surface in self.surfaces:
            content = f"{self.kind}-{surface}"
            link = attribution.register_link(
                campaign_id=self.id,
                medium=surface,
                content=content,
                surface=surface,
            )
            links.append(link)
        return links

    def cta_for(self, surface: str) -> str:
        return CTA_COPY.get(self.kind, {}).get(surface, "Link in bio — Fanvue 🖤")

    def teaser(self) -> str:
        return TEASER_COPY.get(self.kind, TEASER_COPY["fanvue_release"])


class PromotionsStrategist:
    """Builds and maintains the rolling 30-day IG->Fanvue promotional calendar."""

    def __init__(self) -> None:
        self.state_path = PROMOS_STATE
        self.attribution = ig_to_fanvue.IGFanvueAttribution()

    # -- persistence -----------------------------------------------------------

    def load(self) -> dict[str, Any]:
        return state.read(
            PROMOS_STATE, {"campaigns": [], "generated_at": None}
        )

    def save(self, data: dict[str, Any]) -> None:
        state.write_atomic(PROMOS_STATE, data)

    # -- campaign construction -------------------------------------------------

    def _fanvue_releases(self) -> list[dict[str, Any]]:
        """Upcoming Fanvue slots within the horizon that look like releases."""
        cal = fanvue_strategist.FanvueContentStrategist().load_calendar()
        now = state.now().date()
        horizon = now + dt.timedelta(days=CAMPAIGN_HORIZON_DAYS)
        out = []
        for s in cal.get("slots", []):
            when = dt.datetime.fromisoformat(s["scheduled_at"]).date()
            if when < now or when > horizon:
                continue
            # Only calendar slots that are still planned (not yet posted) need a promo.
            if s.get("status") != "planned":
                continue
            out.append(s)
        return out

    def _campaign_id(self, kind: str, day: dt.date, suffix: str | None = None) -> str:
        base = f"promo-{kind}-{day.isoformat()}"
        return f"{base}-{suffix}" if suffix else base

    def _already_exists(self, data: dict[str, Any], campaign_id: str) -> bool:
        return any(c["id"] == campaign_id for c in data.get("campaigns", []))

    # -- calendar build --------------------------------------------------------

    def build(self, today: dt.date | None = None) -> dict[str, Any]:
        """Rolling 30-day promotional calendar coordinating IG surfaces with
        Fanvue releases, milestones, intro offers and themed collections."""
        today = today or state.now().date()
        horizon = today + dt.timedelta(days=CAMPAIGN_HORIZON_DAYS)
        data = self.load()
        campaigns = {c["id"]: PromoCampaign.from_dict(c) for c in data.get("campaigns", [])}

        added = 0

        # 1. Fanvue-release campaigns: one per upcoming Fanvue release slot.
        for slot in self._fanvue_releases():
            day = dt.datetime.fromisoformat(slot["scheduled_at"]).date()
            promo_day = day - dt.timedelta(days=PROMO_LEAD_DAYS)
            if promo_day < today:
                promo_day = today
            campaign_id = self._campaign_id("fanvue_release", day, slot["id"])
            if self._already_exists(data, campaign_id):
                continue
            c = PromoCampaign(
                campaign_id=campaign_id,
                kind="fanvue_release",
                title=f"Fanvue release — {slot.get('pillar','')} / {slot.get('theme','')}",
                start_date=promo_day,
                end_date=day,
                fanvue_slot_id=slot["id"],
                surfaces=["reel", "story", "bio"],
            )
            campaigns[campaign_id] = c
            added += 1
            state.log(f"ig_promotions: campaign {campaign_id} for Fanvue slot {slot['id']}")

        # 2. Intro offer campaign (once per horizon, early window).
        intro_id = self._campaign_id("intro_offer", today)
        if not self._already_exists(data, intro_id):
            end = today + dt.timedelta(days=14)
            c = PromoCampaign(
                campaign_id=intro_id,
                kind="intro_offer",
                title="Limited-time introductory Fanvue subscription offer",
                start_date=today,
                end_date=min(end, horizon),
                surfaces=["reel", "story", "bio", "highlight"],
            )
            campaigns[intro_id] = c
            added += 1
            state.log(f"ig_promotions: campaign {intro_id} intro offer")

        # 3. Themed collection campaign (one per horizon, mid window).
        themed_id = self._campaign_id("themed_collection", today + dt.timedelta(days=10))
        if not self._already_exists(data, themed_id):
            end = today + dt.timedelta(days=20)
            c = PromoCampaign(
                campaign_id=themed_id,
                kind="themed_collection",
                title="Exclusive Fanvue themed collection",
                start_date=today + dt.timedelta(days=10),
                end_date=min(end, horizon),
                surfaces=["reel", "story", "bio"],
            )
            campaigns[themed_id] = c
            added += 1
            state.log(f"ig_promotions: campaign {themed_id} themed collection")

        # 4. Subscriber milestone campaigns (data-driven: scheduled when a
        #    milestone is projected). Placed as placeholders; Eli approves the
        #    actual milestone price/offer via the fanvue_pricing_change gate.
        subs = self._projected_subscribers()
        for milestone in (50, 100, 250):
            if subs < milestone:
                # project roughly when we might reach it; place at end of horizon
                m_day = today + dt.timedelta(days=28)
                mid = self._campaign_id("subscriber_milestone", m_day)
                if not self._already_exists(data, mid):
                    c = PromoCampaign(
                        campaign_id=mid,
                        kind="subscriber_milestone",
                        title=f"{milestone} Fanvue subscribers — thank-you drop",
                        start_date=m_day,
                        end_date=m_day,
                        surfaces=["story", "bio"],
                    )
                    campaigns[mid] = c
                    added += 1
                    state.log(f"ig_promotions: campaign {mid} subscriber milestone placeholder")
                break  # only the next unreached milestone

        data["campaigns"] = [c.to_dict() for c in campaigns.values()]
        data["generated_at"] = state.iso()
        self.save(data)
        return {"added": added, "total": len(data["campaigns"]),
                "campaigns": data["campaigns"]}

    # -- helpers ---------------------------------------------------------------

    def _projected_subscribers(self) -> int:
        """Current Fanvue subscriber count from the last real report, if any."""
        try:
            report_path = config.DATA / "fanvue_weekly_report.json"
            report = json.loads(report_path.read_text(encoding="utf8"))
            return int(report.get("subscribers", {}).get("count_first_page") or 0)
        except Exception:
            return 0

    def active(self, as_of: dt.date | None = None) -> list[dict[str, Any]]:
        """Campaigns active on the given date (default: today)."""
        as_of = as_of or state.now().date()
        data = self.load()
        out = []
        for c in data.get("campaigns", []):
            start = dt.date.fromisoformat(c["start_date"])
            end = dt.date.fromisoformat(c["end_date"])
            if start <= as_of <= end:
                out.append(c)
        return out

    def links_for_active(self, as_of: dt.date | None = None) -> list[dict[str, Any]]:
        """UTM links for all campaigns active on a date, for use in IG captions."""
        out = []
        for c in self.active(as_of):
            pc = PromoCampaign.from_dict(c)
            out.append({
                "campaign_id": pc.id,
                "kind": pc.kind,
                "title": pc.title,
                "cta": pc.cta_for("bio"),
                "teaser": pc.teaser(),
                "links": pc.utm_links(self.attribution),
            })
        return out


# module-level helpers so pipeline.py can import functions directly
_strategist: PromotionsStrategist | None = None


def _get_strategist() -> PromotionsStrategist:
    global _strategist
    if _strategist is None:
        _strategist = PromotionsStrategist()
    return _strategist


def build_calendar(today: dt.date | None = None) -> dict[str, Any]:
    return _get_strategist().build(today)


def load_calendar() -> dict[str, Any]:
    return _get_strategist().load()


def save_calendar(data: dict[str, Any]) -> None:
    _get_strategist().save(data)


def active_campaigns(as_of: dt.date | None = None) -> list[dict[str, Any]]:
    return _get_strategist().active(as_of)


def links_for_active(as_of: dt.date | None = None) -> list[dict[str, Any]]:
    return _get_strategist().links_for_active(as_of)
