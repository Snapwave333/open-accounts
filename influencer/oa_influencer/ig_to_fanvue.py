"""IG-to-Fanvue attribution: UTM-tagged links, campaign IDs, conversion analytics.

Every Instagram surface that links to Fanvue (Reel captions, Story link stickers,
bio link) carries a UTM-tagged URL with a campaign ID. Clicks are logged locally
so the weekly report can attribute Fanvue visits and paid subs to specific Reels,
Stories, themes and CTAs.

Two-platform rule (Eli, 2026-11-03): Instagram is the top of the funnel; Fanvue is
where the monetized content lives. This module never simulates Fanvue activity —
it only tracks outbound intent (clicks to Fanvue) and correlates with real Fanvue
subscriber/earnings data where the API exposes it.
"""
from __future__ import annotations

import datetime as dt
import json
from pathlib import Path
from typing import Any
from urllib.parse import urlencode, urlparse, parse_qs, urlunparse

from . import config, state

LINKS_STATE = config.DATA / "ig_fanvue_links.json"
CLICKS_STATE = config.DATA / "ig_fanvue_clicks.json"
CONVERSIONS_STATE = config.DATA / "ig_fanvue_conversions.json"

FANVUE_SUBSCRIPTION_URL = "https://fanvue.com/nova_a_aa_"
WALLPAPER_URL = "https://nova-vexley.com/wallpapers"  # placeholder until a store exists

UTM_SOURCE = "instagram"


class IGFanvueAttribution:
    """UTM link registry + click/conversion logger for IG->Fanvue attribution."""

    def __init__(self) -> None:
        self.links_path = LINKS_STATE
        self.clicks_path = CLICKS_STATE
        self.conversions_path = CONVERSIONS_STATE

    # -- persistence -----------------------------------------------------------

    def _load_links(self) -> dict[str, Any]:
        return state.read(self.links_path, {"links": []})

    def _save_links(self, data: dict[str, Any]) -> None:
        state.write_atomic(self.links_path, data)

    def _load_clicks(self) -> dict[str, Any]:
        return state.read(self.clicks_path, {"clicks": []})

    def _save_clicks(self, data: dict[str, Any]) -> None:
        state.write_atomic(self.clicks_path, data)

    def _load_conversions(self) -> dict[str, Any]:
        return state.read(self.conversions_path, {"conversions": []})

    def _save_conversions(self, data: dict[str, Any]) -> None:
        state.write_atomic(self.conversions_path, data)

    # -- link building ---------------------------------------------------------

    @staticmethod
    def build_fanvue_url(campaign_id: str, medium: str, content: str) -> str:
        """UTM-tagged Fanvue subscription URL for a given campaign/surface."""
        params = {
            "utm_source": UTM_SOURCE,
            "utm_medium": medium,
            "utm_campaign": campaign_id,
            "utm_content": content,
        }
        parts = urlparse(FANVUE_SUBSCRIPTION_URL)
        qs = parse_qs(parts.query)
        qs.update(params)
        return urlunparse((
            parts.scheme, parts.netloc, parts.path, parts.params,
            urlencode(qs, doseq=True), parts.fragment,
        ))

    @staticmethod
    def build_wallpaper_url(campaign_id: str, medium: str, content: str) -> str:
        """UTM-tagged wallpaper-shop URL."""
        params = {
            "utm_source": UTM_SOURCE,
            "utm_medium": medium,
            "utm_campaign": campaign_id,
            "utm_content": content,
        }
        parts = urlparse(WALLPAPER_URL)
        qs = parse_qs(parts.query)
        qs.update(params)
        return urlunparse((
            parts.scheme, parts.netloc, parts.path, parts.params,
            urlencode(qs, doseq=True), parts.fragment,
        ))

    def register_link(
        self, campaign_id: str, medium: str, content: str, surface: str,
        slot_id: str | None = None, url: str | None = None,
    ) -> dict[str, Any]:
        """Register a UTM-tagged link so clicks can be attributed back to it."""
        data = self._load_links()
        link_url = url or self.build_fanvue_url(campaign_id, medium, content)
        entry = {
            "campaign_id": campaign_id,
            "medium": medium,
            "content": content,
            "surface": surface,          # reel | story | bio | pinned | highlight
            "slot_id": slot_id,
            "url": link_url,
            "registered_at": state.iso(),
            "clicks": 0,
        }
        data["links"].append(entry)
        self._save_links(data)
        state.log(f"ig_fanvue: registered link {campaign_id} ({surface}) -> {link_url}")
        return entry

    def link_for_slot(self, slot: dict) -> dict[str, Any]:
        """Convenience: register + return the Fanvue link for an IG slot (Reel/Story)."""
        medium = slot.get("utm_medium", "post")
        content = slot.get("utm_content", f"{slot.get('pillar','general')}-{slot.get('kind','post')}")
        campaign_id = slot.get("utm_campaign") or f"{slot.get('platform','ig')}-{slot.get('id','unknown')}"
        return self.register_link(
            campaign_id=campaign_id,
            medium=medium,
            content=content,
            surface=slot.get("kind", "post"),
            slot_id=slot.get("id"),
        )

    # -- click logging ---------------------------------------------------------

    def record_click(
        self, campaign_id: str, medium: str, content: str, surface: str,
        referrer: str | None = None, slot_id: str | None = None,
    ) -> dict[str, Any]:
        """Log a click on a UTM-tagged Fanvue link (from link-in-bio taps, etc.)."""
        data = self._load_clicks()
        entry = {
            "campaign_id": campaign_id,
            "medium": medium,
            "content": content,
            "surface": surface,
            "slot_id": slot_id,
            "referrer": referrer,
            "clicked_at": state.iso(),
        }
        data["clicks"].append(entry)
        self._save_clicks(data)
        # increment link click counter
        links = self._load_links()
        for link in links["links"]:
            if link["campaign_id"] == campaign_id and link["medium"] == medium:
                link["clicks"] = int(link.get("clicks", 0)) + 1
        self._save_links(links)
        state.log(f"ig_fanvue: click {campaign_id} via {surface} ({referrer or 'direct'})")
        return entry

    # -- conversion recording --------------------------------------------------

    def record_conversion(
        self, campaign_id: str, medium: str, content: str, surface: str,
        kind: str, amount_cents: int | None = None, fanvue_user_id: str | None = None,
    ) -> dict[str, Any]:
        """Record a Fanvue conversion attributed to an IG campaign (paid sub, PPV unlock, etc.).

        kind: "subscription" | "ppv_unlock" | "tip" | "renewal"
        """
        data = self._load_conversions()
        entry = {
            "campaign_id": campaign_id,
            "medium": medium,
            "content": content,
            "surface": surface,
            "kind": kind,
            "amount_cents": amount_cents,
            "fanvue_user_id": fanvue_user_id,
            "converted_at": state.iso(),
        }
        data["conversions"].append(entry)
        self._save_conversions(data)
        state.log(f"ig_fanvue: conversion {kind} {campaign_id} via {surface} amount={amount_cents}")
        return entry

    # -- analytics -------------------------------------------------------------

    def campaign_performance(self, days: int | None = None) -> list[dict[str, Any]]:
        """Aggregate clicks and conversions per campaign, optionally within a lookback window."""
        links = self._load_links().get("links", [])
        clicks = self._load_clicks().get("clicks", [])
        conversions = self._load_conversions().get("conversions", [])

        cutoff = None
        if days is not None:
            cutoff = state.now() - dt.timedelta(days=days)

        def in_window(ts: str) -> bool:
            if cutoff is None:
                return True
            try:
                return dt.datetime.fromisoformat(ts) >= cutoff
            except ValueError:
                return False

        by_campaign: dict[str, dict[str, Any]] = {}
        for link in links:
            key = f"{link['campaign_id']}|{link['medium']}"
            by_campaign[key] = {
                "campaign_id": link["campaign_id"],
                "medium": link["medium"],
                "content": link["content"],
                "surface": link["surface"],
                "slot_id": link.get("slot_id"),
                "url": link["url"],
                "registered_at": link["registered_at"],
                "clicks": int(link.get("clicks", 0)),
                "conversions": 0,
                "revenue_cents": 0,
            }

        for click in clicks:
            if not in_window(click["clicked_at"]):
                continue
            key = f"{click['campaign_id']}|{click['medium']}"
            if key in by_campaign:
                by_campaign[key]["clicks"] += 1

        for conv in conversions:
            if not in_window(conv["converted_at"]):
                continue
            key = f"{conv['campaign_id']}|{conv['medium']}"
            if key in by_campaign:
                by_campaign[key]["conversions"] += 1
                if conv.get("amount_cents"):
                    by_campaign[key]["revenue_cents"] += int(conv["amount_cents"])

        out = sorted(by_campaign.values(), key=lambda c: c["clicks"], reverse=True)
        for c in out:
            c["conversion_rate"] = (
                round(c["conversions"] / c["clicks"] * 100, 2) if c["clicks"] else 0.0
            )
            c["revenue_usd"] = round(c["revenue_cents"] / 100, 2)
        return out

    def surface_performance(self, days: int | None = None) -> list[dict[str, Any]]:
        """Aggregate clicks/conversions by surface (reel | story | bio | pinned | highlight)."""
        by_surface: dict[str, dict[str, Any]] = {}
        clicks = self._load_clicks().get("clicks", [])
        conversions = self._load_conversions().get("conversions", [])
        cutoff = None
        if days is not None:
            cutoff = state.now() - dt.timedelta(days=days)

        def in_window(ts: str) -> bool:
            if cutoff is None:
                return True
            try:
                return dt.datetime.fromisoformat(ts) >= cutoff
            except ValueError:
                return False

        for click in clicks:
            if not in_window(click["clicked_at"]):
                continue
            s = click["surface"]
            by_surface.setdefault(s, {"clicks": 0, "conversions": 0, "revenue_cents": 0})
            by_surface[s]["clicks"] += 1

        for conv in conversions:
            if not in_window(conv["converted_at"]):
                continue
            s = conv["surface"]
            by_surface.setdefault(s, {"clicks": 0, "conversions": 0, "revenue_cents": 0})
            by_surface[s]["conversions"] += 1
            if conv.get("amount_cents"):
                by_surface[s]["revenue_cents"] += int(conv["amount_cents"])

        out = []
        for s, v in sorted(by_surface.items(), key=lambda kv: kv[1]["clicks"], reverse=True):
            v["conversion_rate"] = round(v["conversions"] / v["clicks"] * 100, 2) if v["clicks"] else 0.0
            v["revenue_usd"] = round(v["revenue_cents"] / 100, 2)
            out.append({"surface": s, **v})
        return out

    def total_clicks(self, days: int | None = None) -> int:
        clicks = self._load_clicks().get("clicks", [])
        if days is None:
            return len(clicks)
        cutoff = state.now() - dt.timedelta(days=days)
        n = 0
        for c in clicks:
            try:
                if dt.datetime.fromisoformat(c["clicked_at"]) >= cutoff:
                    n += 1
            except ValueError:
                continue
        return n

    def total_conversions(self, days: int | None = None) -> dict[str, Any]:
        conversions = self._load_conversions().get("conversions", [])
        if days is None:
            return {"count": len(conversions), "revenue_cents": sum(int(c.get("amount_cents") or 0) for c in conversions)}
        cutoff = state.now() - dt.timedelta(days=days)
        count = 0
        revenue = 0
        for c in conversions:
            try:
                if dt.datetime.fromisoformat(c["converted_at"]) >= cutoff:
                    count += 1
                    revenue += int(c.get("amount_cents") or 0)
            except ValueError:
                continue
        return {"count": count, "revenue_cents": revenue, "revenue_usd": round(revenue / 100, 2)}


# module-level helpers
_attribution: IGFanvueAttribution | None = None


def _get_attribution() -> IGFanvueAttribution:
    global _attribution
    if _attribution is None:
        _attribution = IGFanvueAttribution()
    return _attribution


def build_fanvue_url(campaign_id: str, medium: str, content: str) -> str:
    return IGFanvueAttribution.build_fanvue_url(campaign_id, medium, content)


def build_wallpaper_url(campaign_id: str, medium: str, content: str) -> str:
    return IGFanvueAttribution.build_wallpaper_url(campaign_id, medium, content)


def register_link(*args, **kwargs) -> dict[str, Any]:
    return _get_attribution().register_link(*args, **kwargs)


def link_for_slot(slot: dict) -> dict[str, Any]:
    return _get_attribution().link_for_slot(slot)


def record_click(*args, **kwargs) -> dict[str, Any]:
    return _get_attribution().record_click(*args, **kwargs)


def record_conversion(*args, **kwargs) -> dict[str, Any]:
    return _get_attribution().record_conversion(*args, **kwargs)


def campaign_performance(days: int | None = None) -> list[dict[str, Any]]:
    return _get_attribution().campaign_performance(days)


def surface_performance(days: int | None = None) -> list[dict[str, Any]]:
    return _get_attribution().surface_performance(days)


def total_clicks(days: int | None = None) -> int:
    return _get_attribution().total_clicks(days)


def total_conversions(days: int | None = None) -> dict[str, Any]:
    return _get_attribution().total_conversions(days)
